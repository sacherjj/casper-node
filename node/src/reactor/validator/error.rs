use thiserror::Error;

use crate::components::{contract_runtime, network, small_network, storage};

/// Error type returned by the validator reactor.
#[derive(Debug, Error)]
pub enum Error {
    /// Metrics-related error
    #[error("prometheus (metrics) error: {0}")]
    Metrics(#[from] prometheus::Error),

    /// `Network` component error.
    #[error("network error: {0}")]
    Network(#[from] network::Error),

    /// `SmallNetwork` component error.
    #[error("small network error: {0}")]
    SmallNetwork(#[from] small_network::Error),

    /// `Storage` component error.
    #[error("storage error: {0}")]
    Storage(#[from] storage::Error),

    /// `Consensus` component error.
    #[error("consensus error: {0}")]
    Consensus(#[from] anyhow::Error),

    /// `ContractRuntime` component error.
    #[error("contract runtime config error: {0}")]
    ContractRuntime(#[from] contract_runtime::ConfigError),

    /// Failed to serialize data.
    #[error("serialization: {0}")]
    Serialization(#[source] bincode::ErrorKind),
}
