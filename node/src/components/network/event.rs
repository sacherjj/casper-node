use std::{
    fmt::{self, Debug, Display, Formatter},
    io,
    num::NonZeroU32,
};

use derive_more::From;
use libp2p::{
    core::connection::{ConnectedPoint, PendingConnectionError},
    Multiaddr,
};
use serde::Serialize;

use super::OneWayMessage;
use crate::{
    effect::requests::{NetworkInfoRequest, NetworkRequest},
    types::NodeId,
};

#[derive(Debug, From, Serialize)]
pub enum Event<P> {
    // ========== Events triggered by the libp2p network behavior ==========
    /// A connection to the given peer has been opened.
    ConnectionEstablished {
        /// Identity of the peer that we have connected to.
        peer_id: NodeId,
        /// Endpoint of the connection that has been opened.
        #[serde(skip_serializing)]
        endpoint: ConnectedPoint,
        /// Number of established connections to this peer, including the one that has just been
        /// opened.
        num_established: NonZeroU32,
    },
    /// A connection with the given peer has been closed, possibly as a result of an error.
    ConnectionClosed {
        /// Identity of the peer that we have connected to.
        peer_id: NodeId,
        /// Endpoint of the connection that has been closed.
        #[serde(skip_serializing)]
        endpoint: ConnectedPoint,
        /// Number of other remaining connections to this same peer.
        num_established: u32,
        /// Reason for the disconnection, if it was not a successful active close.
        cause: Option<String>,
    },
    /// Tried to dial an address but it ended up being unreachable.
    UnreachableAddress {
        /// `NodeId` that we were trying to reach.
        peer_id: NodeId,
        /// Address that we failed to reach.
        address: Multiaddr,
        /// Error that has been encountered.
        #[serde(skip_serializing)]
        error: PendingConnectionError<io::Error>,
        /// Number of remaining connection attempts that are being tried for this peer.
        attempts_remaining: u32,
    },
    /// Tried to dial an address but it ended up being unreachable.  Contrary to
    /// `UnreachableAddress`, we don't know the identity of the peer that we were trying to reach.
    UnknownPeerUnreachableAddress {
        /// Address that we failed to reach.
        address: Multiaddr,
        /// Error that has been encountered.
        #[serde(skip_serializing)]
        error: PendingConnectionError<io::Error>,
    },
    /// One of our listeners has reported a new local listening address.
    NewListenAddress(Multiaddr),
    /// One of our listeners has reported the expiration of a listening address.
    ExpiredListenAddress(Multiaddr),
    /// One of the listeners gracefully closed.
    ListenerClosed {
        /// The addresses that the listener was listening on. These addresses are now considered
        /// expired, similar to if a [`ExpiredListenAddress`](Event::ExpiredListenAddress) event
        /// has been generated for each of them.
        addresses: Vec<Multiaddr>,
        /// Reason for the closure. Contains `Ok(())` if the stream produced `None`, or `Err` if
        /// the stream produced an error.
        #[serde(skip_serializing)]
        reason: Result<(), io::Error>,
    },
    /// One of the listeners reported a non-fatal error.
    ListenerError {
        /// The listener error.
        #[serde(skip_serializing)]
        error: io::Error,
    },

    // ========== Other events ==========
    /// Received one-way network message.
    IncomingOneWayMessage {
        source: NodeId,
        message: OneWayMessage<P>,
    },

    /// Incoming network request.
    #[from]
    NetworkRequest {
        #[serde(skip_serializing)]
        request: NetworkRequest<NodeId, P>,
    },

    /// Incoming network info request.
    #[from]
    NetworkInfoRequest {
        #[serde(skip_serializing)]
        info_request: NetworkInfoRequest<NodeId>,
    },
}

impl<P: Display> Display for Event<P> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Event::ConnectionEstablished {
                peer_id,
                endpoint,
                num_established,
            } => write!(
                f,
                "connection {} to {} at {:?} established",
                num_established, peer_id, endpoint
            ),
            Event::ConnectionClosed {
                peer_id,
                endpoint,
                num_established,
                cause: Some(error),
            } => write!(
                f,
                "connection to {} at {:?} closed, {} remaining: {}",
                peer_id, endpoint, num_established, error
            ),
            Event::ConnectionClosed {
                peer_id,
                endpoint,
                num_established,
                cause: None,
            } => write!(
                f,
                "connection to {} at {:?} closed, {} remaining",
                peer_id, endpoint, num_established
            ),
            Event::UnreachableAddress {
                peer_id,
                address,
                error,
                attempts_remaining,
            } => write!(
                f,
                "failed to connect to {} at {}, {} attempts remaining: {}",
                peer_id, address, attempts_remaining, error
            ),
            Event::UnknownPeerUnreachableAddress { address, error } => {
                write!(f, "failed to connect to peer at {}: {}", address, error)
            }
            Event::NewListenAddress(address) => write!(f, "new listening address {}", address),
            Event::ExpiredListenAddress(address) => {
                write!(f, "expired listening address {}", address)
            }
            Event::ListenerClosed {
                addresses,
                reason: Ok(()),
            } => write!(f, "closed listener {:?}", addresses),
            Event::ListenerClosed {
                addresses,
                reason: Err(error),
            } => write!(f, "closed listener {:?}: {}", addresses, error),
            Event::ListenerError { error } => write!(f, "non-fatal listener error: {}", error),
            Event::IncomingOneWayMessage {
                source: node_id,
                message,
            } => write!(f, "message from {}: {}", node_id, message),
            Event::NetworkRequest { request } => write!(f, "request: {}", request),
            Event::NetworkInfoRequest { info_request } => {
                write!(f, "info request: {}", info_request)
            }
        }
    }
}
