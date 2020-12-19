//! Fully connected overlay network
//!
//! The *small network* is an overlay network where each node participating is connected to every
//! other node on the network. The *small* portion of the name stems from the fact that this
//! approach is not scalable, as it requires at least $O(n)$ network connections and broadcast will
//! result in $O(n^2)$ messages.
//!
//! # Node IDs
//!
//! Each node has a self-generated node ID based on its self-signed TLS certificate. Whenever a
//! connection is made to another node, it verifies the "server"'s certificate to check that it
//! connected to the correct node and sends its own certificate during the TLS handshake,
//! establishing identity.
//!
//! # Messages and payloads
//!
//! The network itself is best-effort, during regular operation, no messages should be lost.
//!
//! # Connection
//!
//! Every node has an ID and a public listening address. The objective of each node is to constantly
//! maintain an outgoing connection to each other node (and thus have an incoming connection from
//! these nodes as well).
//!
//! Any incoming connection is strictly read from, while any outgoing connection is strictly used
//! for sending messages.
//!
//! Nodes gossip their public listening addresses periodically, and on learning of a new address,
//! a node will try to establish an outgoing connection.
//!
//! On losing an incoming or outgoing connection for a given peer, the other connection is closed.
//! No explicit reconnect is attempted. Instead, if the peer is still online, the normal gossiping
//! process will cause both peers to connect again.

mod config;
mod error;
mod event;
mod gossiped_address;
mod message;
#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    convert::Infallible,
    env,
    fmt::{self, Debug, Display, Formatter},
    io,
    net::{SocketAddr, TcpListener},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::Context;
use datasize::DataSize;
use futures::{
    future::{select, BoxFuture, Either},
    stream::{SplitSink, SplitStream},
    FutureExt, SinkExt, StreamExt,
};
use openssl::pkey;
use pkey::{PKey, Private};
use rand::seq::IteratorRandom;
use serde::{de::DeserializeOwned, Serialize};
use tokio::{
    net::TcpStream,
    sync::{
        mpsc::{self, UnboundedReceiver, UnboundedSender},
        watch,
    },
    task::JoinHandle,
};
use tokio_openssl::SslStream;
use tokio_serde::{formats::SymmetricalMessagePack, SymmetricallyFramed};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{debug, error, info, trace, warn};

use self::error::Result;
pub(crate) use self::{event::Event, gossiped_address::GossipedAddress, message::Message};
use crate::{
    components::{network::ENABLE_LIBP2P_ENV_VAR, Component},
    crypto::hash::Digest,
    effect::{
        announcements::NetworkAnnouncement,
        requests::{NetworkInfoRequest, NetworkRequest},
        EffectBuilder, EffectExt, EffectResultExt, Effects,
    },
    fatal,
    reactor::{EventQueueHandle, Finalize, QueueKind},
    tls::{self, TlsCert},
    types::NodeId,
    utils, NodeRng,
};
pub use config::Config;
pub use error::Error;

const MAX_ASYMMETRIC_CONNECTION_SEEN: u16 = 3;

#[derive(DataSize, Debug)]
pub(crate) struct OutgoingConnection<P> {
    #[data_size(skip)] // Unfortunately, there is no way to inspect an `UnboundedSender`.
    sender: UnboundedSender<Message<P>>,
    peer_address: SocketAddr,

    // for keeping track of connection asymmetry, tracking the number of times we've seen this
    // connection be asymmetric.
    times_seen_asymmetric: u16,
}

#[derive(DataSize, Debug)]
pub(crate) struct IncomingConnection {
    peer_address: SocketAddr,

    // for keeping track of connection asymmetry, tracking the number of times we've seen this
    // connection be asymmetric.
    times_seen_asymmetric: u16,
}

#[derive(DataSize)]
pub(crate) struct SmallNetwork<REv, P>
where
    REv: 'static,
{
    /// Server certificate.
    certificate: Arc<TlsCert>,
    /// Server secret key.
    secret_key: Arc<PKey<Private>>,
    /// Our public listening address.
    public_address: SocketAddr,
    /// Our node ID,
    our_id: NodeId,
    /// If we connect to ourself, this flag is set to true.
    is_bootstrap_node: bool,
    /// Handle to event queue.
    event_queue: EventQueueHandle<REv>,

    /// Incoming network connection addresses.
    incoming: HashMap<NodeId, IncomingConnection>,
    /// Outgoing network connections' messages.
    outgoing: HashMap<NodeId, OutgoingConnection<P>>,

    /// List of addresses which this node will avoid connecting to.
    blocklist: HashSet<SocketAddr>,

    /// Pending outgoing connections: ones for which we are currently trying to make a connection.
    pending: HashSet<SocketAddr>,
    /// The interval between each fresh round of gossiping the node's public listening address.
    gossip_interval: Duration,
    /// An index for an iteration of gossiping our own public listening address.  This is
    /// incremented by 1 on each iteration, and wraps on overflow.
    next_gossip_address_index: u32,
    /// The hash of the chainspec.  We only remain connected to peers with the same
    /// `genesis_config_hash` as us.
    genesis_config_hash: Digest,
    /// Channel signaling a shutdown of the small network.
    // Note: This channel is closed when `SmallNetwork` is dropped, signalling the receivers that
    // they should cease operation.
    #[data_size(skip)]
    shutdown_sender: Option<watch::Sender<()>>,
    /// A clone of the receiver is passed to the message reader for all new incoming connections in
    /// order that they can be gracefully terminated.
    #[data_size(skip)]
    shutdown_receiver: watch::Receiver<()>,
    /// Flag to indicate the server has stopped running.
    is_stopped: Arc<AtomicBool>,
    /// Join handle for the server thread.
    server_join_handle: Option<JoinHandle<()>>,
}

impl<REv, P> SmallNetwork<REv, P>
where
    P: Serialize + DeserializeOwned + Clone + Debug + Display + Send + 'static,
    REv: Send + From<Event<P>> + From<NetworkAnnouncement<NodeId, P>>,
{
    /// Creates a new small network component instance.
    ///
    /// If `notify` is set to `false`, no systemd notifications will be sent, regardless of
    /// configuration.
    #[allow(clippy::type_complexity)]
    pub(crate) fn new(
        event_queue: EventQueueHandle<REv>,
        cfg: Config,
        genesis_config_hash: Digest,
        notify: bool,
    ) -> Result<(SmallNetwork<REv, P>, Effects<Event<P>>)> {
        // Assert we have at least one known address in the config.
        if cfg.known_addresses.is_empty() {
            warn!("no known addresses provided via config");
            return Err(Error::InvalidConfig);
        }

        let mut public_address =
            utils::resolve_address(&cfg.public_address).map_err(Error::ResolveAddr)?;

        // First, we generate the TLS keys.
        let (cert, secret_key) = tls::generate_node_cert().map_err(Error::CertificateGeneration)?;
        let certificate = Arc::new(tls::validate_cert(cert).map_err(Error::OwnCertificateInvalid)?);
        let our_id = NodeId::from(certificate.public_key_fingerprint());

        // If the env var "CASPER_ENABLE_LIBP2P" is defined, exit without starting the server.
        if env::var(ENABLE_LIBP2P_ENV_VAR).is_ok() {
            let model = SmallNetwork {
                certificate,
                secret_key: Arc::new(secret_key),
                public_address,
                our_id,
                is_bootstrap_node: false,
                event_queue,
                incoming: HashMap::new(),
                outgoing: HashMap::new(),
                pending: HashSet::new(),
                blocklist: HashSet::new(),
                gossip_interval: cfg.gossip_interval,
                next_gossip_address_index: 0,
                genesis_config_hash,
                shutdown_sender: None,
                shutdown_receiver: watch::channel(()).1,
                server_join_handle: None,
                is_stopped: Arc::new(AtomicBool::new(true)),
            };
            return Ok((model, Effects::new()));
        }

        // We can now create a listener.
        let bind_address = utils::resolve_address(&cfg.bind_address).map_err(Error::ResolveAddr)?;
        let listener = TcpListener::bind(bind_address)
            .map_err(|error| Error::ListenerCreation(error, bind_address))?;

        // Once the port has been bound, we can notify systemd if instructed to do so.
        if notify {
            if cfg.systemd_support {
                if sd_notify::booted().map_err(Error::SystemD)? {
                    info!("notifying systemd that the network is ready to receive connections");
                    sd_notify::notify(true, &[sd_notify::NotifyState::Ready])
                        .map_err(Error::SystemD)?;
                } else {
                    warn!("systemd_support enabled but not booted with systemd, ignoring");
                }
            } else {
                debug!("systemd_support disabled, not notifying");
            }
        }
        let local_address = listener.local_addr().map_err(Error::ListenerAddr)?;

        // Substitute the actually bound port if set to 0.
        if public_address.port() == 0 {
            public_address.set_port(local_address.port());
        }

        // Run the server task.
        // We spawn it ourselves instead of through an effect to get a hold of the join handle,
        // which we need to shutdown cleanly later on.
        info!(%local_address, %public_address, "{}: starting server background task", our_id);
        let (server_shutdown_sender, server_shutdown_receiver) = watch::channel(());
        let shutdown_receiver = server_shutdown_receiver.clone();
        let server_join_handle = tokio::spawn(server_task(
            event_queue,
            tokio::net::TcpListener::from_std(listener).map_err(Error::ListenerConversion)?,
            server_shutdown_receiver,
            our_id,
        ));

        let our_id = NodeId::from(certificate.public_key_fingerprint());
        let mut model = SmallNetwork {
            certificate,
            secret_key: Arc::new(secret_key),
            public_address,
            our_id,
            is_bootstrap_node: false,
            event_queue,
            incoming: HashMap::new(),
            outgoing: HashMap::new(),
            pending: HashSet::new(),
            blocklist: HashSet::new(),
            gossip_interval: cfg.gossip_interval,
            next_gossip_address_index: 0,
            genesis_config_hash,
            shutdown_sender: Some(server_shutdown_sender),
            shutdown_receiver,
            server_join_handle: Some(server_join_handle),
            is_stopped: Arc::new(AtomicBool::new(false)),
        };

        // Bootstrap process.
        let mut effects = Effects::new();

        for address in &cfg.known_addresses {
            match utils::resolve_address(address) {
                Ok(known_address) => {
                    model.pending.insert(known_address);

                    // We successfully resolved an address, add an effect to connect to it.
                    effects.extend(
                        connect_outgoing(
                            known_address,
                            Arc::clone(&model.certificate),
                            Arc::clone(&model.secret_key),
                            Arc::clone(&model.is_stopped),
                        )
                        .result(
                            move |(peer_id, transport)| Event::OutgoingEstablished {
                                peer_id,
                                transport,
                            },
                            move |error| Event::BootstrappingFailed {
                                peer_address: known_address,
                                error,
                            },
                        ),
                    );
                }
                Err(err) => {
                    warn!("failed to resolve known address {}: {}", address, err);
                }
            }
        }

        let effect_builder = EffectBuilder::new(event_queue);

        // If there are no pending connections, we failed to resolve any.
        if model.pending.is_empty() && !cfg.known_addresses.is_empty() {
            effects.extend(fatal!(
                effect_builder,
                "was given known addresses, but failed to resolve any of them"
            ));
        } else {
            // Start broadcasting our public listening address.
            effects.extend(model.gossip_our_address(effect_builder));
        }

        Ok((model, effects))
    }

    /// Queues a message to be sent to all nodes.
    fn broadcast_message(&self, msg: Message<P>) {
        for peer_id in self.outgoing.keys() {
            self.send_message(peer_id.clone(), msg.clone());
        }
    }

    /// Queues a message to `count` random nodes on the network.
    fn gossip_message(
        &self,
        rng: &mut NodeRng,
        msg: Message<P>,
        count: usize,
        exclude: HashSet<NodeId>,
    ) -> HashSet<NodeId> {
        let peer_ids = self
            .outgoing
            .keys()
            .filter(|&peer_id| !exclude.contains(peer_id))
            .choose_multiple(rng, count);

        if peer_ids.len() != count {
            // TODO - set this to `warn!` once we are normally testing with networks large enough to
            //        make it a meaningful and infrequent log message.
            trace!(
                wanted = count,
                selected = peer_ids.len(),
                "{}: could not select enough random nodes for gossiping, not enough non-excluded \
                outgoing connections",
                self.our_id
            );
        }

        for &peer_id in &peer_ids {
            self.send_message(peer_id.clone(), msg.clone());
        }

        peer_ids.into_iter().cloned().collect()
    }

    /// Queues a message to be sent to a specific node.
    fn send_message(&self, dest: NodeId, msg: Message<P>) {
        // Try to send the message.
        if let Some(connection) = self.outgoing.get(&dest) {
            if let Err(msg) = connection.sender.send(msg) {
                // We lost the connection, but that fact has not reached us yet.
                warn!(%dest, ?msg, "{}: dropped outgoing message, lost connection", self.our_id);
            }
        } else {
            // We are not connected, so the reconnection is likely already in progress.
            debug!(%dest, ?msg, "{}: dropped outgoing message, no connection", self.our_id);
        }
    }

    fn handle_incoming_tls_handshake_completed(
        &mut self,
        effect_builder: EffectBuilder<REv>,
        result: Result<(NodeId, Transport)>,
        peer_address: SocketAddr,
    ) -> Effects<Event<P>> {
        match result {
            Ok((peer_id, transport)) => {
                // If we have connected to ourself, allow the connection to drop.
                if peer_id == self.our_id {
                    self.is_bootstrap_node = true;
                    debug!(
                        %peer_address,
                        local_address=?transport.get_ref().local_addr(),
                        "{}: connected incoming to ourself - closing connection",
                        self.our_id
                    );
                    return Effects::new();
                }

                // If the peer has already disconnected, allow the connection to drop.
                if let Err(error) = transport.get_ref().peer_addr() {
                    debug!(
                        %peer_address,
                        local_address=?transport.get_ref().local_addr(),
                        %error,
                        "{}: incoming connection dropped",
                        self.our_id
                    );
                    return Effects::new();
                }

                debug!(%peer_id, %peer_address, "{}: established incoming connection", self.our_id);
                // The sink is only used to send a single handshake message, then dropped.
                let (mut sink, stream) = framed::<P>(transport).split();
                let handshake = Message::Handshake {
                    genesis_config_hash: self.genesis_config_hash,
                };
                let mut effects = async move {
                    let _ = sink.send(handshake).await;
                }
                .ignore::<Event<P>>();

                let _ = self.incoming.insert(
                    peer_id.clone(),
                    IncomingConnection {
                        peer_address,
                        times_seen_asymmetric: 0,
                    },
                );

                // If the connection is now complete, announce the new peer before starting reader.
                effects.extend(self.check_connection_complete(effect_builder, peer_id.clone()));

                effects.extend(
                    message_reader(
                        self.event_queue,
                        stream,
                        self.shutdown_receiver.clone(),
                        self.our_id.clone(),
                        peer_id.clone(),
                    )
                    .event(move |result| Event::IncomingClosed {
                        result,
                        peer_id,
                        peer_address,
                    }),
                );

                effects
            }
            Err(err) => {
                warn!(%peer_address, %err, "{}: TLS handshake failed", self.our_id);
                Effects::new()
            }
        }
    }

    /// Sets up an established outgoing connection.
    fn setup_outgoing(
        &mut self,
        effect_builder: EffectBuilder<REv>,
        peer_id: NodeId,
        transport: Transport,
    ) -> Effects<Event<P>> {
        // This connection is send-only, we only use the sink.
        let peer_address = transport
            .get_ref()
            .peer_addr()
            .expect("should have peer address");

        if !self.pending.remove(&peer_address) {
            info!(
                %peer_address,
                "{}: this peer's incoming connection has dropped, so don't establish an outgoing",
                self.our_id
            );
            return Effects::new();
        }

        // If we have connected to ourself, allow the connection to drop.
        if peer_id == self.our_id {
            self.is_bootstrap_node = true;
            debug!(
                peer_address=?transport.get_ref().peer_addr(),
                local_address=?transport.get_ref().local_addr(),
                "{}: connected outgoing to ourself - closing connection",
                self.our_id,
            );
            return Effects::new();
        }

        // The stream is only used to receive a single handshake message and then dropped.
        let (sink, stream) = framed::<P>(transport).split();
        debug!(%peer_id, %peer_address, "{}: established outgoing connection", self.our_id);

        let (sender, receiver) = mpsc::unbounded_channel();
        let connection = OutgoingConnection {
            peer_address,
            sender,
            times_seen_asymmetric: 0,
        };
        if self.outgoing.insert(peer_id.clone(), connection).is_some() {
            // We assume that for a reconnect to have happened, the outgoing entry must have
            // been either non-existent yet or cleaned up by the handler of the connection
            // closing event. If this is not the case, an assumed invariant has been violated.
            error!(%peer_id, "{}: did not expect leftover channel in outgoing map", self.our_id);
        }

        let mut effects = self.check_connection_complete(effect_builder, peer_id.clone());

        let handshake = Message::Handshake {
            genesis_config_hash: self.genesis_config_hash,
        };
        let peer_id_cloned = peer_id.clone();
        effects.extend(
            message_sender(receiver, sink, handshake).event(move |result| Event::OutgoingFailed {
                peer_id: Some(peer_id),
                peer_address,
                error: result.err().map(Into::into),
            }),
        );
        effects.extend(
            handshake_reader(
                self.event_queue,
                stream,
                self.our_id.clone(),
                peer_id_cloned,
                peer_address,
            )
            .ignore::<Event<P>>(),
        );

        effects
    }

    fn handle_outgoing_lost(
        &mut self,
        effect_builder: EffectBuilder<REv>,
        peer_id: Option<NodeId>,
        peer_address: SocketAddr,
        error: Option<Error>,
    ) -> Effects<Event<P>> {
        let _ = self.pending.remove(&peer_address);

        if let Some(peer_id) = peer_id {
            if let Some(err) = error {
                warn!(%peer_id, %peer_address, %err, "{}: outgoing connection failed", self.our_id);
            } else {
                warn!(%peer_id, %peer_address, "{}: outgoing connection closed", self.our_id);
            }
            return self.remove(effect_builder, &peer_id, false);
        }

        // If we don't have the node ID passed in here, it was never added as an
        // outgoing connection, hence no need to call `self.remove()`.
        if let Some(err) = error {
            warn!(%peer_address, %err, "{}: outgoing connection failed", self.our_id);
        } else {
            warn!(%peer_address, "{}: outgoing connection closed", self.our_id);
        }

        Effects::new()
    }

    fn remove(
        &mut self,
        effect_builder: EffectBuilder<REv>,
        peer_id: &NodeId,
        add_to_blocklist: bool,
    ) -> Effects<Event<P>> {
        if let Some(incoming) = self.incoming.remove(&peer_id) {
            let _ = self.pending.remove(&incoming.peer_address);
        }
        if let Some(outgoing) = self.outgoing.remove(&peer_id) {
            if add_to_blocklist {
                self.blocklist.insert(outgoing.peer_address);
            }
        }
        self.terminate_if_isolated(effect_builder)
    }

    /// Gossips our public listening address, and schedules the next such gossip round.
    fn gossip_our_address(&mut self, effect_builder: EffectBuilder<REv>) -> Effects<Event<P>> {
        self.next_gossip_address_index = self.next_gossip_address_index.wrapping_add(1);
        let our_address = GossipedAddress::new(self.public_address, self.next_gossip_address_index);
        let mut effects = effect_builder
            .announce_gossip_our_address(our_address)
            .ignore();
        effects.extend(
            effect_builder
                .set_timeout(self.gossip_interval)
                .event(|_| Event::GossipOurAddress),
        );
        effects
    }

    /// Marks connections as asymmetric (only incoming or only outgoing) and removes them if they
    /// pass the upper limit for this. Connections that are symmetrical are reset to 0.
    fn enforce_symmetric_connections(
        &mut self,
        effect_builder: EffectBuilder<REv>,
    ) -> Effects<Event<P>> {
        let mut remove = Vec::new();
        for (node_id, conn) in self.incoming.iter_mut() {
            if !self.outgoing.contains_key(node_id) {
                if conn.times_seen_asymmetric >= MAX_ASYMMETRIC_CONNECTION_SEEN {
                    remove.push(node_id.clone());
                } else {
                    conn.times_seen_asymmetric += 1;
                }
            } else {
                conn.times_seen_asymmetric = 0;
            }
        }
        for (node_id, conn) in self.outgoing.iter_mut() {
            if !self.incoming.contains_key(node_id) {
                if conn.times_seen_asymmetric >= MAX_ASYMMETRIC_CONNECTION_SEEN {
                    remove.push(node_id.clone());
                } else {
                    conn.times_seen_asymmetric += 1;
                }
            } else {
                conn.times_seen_asymmetric = 0;
            }
        }
        let mut effects = Effects::new();
        for node_id in remove {
            effects.extend(self.remove(effect_builder, &node_id, true));
        }
        effects
    }

    /// Handles a received message.
    fn handle_message(
        &mut self,
        effect_builder: EffectBuilder<REv>,
        peer_id: NodeId,
        msg: Message<P>,
    ) -> Effects<Event<P>>
    where
        REv: From<NetworkAnnouncement<NodeId, P>>,
    {
        match msg {
            Message::Handshake {
                genesis_config_hash,
            } => {
                if genesis_config_hash != self.genesis_config_hash {
                    info!(
                        our_hash=?self.genesis_config_hash,
                        their_hash=?genesis_config_hash,
                        "{}: dropping connection to {} due to genesis config hash mismatch",
                        self.our_id,
                        peer_id
                    );
                    return self.remove(effect_builder, &peer_id, false);
                }
                Effects::new()
            }
            Message::Payload(payload) => effect_builder
                .announce_message_received(peer_id, payload)
                .ignore(),
        }
    }

    fn connect_to_peer_if_required(&mut self, peer_address: SocketAddr) -> Effects<Event<P>> {
        if self.pending.contains(&peer_address)
            || self.blocklist.contains(&peer_address)
            || self
                .outgoing
                .iter()
                .any(|(_peer_id, connection)| connection.peer_address == peer_address)
        {
            // We're already trying to connect, are connected, or the connection is on the blocklist
            // - do nothing.
            Effects::new()
        } else {
            // We need to connect.
            assert!(self.pending.insert(peer_address));
            connect_outgoing(
                peer_address,
                Arc::clone(&self.certificate),
                Arc::clone(&self.secret_key),
                Arc::clone(&self.is_stopped),
            )
            .result(
                move |(peer_id, transport)| Event::OutgoingEstablished { peer_id, transport },
                move |error| Event::OutgoingFailed {
                    peer_id: None,
                    peer_address,
                    error: Some(error),
                },
            )
        }
    }

    /// Checks whether a connection has been established fully, i.e. with an incoming and outgoing
    /// connection.
    ///
    /// Returns either no effect or an announcement that a new peer has connected.
    fn check_connection_complete(
        &self,
        effect_builder: EffectBuilder<REv>,
        peer_id: NodeId,
    ) -> Effects<Event<P>> {
        if self.outgoing.contains_key(&peer_id) && self.incoming.contains_key(&peer_id) {
            debug!(%peer_id, "connection to peer is now complete");
            effect_builder.announce_new_peer(peer_id).ignore()
        } else {
            Effects::new()
        }
    }

    fn terminate_if_isolated(&self, effect_builder: EffectBuilder<REv>) -> Effects<Event<P>> {
        if self.is_isolated() {
            if self.is_bootstrap_node {
                info!(
                    "{}: failed to bootstrap to any other nodes, but continuing to run as we are a \
                    bootstrap node",
                    self.our_id
                );
            } else {
                // Note that we could retry the connection to other nodes, but for now we
                // just leave it up to the node operator to restart.
                return fatal!(
                    effect_builder,
                    "{}: failed to connect to any known node, now isolated",
                    self.our_id
                );
            }
        }
        Effects::new()
    }

    /// Returns the set of connected nodes.
    pub(crate) fn peers(&self) -> BTreeMap<NodeId, String> {
        let mut ret = BTreeMap::new();
        for (node_id, connection) in &self.outgoing {
            ret.insert(node_id.clone(), connection.peer_address.to_string());
        }
        for (node_id, connection) in &self.incoming {
            ret.entry(node_id.clone())
                .or_insert_with(|| connection.peer_address.to_string());
        }
        ret
    }

    /// Returns whether or not this node has been isolated.
    ///
    /// An isolated node has no chance of recovering a connection to the network and is not
    /// connected to any peer.
    fn is_isolated(&self) -> bool {
        self.pending.is_empty() && self.outgoing.is_empty() && self.incoming.is_empty()
    }

    /// Returns the node id of this network node.
    /// - Used in validator test.
    #[cfg(test)]
    pub(crate) fn node_id(&self) -> NodeId {
        self.our_id.clone()
    }
}

impl<REv, P> Finalize for SmallNetwork<REv, P>
where
    REv: Send + 'static,
    P: Send + 'static,
{
    fn finalize(mut self) -> BoxFuture<'static, ()> {
        async move {
            // Close the shutdown socket, causing the server to exit.
            drop(self.shutdown_sender.take());

            // Set the flag to true, ensuring any ongoing attempts to establish outgoing TLS
            // connections return errors.
            self.is_stopped.store(true, Ordering::SeqCst);

            // Wait for the server to exit cleanly.
            if let Some(join_handle) = self.server_join_handle.take() {
                match join_handle.await {
                    Ok(_) => debug!("{}: server exited cleanly", self.our_id),
                    Err(err) => error!(%self.our_id,%err, "could not join server task cleanly"),
                }
            } else if env::var(ENABLE_LIBP2P_ENV_VAR).is_err() {
                warn!("{}: server shutdown while already shut down", self.our_id)
            }
        }
        .boxed()
    }
}

impl<REv, P> Component<REv> for SmallNetwork<REv, P>
where
    REv: Send + From<Event<P>> + From<NetworkAnnouncement<NodeId, P>>,
    P: Serialize + DeserializeOwned + Clone + Debug + Display + Send + 'static,
{
    type Event = Event<P>;
    type ConstructionError = Infallible;

    fn handle_event(
        &mut self,
        effect_builder: EffectBuilder<REv>,
        rng: &mut NodeRng,
        event: Self::Event,
    ) -> Effects<Self::Event> {
        match event {
            Event::BootstrappingFailed {
                peer_address,
                error,
            } => {
                warn!(%error, "{}: connection to known node at {} failed", self.our_id, peer_address);

                let was_removed = self.pending.remove(&peer_address);
                assert!(
                    was_removed,
                    "Bootstrap failed for node, but it was not in the set of pending connections"
                );
                self.terminate_if_isolated(effect_builder)
            }
            Event::IncomingNew {
                stream,
                peer_address,
            } => {
                debug!(%peer_address, "{}: incoming connection, starting TLS handshake", self.our_id);

                setup_tls(stream, self.certificate.clone(), self.secret_key.clone())
                    .boxed()
                    .event(move |result| Event::IncomingHandshakeCompleted {
                        result,
                        peer_address,
                    })
            }
            Event::IncomingHandshakeCompleted {
                result,
                peer_address,
            } => self.handle_incoming_tls_handshake_completed(effect_builder, result, peer_address),
            Event::IncomingMessage { peer_id, msg } => {
                self.handle_message(effect_builder, peer_id, msg)
            }
            Event::IncomingClosed {
                result,
                peer_id,
                peer_address,
            } => {
                match result {
                    Ok(()) => info!(%peer_id, %peer_address, "{}: connection closed", self.our_id),
                    Err(err) => {
                        warn!(%peer_id, %peer_address, %err, "{}: connection dropped", self.our_id)
                    }
                }
                self.remove(effect_builder, &peer_id, false)
            }
            Event::OutgoingEstablished { peer_id, transport } => {
                self.setup_outgoing(effect_builder, peer_id, transport)
            }
            Event::OutgoingFailed {
                peer_id,
                peer_address,
                error,
            } => self.handle_outgoing_lost(effect_builder, peer_id, peer_address, error),
            Event::NetworkRequest {
                req:
                    NetworkRequest::SendMessage {
                        dest,
                        payload,
                        responder,
                    },
            } => {
                // We're given a message to send out.
                self.send_message(dest, Message::Payload(payload));
                responder.respond(()).ignore()
            }
            Event::NetworkRequest {
                req: NetworkRequest::Broadcast { payload, responder },
            } => {
                // We're given a message to broadcast.
                self.broadcast_message(Message::Payload(payload));
                responder.respond(()).ignore()
            }
            Event::NetworkRequest {
                req:
                    NetworkRequest::Gossip {
                        payload,
                        count,
                        exclude,
                        responder,
                    },
            } => {
                // We're given a message to gossip.
                let sent_to = self.gossip_message(rng, Message::Payload(payload), count, exclude);
                responder.respond(sent_to).ignore()
            }
            Event::NetworkInfoRequest {
                req: NetworkInfoRequest::GetPeers { responder },
            } => responder.respond(self.peers()).ignore(),
            Event::GossipOurAddress => {
                let mut effects = self.gossip_our_address(effect_builder);
                effects.extend(self.enforce_symmetric_connections(effect_builder));
                effects
            }
            Event::PeerAddressReceived(gossiped_address) => {
                self.connect_to_peer_if_required(gossiped_address.into())
            }
        }
    }
}

/// Core accept loop for the networking server.
///
/// Never terminates.
async fn server_task<P, REv>(
    event_queue: EventQueueHandle<REv>,
    mut listener: tokio::net::TcpListener,
    mut shutdown_receiver: watch::Receiver<()>,
    our_id: NodeId,
) where
    REv: From<Event<P>>,
{
    // The server task is a bit tricky, since it has to wait on incoming connections while at the
    // same time shut down if the networking component is dropped, otherwise the TCP socket will
    // stay open, preventing reuse.

    // We first create a future that never terminates, handling incoming connections:
    let cloned_our_id = our_id.clone();
    let accept_connections = async move {
        loop {
            // We handle accept errors here, since they can be caused by a temporary resource
            // shortage or the remote side closing the connection while it is waiting in
            // the queue.
            match listener.accept().await {
                Ok((stream, peer_address)) => {
                    // Move the incoming connection to the event queue for handling.
                    let event = Event::IncomingNew {
                        stream,
                        peer_address,
                    };
                    event_queue
                        .schedule(event, QueueKind::NetworkIncoming)
                        .await;
                }
                // TODO: Handle resource errors gracefully.
                //       In general, two kinds of errors occur here: Local resource exhaustion,
                //       which should be handled by waiting a few milliseconds, or remote connection
                //       errors, which can be dropped immediately.
                //
                //       The code in its current state will consume 100% CPU if local resource
                //       exhaustion happens, as no distinction is made and no delay introduced.
                Err(err) => {
                    warn!(%err, "{}: dropping incoming connection during accept", cloned_our_id)
                }
            }
        }
    };

    let shutdown_messages = async move { while shutdown_receiver.recv().await.is_some() {} };

    // Now we can wait for either the `shutdown` channel's remote end to do be dropped or the
    // infinite loop to terminate, which never happens.
    match select(Box::pin(shutdown_messages), Box::pin(accept_connections)).await {
        Either::Left(_) => info!(
            "{}: shutting down socket, no longer accepting incoming connections",
            our_id
        ),
        Either::Right(_) => unreachable!(),
    }
}

/// Server-side TLS handshake.
///
/// This function groups the TLS handshake into a convenient function, enabling the `?` operator.
async fn setup_tls(
    stream: TcpStream,
    cert: Arc<TlsCert>,
    secret_key: Arc<PKey<Private>>,
) -> Result<(NodeId, Transport)> {
    let tls_stream = tokio_openssl::accept(
        &tls::create_tls_acceptor(&cert.as_x509().as_ref(), &secret_key.as_ref())
            .map_err(Error::AcceptorCreation)?,
        stream,
    )
    .await?;

    // We can now verify the certificate.
    let peer_cert = tls_stream
        .ssl()
        .peer_certificate()
        .ok_or(Error::NoClientCertificate)?;

    Ok((
        NodeId::from(tls::validate_cert(peer_cert)?.public_key_fingerprint()),
        tls_stream,
    ))
}

/// Network handshake reader for single handshake message received by outgoing connection.
async fn handshake_reader<REv, P>(
    event_queue: EventQueueHandle<REv>,
    mut stream: SplitStream<FramedTransport<P>>,
    our_id: NodeId,
    peer_id: NodeId,
    peer_address: SocketAddr,
) where
    P: DeserializeOwned + Send + Display,
    REv: From<Event<P>>,
{
    if let Some(incoming_handshake_msg) = stream.next().await {
        if let Ok(msg @ Message::Handshake { .. }) = incoming_handshake_msg {
            debug!(%msg, %peer_id, "{}: handshake received", our_id);
            return event_queue
                .schedule(
                    Event::IncomingMessage { peer_id, msg },
                    QueueKind::NetworkIncoming,
                )
                .await;
        }
    }
    warn!(%peer_id, "{}: receiving handshake failed, closing connection", our_id);
    event_queue
        .schedule(
            Event::OutgoingFailed {
                peer_id: Some(peer_id),
                peer_address,
                error: None,
            },
            QueueKind::Network,
        )
        .await
}

/// Network message reader.
///
/// Schedules all received messages until the stream is closed or an error occurs.
async fn message_reader<REv, P>(
    event_queue: EventQueueHandle<REv>,
    mut stream: SplitStream<FramedTransport<P>>,
    mut shutdown_receiver: watch::Receiver<()>,
    our_id: NodeId,
    peer_id: NodeId,
) -> io::Result<()>
where
    P: DeserializeOwned + Send + Display,
    REv: From<Event<P>>,
{
    let our_id_ref = &our_id;
    let peer_id_cloned = peer_id.clone();
    let read_messages = async move {
        while let Some(msg_result) = stream.next().await {
            match msg_result {
                Ok(msg) => {
                    debug!(%msg, peer_id=%peer_id_cloned, "{}: message received", our_id_ref);
                    // We've received a message, push it to the reactor.
                    event_queue
                        .schedule(
                            Event::IncomingMessage {
                                peer_id: peer_id_cloned.clone(),
                                msg,
                            },
                            QueueKind::NetworkIncoming,
                        )
                        .await;
                }
                Err(err) => {
                    warn!(%err, peer_id=%peer_id_cloned, "{}: receiving message failed, closing connection", our_id_ref);
                    return Err(err);
                }
            }
        }
        Ok(())
    };

    let shutdown_messages = async move { while shutdown_receiver.recv().await.is_some() {} };

    // Now we can wait for either the `shutdown` channel's remote end to do be dropped or the
    // while loop to terminate.
    match select(Box::pin(shutdown_messages), Box::pin(read_messages)).await {
        Either::Left(_) => info!(
            %peer_id,
            "{}: shutting down incoming connection message reader",
            &our_id
        ),
        Either::Right(_) => (),
    }

    Ok(())
}

/// Network message sender.
///
/// Reads from a channel and sends all messages, until the stream is closed or an error occurs.
///
/// Initially sends a handshake including the `genesis_config_hash` as a final handshake step.  If
/// the recipient's `genesis_config_hash` doesn't match, the connection will be closed.
async fn message_sender<P>(
    mut queue: UnboundedReceiver<Message<P>>,
    mut sink: SplitSink<FramedTransport<P>, Message<P>>,
    handshake: Message<P>,
) -> Result<()>
where
    P: Serialize + Send,
{
    sink.send(handshake).await.map_err(Error::MessageNotSent)?;
    while let Some(payload) = queue.recv().await {
        // We simply error-out if the sink fails, it means that our connection broke.
        sink.send(payload).await.map_err(Error::MessageNotSent)?;
    }

    Ok(())
}

/// Transport type alias for base encrypted connections.
type Transport = SslStream<TcpStream>;

/// A framed transport for `Message`s.
type FramedTransport<P> = SymmetricallyFramed<
    Framed<Transport, LengthDelimitedCodec>,
    Message<P>,
    SymmetricalMessagePack<Message<P>>,
>;

/// Constructs a new framed transport on a stream.
fn framed<P>(stream: Transport) -> FramedTransport<P> {
    let length_delimited = Framed::new(stream, LengthDelimitedCodec::new());
    SymmetricallyFramed::new(
        length_delimited,
        SymmetricalMessagePack::<Message<P>>::default(),
    )
}

/// Initiates a TLS connection to a remote address.
async fn connect_outgoing(
    peer_address: SocketAddr,
    our_certificate: Arc<TlsCert>,
    secret_key: Arc<PKey<Private>>,
    server_is_stopped: Arc<AtomicBool>,
) -> Result<(NodeId, Transport)> {
    let mut config = tls::create_tls_connector(&our_certificate.as_x509(), &secret_key)
        .context("could not create TLS connector")?
        .configure()
        .map_err(Error::ConnectorConfiguration)?;
    config.set_verify_hostname(false);

    let stream = TcpStream::connect(peer_address)
        .await
        .context("TCP connection failed")?;

    let tls_stream = tokio_openssl::connect(config, "this-will-not-be-checked.example.com", stream)
        .await
        .context("tls handshake failed")?;

    let peer_cert = tls_stream
        .ssl()
        .peer_certificate()
        .ok_or(Error::NoServerCertificate)?;

    let peer_id = tls::validate_cert(peer_cert)?.public_key_fingerprint();

    if server_is_stopped.load(Ordering::SeqCst) {
        debug!(
            our_id=%our_certificate.public_key_fingerprint(),
            %peer_address,
            "server stopped - aborting outgoing TLS connection"
        );
        Err(Error::ServerStopped)
    } else {
        Ok((NodeId::from(peer_id), tls_stream))
    }
}

impl<R, P> Debug for SmallNetwork<R, P>
where
    P: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SmallNetwork")
            .field("our_id", &self.our_id)
            .field("certificate", &"<SSL cert>")
            .field("secret_key", &"<hidden>")
            .field("public_address", &self.public_address)
            .field("event_queue", &"<event_queue>")
            .field("incoming", &self.incoming)
            .field("outgoing", &self.outgoing)
            .field("pending", &self.pending)
            .finish()
    }
}
