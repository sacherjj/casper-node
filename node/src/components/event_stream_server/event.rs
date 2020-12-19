use std::fmt::{self, Display, Formatter};

use casper_types::ExecutionResult;

use crate::{
    components::consensus::EraId,
    crypto::asymmetric_key::PublicKey,
    types::{
        BlockHash, BlockHeader, DeployHash, DeployHeader, FinalitySignature, FinalizedBlock,
        Timestamp,
    },
};

#[derive(Debug)]
pub enum Event {
    BlockFinalized(Box<FinalizedBlock>),
    BlockAdded {
        block_hash: BlockHash,
        block_header: Box<BlockHeader>,
    },
    DeployProcessed {
        deploy_hash: DeployHash,
        deploy_header: Box<DeployHeader>,
        block_hash: BlockHash,
        execution_result: Box<ExecutionResult>,
    },
    Fault {
        era_id: EraId,
        public_key: PublicKey,
        timestamp: Timestamp,
    },
    FinalitySignature(Box<FinalitySignature>),
}

impl Display for Event {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        match self {
            Event::BlockFinalized(finalized_block) => write!(
                formatter,
                "block finalized {}",
                finalized_block.proto_block().hash()
            ),
            Event::BlockAdded { block_hash, .. } => write!(formatter, "block added {}", block_hash),
            Event::DeployProcessed { deploy_hash, .. } => {
                write!(formatter, "deploy processed {}", deploy_hash)
            }
            Event::Fault {
                era_id,
                public_key,
                timestamp,
            } => write!(
                formatter,
                "An equivocator with public key: {} has been identified at time: {} in era: {}",
                public_key, timestamp, era_id,
            ),
            Event::FinalitySignature(fs) => write!(formatter, "finality signature {}", fs),
        }
    }
}
