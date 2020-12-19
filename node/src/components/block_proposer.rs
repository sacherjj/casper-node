//! Block proposer.
//!
//! The block proposer stores deploy hashes in memory, tracking their suitability for inclusion into
//! a new block. Upon request, it returns a list of candidates that can be included.

mod deploy_sets;
mod event;
mod metrics;

#[cfg(test)]
mod tests;

use std::{
    collections::{HashMap, HashSet},
    convert::Infallible,
    time::Duration,
};

use datasize::DataSize;
use prometheus::{self, Registry};
use semver::Version;
use tracing::{debug, error, info, trace, warn};

use crate::{
    components::{chainspec_loader::DeployConfig, Component},
    effect::{
        requests::{BlockProposerRequest, ProtoBlockRequest, StateStoreRequest, StorageRequest},
        EffectBuilder, EffectExt, Effects,
    },
    types::{DeployHash, DeployHeader, ProtoBlock, Timestamp},
    NodeRng,
};
use casper_execution_engine::{core::engine_state::CONV_RATE, shared::gas::Gas};
pub(crate) use deploy_sets::BlockProposerDeploySets;
pub(crate) use event::{DeployType, Event};
use metrics::BlockProposerMetrics;
use num_traits::Zero;

/// Block proposer component.
#[derive(DataSize, Debug)]
pub(crate) struct BlockProposer {
    /// The current state of the proposer component.
    state: BlockProposerState,

    /// Metrics, present in all states.
    metrics: BlockProposerMetrics,
}

/// Interval after which a pruning of the internal sets is triggered.
// TODO: Make configurable.
const PRUNE_INTERVAL: Duration = Duration::from_secs(10);

/// Experimentally, deploys are in the range of 270-280 bytes, we use this to determine if we are
/// within a threshold to break iteration of `pending` early.
const DEPLOY_APPROX_MIN_SIZE: usize = 300;

/// The type of values expressing the block height in the chain.
type BlockHeight = u64;

/// A queue of contents of blocks that we know have been finalized, but we are still missing
/// notifications about finalization of some of their ancestors. It maps block height to the
/// deploys contained in the corresponding block.
type FinalizationQueue = HashMap<BlockHeight, Vec<DeployHash>>;

/// A queue of requests we can't respond to yet, because we aren't up to date on finalized blocks.
/// The key is the height of the next block we will expect to be finalized at the point when we can
/// fulfill the corresponding requests.
type RequestQueue = HashMap<BlockHeight, Vec<ProtoBlockRequest>>;

/// Current operational state of a block proposer.
#[derive(DataSize, Debug)]
#[allow(clippy::large_enum_variant)]
enum BlockProposerState {
    /// Block proposer is initializing, waiting for a state snapshot.
    Initializing { pending: Vec<Event> },
    /// Normal operation.
    Ready(BlockProposerReady),
}

impl BlockProposer {
    /// Creates a new block proposer instance.
    pub(crate) fn new<REv>(
        registry: Registry,
        effect_builder: EffectBuilder<REv>,
        next_finalized_block: BlockHeight,
    ) -> Result<(Self, Effects<Event>), prometheus::Error>
    where
        REv: From<Event> + From<StorageRequest> + From<StateStoreRequest> + Send + 'static,
    {
        // Note: Version is currently not honored by the storage component, so we just hardcode
        // 1.0.0.
        let effects = async move {
            let chainspec = effect_builder
                .get_chainspec(Version::new(1, 0, 0))
                .await
                // Note: Currently the storage component will always return a chainspec, however the
                // interface has not kept up with this yet.
                .expect("chainspec should be infallible");

            // With the chainspec, we can now load the state from storage or use a fresh instance if
            // loading fails.
            let key = deploy_sets::create_storage_key(&chainspec);
            let sets = effect_builder
                .load_state(key.into())
                .await
                .unwrap_or_default();

            (chainspec, sets)
        }
        .event(move |(chainspec, sets)| Event::Loaded {
            chainspec,
            sets,
            next_finalized_block,
        });

        let block_proposer = BlockProposer {
            state: BlockProposerState::Initializing {
                pending: Vec::new(),
            },
            metrics: BlockProposerMetrics::new(registry)?,
        };

        Ok((block_proposer, effects))
    }
}

impl<REv> Component<REv> for BlockProposer
where
    REv: From<Event> + From<StorageRequest> + From<StateStoreRequest> + Send + 'static,
{
    type Event = Event;
    type ConstructionError = Infallible;

    fn handle_event(
        &mut self,
        effect_builder: EffectBuilder<REv>,
        _rng: &mut NodeRng,
        event: Self::Event,
    ) -> Effects<Self::Event> {
        let mut effects = Effects::new();

        // We handle two different states in the block proposer, but our "ready" state is
        // encapsulated in a separate type to simplify the code. The `Initializing` state is simple
        // enough to handle it here directly.
        match (&mut self.state, event) {
            (
                BlockProposerState::Initializing { ref mut pending },
                Event::Loaded {
                    chainspec,
                    sets,
                    next_finalized_block,
                },
            ) => {
                let mut new_ready_state = BlockProposerReady {
                    sets: sets
                        .unwrap_or_default()
                        .with_next_finalized(next_finalized_block),
                    deploy_config: chainspec.genesis.deploy_config,
                    state_key: deploy_sets::create_storage_key(&chainspec),
                    request_queue: Default::default(),
                    unhandled_finalized: Default::default(),
                };

                // Replay postponed events onto new state.
                for ev in pending.drain(..) {
                    effects.extend(new_ready_state.handle_event(effect_builder, ev));
                }

                self.state = BlockProposerState::Ready(new_ready_state);

                // Start pruning deploys after delay.
                effects.extend(
                    effect_builder
                        .set_timeout(PRUNE_INTERVAL)
                        .event(|_| Event::Prune),
                );
            }
            (BlockProposerState::Initializing { ref mut pending }, event) => {
                // Any incoming events are just buffered until initialization is complete.
                pending.push(event);
            }

            (BlockProposerState::Ready(ref mut ready_state), event) => {
                effects.extend(ready_state.handle_event(effect_builder, event));

                // Update metrics after the effects have been applied.
                self.metrics
                    .pending_deploys
                    .set(ready_state.sets.pending.len() as i64);
            }
        };

        effects
    }
}

/// State of operational block proposer.
#[derive(DataSize, Debug)]
struct BlockProposerReady {
    /// Set of deploys currently stored in the block proposer.
    sets: BlockProposerDeploySets,
    /// `unhandled_finalized` is a set of hashes for deploys that the `BlockProposer` has not yet
    /// seen but were reported as reported to `finalized_deploys()`. They are used to
    /// filter deploys for proposal, similar to `self.sets.finalized_deploys`.
    unhandled_finalized: HashSet<DeployHash>,
    // We don't need the whole Chainspec here, just the deploy config.
    deploy_config: DeployConfig,
    /// Key for storing the block proposer state.
    state_key: Vec<u8>,
    /// The queue of requests awaiting being handled.
    request_queue: RequestQueue,
}

impl BlockProposerReady {
    fn handle_event<REv>(
        &mut self,
        effect_builder: EffectBuilder<REv>,
        event: Event,
    ) -> Effects<Event>
    where
        REv: Send + From<StateStoreRequest>,
    {
        match event {
            Event::Request(BlockProposerRequest::RequestProtoBlock(request)) => {
                if request.next_finalized > self.sets.next_finalized {
                    warn!(
                        request_next_finalized = %request.next_finalized,
                        self_next_finalized = %self.sets.next_finalized,
                        "received request before finalization announcement"
                    );
                    self.request_queue
                        .entry(request.next_finalized)
                        .or_default()
                        .push(request);
                    Effects::new()
                } else {
                    request
                        .responder
                        .respond(self.propose_proto_block(
                            self.deploy_config,
                            request.current_instant,
                            request.past_deploys,
                            request.random_bit,
                        ))
                        .ignore()
                }
            }
            Event::BufferDeploy { hash, deploy_type } => {
                self.add_deploy_or_transfer(Timestamp::now(), hash, *deploy_type);
                Effects::new()
            }
            Event::Prune => {
                let pruned = self.prune(Timestamp::now());
                debug!("Pruned {} deploys from buffer", pruned);

                // After pruning, we store a state snapshot.
                let mut effects = effect_builder
                    .save_state(self.state_key.clone().into(), self.sets.clone())
                    .ignore();

                // Re-trigger timer after `PRUNE_INTERVAL`.
                effects.extend(
                    effect_builder
                        .set_timeout(PRUNE_INTERVAL)
                        .event(|_| Event::Prune),
                );

                effects
            }
            Event::Loaded { sets, .. } => {
                // This should never happen, but we can just ignore the event and carry on.
                error!(
                    ?sets,
                    "got loaded event for block proposer state during ready state"
                );
                Effects::new()
            }
            Event::FinalizedProtoBlock { block, mut height } => {
                let (_, mut deploys, transfers, _) = block.destructure();
                deploys.extend(transfers);

                if height > self.sets.next_finalized {
                    warn!(
                        %height,
                        next_finalized = %self.sets.next_finalized,
                        "received finalized blocks out of order; queueing"
                    );
                    // safe to subtract 1 - height will never be 0 in this branch, because
                    // next_finalized is at least 0, and height has to be greater
                    self.sets.finalization_queue.insert(height - 1, deploys);
                    Effects::new()
                } else {
                    let mut effects = self.handle_finalized_block(effect_builder, height, deploys);
                    while let Some(deploys) = self.sets.finalization_queue.remove(&height) {
                        info!(%height, "removed finalization queue entry");
                        height += 1;
                        effects.extend(self.handle_finalized_block(
                            effect_builder,
                            height,
                            deploys,
                        ));
                    }
                    effects
                }
            }
        }
    }

    /// Adds a deploy to the block proposer.
    ///
    /// Returns `false` if the deploy has been rejected.
    fn add_deploy_or_transfer(
        &mut self,
        current_instant: Timestamp,
        hash: DeployHash,
        deploy_or_transfer: DeployType,
    ) {
        if deploy_or_transfer.header().expired(current_instant) {
            trace!("expired deploy {} rejected from the buffer", hash);
            return;
        }
        if self.unhandled_finalized.remove(&hash) {
            info!(
                "deploy {} was previously marked as finalized, storing header",
                hash
            );
            self.sets
                .finalized_deploys
                .insert(hash, deploy_or_transfer.take_header());
            return;
        }
        // only add the deploy if it isn't contained in a finalized block
        if self.sets.finalized_deploys.contains_key(&hash) {
            info!("deploy {} rejected from the buffer", hash);
        } else {
            self.sets.pending.insert(hash, deploy_or_transfer);
            info!("added deploy {} to the buffer", hash);
        }
    }

    /// Notifies the block proposer that a block has been finalized.
    fn finalized_deploys<I>(&mut self, deploys: I)
    where
        I: IntoIterator<Item = DeployHash>,
    {
        for deploy_hash in deploys.into_iter() {
            match self.sets.pending.remove(&deploy_hash) {
                Some(deploy_type) => {
                    self.sets
                        .finalized_deploys
                        .insert(deploy_hash, deploy_type.take_header());
                }
                // If we haven't seen this deploy before, we still need to take note of it.
                _ => {
                    self.unhandled_finalized.insert(deploy_hash);
                }
            }
        }
    }

    /// Handles finalization of a block.
    fn handle_finalized_block<I, REv>(
        &mut self,
        _effect_builder: EffectBuilder<REv>,
        height: BlockHeight,
        deploys: I,
    ) -> Effects<Event>
    where
        I: IntoIterator<Item = DeployHash>,
    {
        self.finalized_deploys(deploys);
        self.sets.next_finalized = height + 1;

        if let Some(requests) = self.request_queue.remove(&self.sets.next_finalized) {
            info!(height = %(height + 1), "handling queued requests");
            requests
                .into_iter()
                .flat_map(|request| {
                    request
                        .responder
                        .respond(self.propose_proto_block(
                            self.deploy_config,
                            request.current_instant,
                            request.past_deploys,
                            request.random_bit,
                        ))
                        .ignore()
                })
                .collect()
        } else {
            Effects::new()
        }
    }

    /// Checks if a deploy is valid (for inclusion into the next block).
    fn is_deploy_valid(
        &self,
        header: &DeployHeader,
        block_timestamp: Timestamp,
        deploy_config: &DeployConfig,
        past_deploys: &HashSet<DeployHash>,
    ) -> bool {
        let all_deps_resolved = || {
            header
                .dependencies()
                .iter()
                .all(|dep| past_deploys.contains(dep) || self.contains_finalized(dep))
        };
        let ttl_valid = header.ttl() <= deploy_config.max_ttl;
        let timestamp_valid = header.timestamp() <= block_timestamp;
        let deploy_valid = header.timestamp() + header.ttl() >= block_timestamp;
        let num_deps_valid = header.dependencies().len() <= deploy_config.max_dependencies as usize;
        ttl_valid && timestamp_valid && deploy_valid && num_deps_valid && all_deps_resolved()
    }

    /// Returns a list of candidates for inclusion into a block.
    fn propose_proto_block(
        &mut self,
        deploy_config: DeployConfig,
        block_timestamp: Timestamp,
        past_deploys: HashSet<DeployHash>,
        random_bit: bool,
    ) -> ProtoBlock {
        let max_transfers = deploy_config.block_max_transfer_count as usize;
        let max_deploys = deploy_config.block_max_deploy_count as usize;
        let max_block_size_bytes = deploy_config.max_block_size as usize;
        let block_gas_limit = Gas::from(deploy_config.block_gas_limit);

        let mut transfers = Vec::new();
        let mut wasm_deploys = Vec::new();
        let mut block_gas_running_total = Gas::zero();
        let mut block_size_running_total = 0usize;

        for (hash, deploy_type) in self.sets.pending.iter() {
            let at_max_transfers = transfers.len() == max_transfers;
            let at_max_deploys = wasm_deploys.len() == max_deploys;

            // Early exit if block limits are met.
            // TODO: break early if gas total is met, but only once we have a constant for the cost
            // of wasm-less transfers. if block_gas_running_total >= block_gas_limit
            if block_size_running_total + DEPLOY_APPROX_MIN_SIZE >= max_block_size_bytes
                || (at_max_deploys && at_max_transfers)
            {
                break;
            }

            if !self.is_deploy_valid(
                &deploy_type.header(),
                block_timestamp,
                &deploy_config,
                &past_deploys,
            ) || past_deploys.contains(hash)
                || self.sets.finalized_deploys.contains_key(hash)
                || block_size_running_total + deploy_type.size() > max_block_size_bytes
            {
                continue;
            }

            let payment_amount_gas = match Gas::from_motes(deploy_type.payment_amount(), CONV_RATE)
            {
                Some(value) => value,
                None => {
                    tracing::error!("payment_amount couldn't be converted from motes to gas");
                    continue;
                }
            };

            let gas_running_total = if let Some(gas_running_total) =
                block_gas_running_total.checked_add(payment_amount_gas)
            {
                gas_running_total
            } else {
                tracing::warn!("block gas would overflow");
                continue;
            };

            if gas_running_total > block_gas_limit {
                continue;
            }
            if deploy_type.is_transfer() && !at_max_transfers {
                transfers.push(*hash);
            } else if !at_max_deploys {
                wasm_deploys.push(*hash);
            }
            block_gas_running_total = gas_running_total;

            block_size_running_total += deploy_type.size();
        }

        ProtoBlock::new(wasm_deploys, transfers, random_bit)
    }

    /// Prunes expired deploy information from the BlockProposer, returns the total deploys pruned.
    fn prune(&mut self, current_instant: Timestamp) -> usize {
        self.sets.prune(current_instant)
    }

    fn contains_finalized(&self, dep: &DeployHash) -> bool {
        self.sets.finalized_deploys.contains_key(dep) || self.unhandled_finalized.contains(dep)
    }
}
