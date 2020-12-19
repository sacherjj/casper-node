use std::{
    collections::{BTreeMap, HashSet},
    fmt::{self, Debug, Display, Formatter},
};

use casper_types::{
    bytesrepr::{self, FromBytes, ToBytes},
    U512,
};
use datasize::DataSize;
use itertools::Itertools;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{
    components::consensus::{
        candidate_block::CandidateBlock, cl_context::ClContext,
        consensus_protocol::ConsensusProtocol, protocols::highway::HighwayProtocol,
        ConsensusMessage,
    },
    crypto::asymmetric_key::PublicKey,
    types::{ProtoBlock, Timestamp},
};

#[derive(
    DataSize,
    Debug,
    Clone,
    Copy,
    Hash,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct EraId(pub(crate) u64);

impl EraId {
    pub(crate) fn message(self, payload: Vec<u8>) -> ConsensusMessage {
        ConsensusMessage::Protocol {
            era_id: self,
            payload,
        }
    }

    pub(crate) fn successor(self) -> EraId {
        EraId(self.0 + 1)
    }

    /// Returns an iterator over all eras that are still bonded in this one, including this one.
    pub(crate) fn iter_bonded(&self, bonded_eras: u64) -> impl Iterator<Item = EraId> {
        (self.0.saturating_sub(bonded_eras)..=self.0).map(EraId)
    }

    /// Returns an iterator over all eras that are still bonded in this one, excluding this one.
    pub(crate) fn iter_other(&self, count: u64) -> impl Iterator<Item = EraId> {
        (self.0.saturating_sub(count)..self.0).map(EraId)
    }

    /// Returns the current era minus `x`, or `None` if that would be less than `0`.
    pub(crate) fn checked_sub(&self, x: u64) -> Option<EraId> {
        self.0.checked_sub(x).map(EraId)
    }
}

impl Display for EraId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "era {}", self.0)
    }
}

impl From<EraId> for u64 {
    fn from(era_id: EraId) -> Self {
        era_id.0
    }
}

impl ToBytes for EraId {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        self.0.to_bytes()
    }

    fn serialized_length(&self) -> usize {
        self.0.serialized_length()
    }
}

impl FromBytes for EraId {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (id_value, remainder) = u64::from_bytes(bytes)?;
        let era_id = EraId(id_value);
        Ok((era_id, remainder))
    }
}

/// A candidate block waiting for validation and dependencies.
#[derive(DataSize)]
pub struct PendingCandidate {
    /// The candidate, to be passed into the consensus instance once dependencies are resolved.
    candidate: CandidateBlock,
    /// Whether the proto block has been validated yet.
    validated: bool,
    /// A list of IDs of accused validators for which we are still missing evidence.
    missing_evidence: Vec<PublicKey>,
}

impl PendingCandidate {
    fn new(candidate: CandidateBlock, missing_evidence: Vec<PublicKey>) -> Self {
        PendingCandidate {
            candidate,
            validated: false,
            missing_evidence,
        }
    }

    fn is_complete(&self) -> bool {
        self.validated && self.missing_evidence.is_empty()
    }
}

pub struct Era<I> {
    /// The consensus protocol instance.
    pub(crate) consensus: Box<dyn ConsensusProtocol<I, ClContext>>,
    /// The scheduled starting time of this era.
    pub(crate) start_time: Timestamp,
    /// The height of this era's first block.
    pub(crate) start_height: u64,
    /// Pending candidate blocks, waiting for validation. The boolean is `true` if the proto block
    /// has been validated; the vector contains the list of accused validators missing evidence.
    candidates: Vec<PendingCandidate>,
    /// Validators banned in this and the next BONDED_ERAS eras, because they were slashed in the
    /// previous switch block.
    pub(crate) newly_slashed: Vec<PublicKey>,
    /// Validators that have been slashed in any of the recent BONDED_ERAS switch blocks. This
    /// includes `newly_slashed`.
    pub(crate) slashed: HashSet<PublicKey>,
    /// Accusations collected in this era so far.
    accusations: HashSet<PublicKey>,
    /// The validator weights.
    validators: BTreeMap<PublicKey, U512>,
}

impl<I> Era<I> {
    pub(crate) fn new(
        consensus: Box<dyn ConsensusProtocol<I, ClContext>>,
        start_time: Timestamp,
        start_height: u64,
        newly_slashed: Vec<PublicKey>,
        slashed: HashSet<PublicKey>,
        validators: BTreeMap<PublicKey, U512>,
    ) -> Self {
        Era {
            consensus,
            start_time,
            start_height,
            candidates: Vec::new(),
            newly_slashed,
            slashed,
            accusations: HashSet::new(),
            validators,
        }
    }

    /// Adds a new candidate block, together with the accusations for which we don't have evidence
    /// yet.
    pub(crate) fn add_candidate(
        &mut self,
        candidate: CandidateBlock,
        missing_evidence: Vec<PublicKey>,
    ) {
        self.candidates
            .push(PendingCandidate::new(candidate, missing_evidence));
    }

    /// Marks the dependencies of candidate blocks on evidence against validator `pub_key` as
    /// resolved and returns all candidates that have no missing dependencies left.
    pub(crate) fn resolve_evidence(&mut self, pub_key: &PublicKey) -> Vec<CandidateBlock> {
        for pc in &mut self.candidates {
            pc.missing_evidence.retain(|pk| pk != pub_key);
        }
        self.consensus.mark_faulty(pub_key);
        self.remove_complete_candidates()
    }

    /// Marks the proto block as valid or invalid, and returns all candidates whose validity is now
    /// fully determined.
    pub(crate) fn resolve_validity(
        &mut self,
        proto_block: &ProtoBlock,
        valid: bool,
    ) -> Vec<CandidateBlock> {
        if valid {
            self.accept_proto_block(proto_block)
        } else {
            self.reject_proto_block(proto_block)
        }
    }

    /// Marks the dependencies of candidate blocks on the validity of the specified proto block as
    /// resolved and returns all candidates that have no missing dependencies left.
    pub(crate) fn accept_proto_block(&mut self, proto_block: &ProtoBlock) -> Vec<CandidateBlock> {
        for pc in &mut self.candidates {
            if pc.candidate.proto_block() == proto_block {
                pc.validated = true;
            }
        }
        self.remove_complete_candidates()
    }

    /// Removes and returns any candidate blocks depending on the validity of the specified proto
    /// block. If it is invalid, all those candidates are invalid.
    pub(crate) fn reject_proto_block(&mut self, proto_block: &ProtoBlock) -> Vec<CandidateBlock> {
        let (invalid, candidates): (Vec<_>, Vec<_>) = self
            .candidates
            .drain(..)
            .partition(|pc| pc.candidate.proto_block() == proto_block);
        self.candidates = candidates;
        invalid.into_iter().map(|pc| pc.candidate).collect()
    }

    /// Adds new accusations from a finalized block.
    pub(crate) fn add_accusations(&mut self, accusations: &[PublicKey]) {
        for pub_key in accusations {
            if !self.slashed.contains(pub_key) {
                self.accusations.insert(*pub_key);
            }
        }
    }

    /// Returns all accusations from finalized blocks so far.
    pub(crate) fn accusations(&self) -> Vec<PublicKey> {
        self.accusations.iter().cloned().sorted().collect()
    }

    /// Returns the map of validator weights.
    pub(crate) fn validators(&self) -> &BTreeMap<PublicKey, U512> {
        &self.validators
    }

    /// Removes and returns all candidate blocks with no missing dependencies.
    fn remove_complete_candidates(&mut self) -> Vec<CandidateBlock> {
        let (complete, candidates): (Vec<_>, Vec<_>) = self
            .candidates
            .drain(..)
            .partition(PendingCandidate::is_complete);
        self.candidates = candidates;
        complete.into_iter().map(|pc| pc.candidate).collect()
    }
}

impl<I> DataSize for Era<I>
where
    I: 'static,
{
    const IS_DYNAMIC: bool = true;

    const STATIC_HEAP_SIZE: usize = 0;

    #[inline]
    fn estimate_heap_size(&self) -> usize {
        // Destructure self, so we can't miss any fields.
        let Era {
            consensus,
            start_time,
            start_height,
            candidates,
            newly_slashed,
            slashed,
            accusations,
            validators,
        } = self;

        // `DataSize` cannot be made object safe due its use of associated constants. We implement
        // it manually here, downcasting the consensus protocol as a workaround.

        let consensus_heap_size = {
            let any_ref = consensus.as_any();

            if let Some(highway) = any_ref.downcast_ref::<HighwayProtocol<I, ClContext>>() {
                highway.estimate_heap_size()
            } else {
                warn!(
                    "could not downcast consensus protocol to \
                    HighwayProtocol<I, ClContext> to determine heap allocation size"
                );
                0
            }
        };

        consensus_heap_size
            + start_time.estimate_heap_size()
            + start_height.estimate_heap_size()
            + candidates.estimate_heap_size()
            + newly_slashed.estimate_heap_size()
            + slashed.estimate_heap_size()
            + accusations.estimate_heap_size()
            + validators.estimate_heap_size()
    }
}

#[cfg(test)]
mod tests {
    use rand::Rng;

    use super::*;
    use crate::testing::TestRng;

    #[test]
    fn bytesrepr_roundtrip() {
        let mut rng = TestRng::new();
        let era_id = EraId(rng.gen());
        bytesrepr::test_serialization_roundtrip(&era_id);
    }
}
