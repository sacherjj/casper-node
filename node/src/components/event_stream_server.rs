//! Event stream server
//!
//! The event stream server provides clients with an event-stream returning Server-Sent Events
//! (SSEs) holding JSON-encoded data.
//!
//! The actual server is run in backgrounded tasks.
//!
//! This module currently provides both halves of what is required for an API server:
//! a component implementation that interfaces with other components via being plugged into a
//! reactor, and an external facing http server that manages SSE subscriptions on a single endpoint.
//!
//! This component is passive and receives announcements made by other components while never making
//! a request of other components itself. The handled announcements are serialized to JSON and
//! pushed to subscribers.
//!
//! This component uses a ring buffer for outbound events providing some robustness against
//! unintended subscriber disconnects, if a disconnected subscriber re-subscribes before the buffer
//! has advanced past their last received event.
//!
//! For details about the SSE model and a list of supported SSEs, see:
//! <https://github.com/CasperLabs/ceps/blob/master/text/0009-client-api.md#rpcs>

mod config;
mod event;
mod http_server;
mod sse_server;

use std::{convert::Infallible, fmt::Debug};

use datasize::DataSize;
use tokio::sync::mpsc::{self, UnboundedSender};

use super::Component;
use crate::{
    effect::{EffectBuilder, Effects},
    NodeRng,
};

pub use config::Config;
pub(crate) use event::Event;
pub use sse_server::SseData;

/// A helper trait whose bounds represent the requirements for a reactor event that `run_server` can
/// work with.
pub trait ReactorEventT: From<Event> + Send {}

impl<REv> ReactorEventT for REv where REv: From<Event> + Send + 'static {}

#[derive(DataSize, Debug)]
pub(crate) struct EventStreamServer {
    /// Channel sender to pass event-stream data to the event-stream server.
    // TODO - this should not be skipped.  Awaiting support for `UnboundedSender` in datasize crate.
    #[data_size(skip)]
    sse_data_sender: UnboundedSender<SseData>,
}

impl EventStreamServer {
    pub(crate) fn new<REv>(config: Config, _effect_builder: EffectBuilder<REv>) -> Self
    where
        REv: ReactorEventT,
    {
        let (sse_data_sender, sse_data_receiver) = mpsc::unbounded_channel();
        tokio::spawn(http_server::run(config, sse_data_receiver));

        EventStreamServer { sse_data_sender }
    }

    /// Broadcasts the SSE data to all clients connected to the event stream.
    fn broadcast(&mut self, sse_data: SseData) -> Effects<Event> {
        let _ = self.sse_data_sender.send(sse_data);
        Effects::new()
    }
}

impl<REv> Component<REv> for EventStreamServer
where
    REv: ReactorEventT,
{
    type Event = Event;
    type ConstructionError = Infallible;

    fn handle_event(
        &mut self,
        _effect_builder: EffectBuilder<REv>,
        _rng: &mut NodeRng,
        event: Self::Event,
    ) -> Effects<Self::Event> {
        match event {
            Event::BlockFinalized(finalized_block) => {
                self.broadcast(SseData::BlockFinalized(*finalized_block))
            }
            Event::BlockAdded {
                block_hash,
                block_header,
            } => self.broadcast(SseData::BlockAdded {
                block_hash,
                block_header: *block_header,
            }),
            Event::DeployProcessed {
                deploy_hash,
                deploy_header,
                block_hash,
                execution_result,
            } => self.broadcast(SseData::DeployProcessed {
                deploy_hash,
                account: *deploy_header.account(),
                timestamp: deploy_header.timestamp(),
                ttl: deploy_header.ttl(),
                dependencies: deploy_header.dependencies().clone(),
                block_hash,
                execution_result,
            }),
            Event::Fault {
                era_id,
                public_key,
                timestamp,
            } => self.broadcast(SseData::Fault {
                era_id,
                public_key,
                timestamp,
            }),
            Event::FinalitySignature(fs) => self.broadcast(SseData::FinalitySignature(fs)),
        }
    }
}
