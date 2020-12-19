mod additive_map_diff;
mod deploy_item_builder;
pub mod exec_with_return;
mod execute_request_builder;
mod step_request_builder;
mod upgrade_request_builder;
pub mod utils;
mod wasm_test_builder;

use num_rational::Ratio;
use num_traits::identities::Zero;
use once_cell::sync::Lazy;

use casper_execution_engine::{
    core::engine_state::{
        genesis::{ExecConfig, GenesisAccount, GenesisConfig},
        run_genesis_request::RunGenesisRequest,
    },
    shared::{motes::Motes, newtypes::Blake2bHash, wasm_config::WasmConfig},
    storage::protocol_data::DEFAULT_WASMLESS_TRANSFER_COST,
};
use casper_types::{account::AccountHash, auction::EraId, ProtocolVersion, PublicKey, U512};

use super::DEFAULT_ACCOUNT_INITIAL_BALANCE;
pub use additive_map_diff::AdditiveMapDiff;
pub use deploy_item_builder::DeployItemBuilder;
pub use execute_request_builder::ExecuteRequestBuilder;
pub use step_request_builder::{RewardItem, SlashItem, StepRequestBuilder};
pub use upgrade_request_builder::UpgradeRequestBuilder;
pub use wasm_test_builder::{
    InMemoryWasmTestBuilder, LmdbWasmTestBuilder, WasmTestBuilder, WasmTestResult,
};

pub const MINT_INSTALL_CONTRACT: &str = "mint_install.wasm";
pub const POS_INSTALL_CONTRACT: &str = "pos_install.wasm";
pub const STANDARD_PAYMENT_INSTALL_CONTRACT: &str = "standard_payment_install.wasm";
pub const AUCTION_INSTALL_CONTRACT: &str = "auction_install.wasm";
pub const DEFAULT_VALIDATOR_SLOTS: u32 = 5;
pub const DEFAULT_AUCTION_DELAY: u64 = 3;
pub const DEFAULT_LOCKED_FUNDS_PERIOD: EraId = 15;
/// Default number of eras that need to pass to be able to withdraw unbonded funds.
pub const DEFAULT_UNBONDING_DELAY: EraId = 14;

/// Default round seigniorage rate represented as a fractional number.
///
/// Annual issuance: 2%
/// Minimum round exponent: 14
/// Ticks per year: 31536000000
///
/// (1+0.02)^((2^14)/31536000000)-1 is expressed as a fraction below.
pub const DEFAULT_ROUND_SEIGNIORAGE_RATE: Ratio<u64> = Ratio::new_raw(6414, 623437335209);

pub const DEFAULT_CHAIN_NAME: &str = "gerald";
pub const DEFAULT_GENESIS_TIMESTAMP: u64 = 0;
pub const DEFAULT_BLOCK_TIME: u64 = 0;
pub const MOCKED_ACCOUNT_ADDRESS: AccountHash = AccountHash::new([48u8; 32]);

pub const ARG_AMOUNT: &str = "amount";

// NOTE: Those values could be constants but are kept as once_cell::sync::Lazy to avoid changes of
// `*FOO` into `FOO` back and forth.
pub static DEFAULT_GENESIS_CONFIG_HASH: Lazy<Blake2bHash> = Lazy::new(|| [42; 32].into());
pub static DEFAULT_ACCOUNT_PUBLIC_KEY: Lazy<PublicKey> =
    Lazy::new(|| PublicKey::Ed25519([199; 32]));
pub static DEFAULT_ACCOUNT_ADDR: Lazy<AccountHash> =
    Lazy::new(|| AccountHash::from(*DEFAULT_ACCOUNT_PUBLIC_KEY));
// Declaring DEFAULT_ACCOUNT_KEY as *DEFAULT_ACCOUNT_ADDR causes tests to stall.
pub static DEFAULT_ACCOUNT_KEY: Lazy<AccountHash> =
    Lazy::new(|| AccountHash::from(*DEFAULT_ACCOUNT_PUBLIC_KEY));
pub static DEFAULT_PROPOSER_PUBLIC_KEY: Lazy<PublicKey> =
    Lazy::new(|| PublicKey::Ed25519([198; 32]));
pub static DEFAULT_PROPOSER_ADDR: Lazy<AccountHash> =
    Lazy::new(|| AccountHash::from(*DEFAULT_PROPOSER_PUBLIC_KEY));
pub static DEFAULT_ACCOUNTS: Lazy<Vec<GenesisAccount>> = Lazy::new(|| {
    let mut ret = Vec::new();
    let genesis_account = GenesisAccount::new(
        *DEFAULT_ACCOUNT_PUBLIC_KEY,
        *DEFAULT_ACCOUNT_ADDR,
        Motes::new(DEFAULT_ACCOUNT_INITIAL_BALANCE.into()),
        Motes::zero(),
    );
    ret.push(genesis_account);
    let proposer_account = GenesisAccount::new(
        *DEFAULT_PROPOSER_PUBLIC_KEY,
        *DEFAULT_PROPOSER_ADDR,
        Motes::new(DEFAULT_ACCOUNT_INITIAL_BALANCE.into()),
        Motes::zero(),
    );
    ret.push(proposer_account);
    ret
});
pub static DEFAULT_PROTOCOL_VERSION: Lazy<ProtocolVersion> = Lazy::new(|| ProtocolVersion::V1_0_0);
pub static DEFAULT_PAYMENT: Lazy<U512> = Lazy::new(|| U512::from(1_500_000_000_000u64));
pub static DEFAULT_WASM_CONFIG: Lazy<WasmConfig> = Lazy::new(WasmConfig::default);
pub static DEFAULT_EXEC_CONFIG: Lazy<ExecConfig> = Lazy::new(|| {
    let mint_installer_bytes;
    let pos_installer_bytes;
    let standard_payment_installer_bytes;
    let auction_installer_bytes;
    mint_installer_bytes = utils::read_wasm_file_bytes(MINT_INSTALL_CONTRACT);
    pos_installer_bytes = utils::read_wasm_file_bytes(POS_INSTALL_CONTRACT);
    standard_payment_installer_bytes =
        utils::read_wasm_file_bytes(STANDARD_PAYMENT_INSTALL_CONTRACT);
    auction_installer_bytes = utils::read_wasm_file_bytes(AUCTION_INSTALL_CONTRACT);

    ExecConfig::new(
        mint_installer_bytes,
        pos_installer_bytes,
        standard_payment_installer_bytes,
        auction_installer_bytes,
        DEFAULT_ACCOUNTS.clone(),
        *DEFAULT_WASM_CONFIG,
        DEFAULT_VALIDATOR_SLOTS,
        DEFAULT_AUCTION_DELAY,
        DEFAULT_LOCKED_FUNDS_PERIOD,
        DEFAULT_ROUND_SEIGNIORAGE_RATE,
        DEFAULT_UNBONDING_DELAY,
        DEFAULT_WASMLESS_TRANSFER_COST,
    )
});
pub static DEFAULT_GENESIS_CONFIG: Lazy<GenesisConfig> = Lazy::new(|| {
    GenesisConfig::new(
        DEFAULT_CHAIN_NAME.to_string(),
        DEFAULT_GENESIS_TIMESTAMP,
        *DEFAULT_PROTOCOL_VERSION,
        DEFAULT_EXEC_CONFIG.clone(),
    )
});
pub static DEFAULT_RUN_GENESIS_REQUEST: Lazy<RunGenesisRequest> = Lazy::new(|| {
    RunGenesisRequest::new(
        *DEFAULT_GENESIS_CONFIG_HASH,
        *DEFAULT_PROTOCOL_VERSION,
        DEFAULT_EXEC_CONFIG.clone(),
    )
});
