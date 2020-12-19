//! REST server
//!
//! The REST server provides clients with a simple RESTful HTTP API. This component is (currently)
//! intended for basic informational / GET endpoints only; more complex operations should be handled
//! via the RPC server.
//!
//! The actual server is run in backgrounded tasks. HTTP requests are translated into reactor
//! requests to various components.
//!
//! This module currently provides both halves of what is required for an API server:
//! a component implementation that interfaces with other components via being plugged into a
//! reactor, and an external facing http server that exposes various uri routes and converts
//! HTTP requests into the appropriate component events.
//!
//! Currently this component supports two endpoints, each of which takes no arguments:
//! /status : a human readable JSON equivalent of the info-get-status rpc method.
//!     example: curl -X GET 'http://<ip>:8888/status'
//! /metrics : time series data collected from the internals of the node being queried.
//!     example: curl -X GET 'http://<ip>:8888/metrics'

mod config;
mod event;
mod filters;
mod http_server;

use std::{convert::Infallible, fmt::Debug};

use datasize::DataSize;
use futures::{future::BoxFuture, join, FutureExt};
use tokio::{sync::oneshot, task::JoinHandle};
use tracing::{debug, error, warn};

use super::Component;
use crate::{
    effect::{
        requests::{ChainspecLoaderRequest, MetricsRequest, NetworkInfoRequest, StorageRequest},
        EffectBuilder, EffectExt, Effects,
    },
    reactor::Finalize,
    types::{NodeId, StatusFeed},
    NodeRng,
};

use crate::effect::requests::RestRequest;
pub use config::Config;
pub(crate) use event::Event;

/// A helper trait capturing all of this components Request type dependencies.
pub trait ReactorEventT:
    From<Event>
    + From<RestRequest<NodeId>>
    + From<NetworkInfoRequest<NodeId>>
    + From<StorageRequest>
    + From<ChainspecLoaderRequest>
    + From<MetricsRequest>
    + Send
{
}

impl<REv> ReactorEventT for REv where
    REv: From<Event>
        + From<RestRequest<NodeId>>
        + From<NetworkInfoRequest<NodeId>>
        + From<StorageRequest>
        + From<ChainspecLoaderRequest>
        + From<MetricsRequest>
        + Send
        + 'static
{
}

#[derive(DataSize, Debug)]
pub(crate) struct RestServer {
    /// When the message is sent, it signals the server loop to exit cleanly.
    shutdown_sender: oneshot::Sender<()>,
    /// The task handle which will only join once the server loop has exited.
    server_join_handle: Option<JoinHandle<()>>,
}

impl RestServer {
    pub(crate) fn new<REv>(config: Config, effect_builder: EffectBuilder<REv>) -> Self
    where
        REv: ReactorEventT,
    {
        let (shutdown_sender, shutdown_receiver) = oneshot::channel::<()>();

        let server_join_handle =
            tokio::spawn(http_server::run(config, effect_builder, shutdown_receiver));

        RestServer {
            shutdown_sender,
            server_join_handle: Some(server_join_handle),
        }
    }
}

impl<REv> Component<REv> for RestServer
where
    REv: ReactorEventT,
{
    type Event = Event;
    type ConstructionError = Infallible;

    fn handle_event(
        &mut self,
        effect_builder: EffectBuilder<REv>,
        _rng: &mut NodeRng,
        event: Self::Event,
    ) -> Effects<Self::Event> {
        match event {
            Event::RestRequest(RestRequest::GetStatus { responder }) => async move {
                let (last_added_block, peers, chainspec_info) = join!(
                    effect_builder.get_highest_block(),
                    effect_builder.network_peers(),
                    effect_builder.get_chainspec_info()
                );
                let status_feed = StatusFeed::new(last_added_block, peers, chainspec_info);
                responder.respond(status_feed).await;
            }
            .ignore(),
            Event::RestRequest(RestRequest::GetMetrics { responder }) => effect_builder
                .get_metrics()
                .event(move |text| Event::GetMetricsResult {
                    text,
                    main_responder: responder,
                }),
            Event::GetMetricsResult {
                text,
                main_responder,
            } => main_responder.respond(text).ignore(),
        }
    }
}

impl Finalize for RestServer {
    fn finalize(mut self) -> BoxFuture<'static, ()> {
        async {
            let _ = self.shutdown_sender.send(());

            // Wait for the server to exit cleanly.
            if let Some(join_handle) = self.server_join_handle.take() {
                match join_handle.await {
                    Ok(_) => debug!("rest server exited cleanly"),
                    Err(error) => error!(%error, "could not join rest server task cleanly"),
                }
            } else {
                warn!("rest server shutdown while already shut down")
            }
        }
        .boxed()
    }
}
