use num_rational::Ratio;

use casper_engine_grpc_server::engine_server::{
    ipc::{
        ChainSpec_ActivationPoint, ChainSpec_NewAuctionDelay, ChainSpec_NewLockedFundsPeriod,
        ChainSpec_NewUnbondingDelay, ChainSpec_NewValidatorSlots,
        ChainSpec_NewWasmlessTransferCost, ChainSpec_UpgradePoint, ChainSpec_WasmConfig,
        DeployCode, UpgradeRequest,
    },
    state,
};
use casper_execution_engine::shared::wasm_config::WasmConfig;
use casper_types::{auction::EraId, ProtocolVersion};

#[derive(Default)]
pub struct UpgradeRequestBuilder {
    pre_state_hash: Vec<u8>,
    current_protocol_version: state::ProtocolVersion,
    new_protocol_version: state::ProtocolVersion,
    upgrade_installer: DeployCode,
    new_wasm_config: Option<ChainSpec_WasmConfig>,
    activation_point: ChainSpec_ActivationPoint,
    new_validator_slots: Option<u32>,
    new_auction_delay: Option<u64>,
    new_locked_funds_period: Option<EraId>,
    new_round_seigniorage_rate: Option<Ratio<u64>>,
    new_unbonding_delay: Option<EraId>,
    new_wasmless_transfer_cost: Option<EraId>,
}

impl UpgradeRequestBuilder {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn with_pre_state_hash(mut self, pre_state_hash: &[u8]) -> Self {
        self.pre_state_hash = pre_state_hash.to_vec();
        self
    }

    pub fn with_current_protocol_version(mut self, protocol_version: ProtocolVersion) -> Self {
        self.current_protocol_version = protocol_version.into();
        self
    }

    pub fn with_new_protocol_version(mut self, protocol_version: ProtocolVersion) -> Self {
        self.new_protocol_version = protocol_version.into();
        self
    }

    pub fn with_new_validator_slots(mut self, new_validator_slots: u32) -> Self {
        self.new_validator_slots = Some(new_validator_slots);
        self
    }

    pub fn with_installer_code(mut self, upgrade_installer: DeployCode) -> Self {
        self.upgrade_installer = upgrade_installer;
        self
    }

    pub fn with_new_wasm_config(mut self, opcode_costs: WasmConfig) -> Self {
        self.new_wasm_config = Some(opcode_costs.into());
        self
    }
    pub fn with_new_auction_delay(mut self, new_auction_delay: u64) -> Self {
        self.new_auction_delay = Some(new_auction_delay);
        self
    }

    pub fn with_new_locked_funds_period(mut self, new_locked_funds_period: EraId) -> Self {
        self.new_locked_funds_period = Some(new_locked_funds_period);
        self
    }

    pub fn with_new_round_seigniorage_rate(mut self, rate: Ratio<u64>) -> Self {
        self.new_round_seigniorage_rate = Some(rate);
        self
    }

    pub fn with_new_unbonding_delay(mut self, unbonding_delay: EraId) -> Self {
        self.new_unbonding_delay = Some(unbonding_delay);
        self
    }

    pub fn with_new_wasmless_transfer_cost(mut self, wasmless_transfer_cost: u64) -> Self {
        self.new_wasmless_transfer_cost = Some(wasmless_transfer_cost);
        self
    }

    pub fn with_activation_point(mut self, height: u64) -> Self {
        self.activation_point = {
            let mut ret = ChainSpec_ActivationPoint::new();
            ret.set_height(height);
            ret
        };
        self
    }

    pub fn build(self) -> UpgradeRequest {
        let mut upgrade_point = ChainSpec_UpgradePoint::new();
        upgrade_point.set_activation_point(self.activation_point);
        if let Some(new_wasm_config) = self.new_wasm_config {
            upgrade_point.set_new_wasm_config(new_wasm_config)
        }
        match self.new_validator_slots {
            None => {}
            Some(new_validator_slots) => {
                let mut chainspec_new_validator_slots = ChainSpec_NewValidatorSlots::new();
                chainspec_new_validator_slots.set_new_validator_slots(new_validator_slots);
                upgrade_point.set_new_validator_slots(chainspec_new_validator_slots);
            }
        }

        if let Some(new_auction_delay) = self.new_auction_delay {
            let mut chainspec_new_auction_delay = ChainSpec_NewAuctionDelay::new();
            chainspec_new_auction_delay.set_new_auction_delay(new_auction_delay);
            upgrade_point.set_new_auction_delay(chainspec_new_auction_delay);
        }

        if let Some(new_locked_funds_period) = self.new_locked_funds_period {
            let mut chainspec_new_locked_funds_period = ChainSpec_NewLockedFundsPeriod::new();
            chainspec_new_locked_funds_period.set_new_locked_funds_period(new_locked_funds_period);
            upgrade_point.set_new_locked_funds_period(chainspec_new_locked_funds_period);
        }

        if let Some(new_round_seigniorage_rate) = self.new_round_seigniorage_rate {
            upgrade_point.set_new_round_seigniorage_rate(new_round_seigniorage_rate.into());
        }

        if let Some(new_unbonding_delay) = self.new_unbonding_delay {
            let mut chainspec_new_unbonding_delay = ChainSpec_NewUnbondingDelay::new();
            chainspec_new_unbonding_delay.set_new_unbonding_delay(new_unbonding_delay);
            upgrade_point.set_new_unbonding_delay(chainspec_new_unbonding_delay);
        }

        if let Some(new_wasmless_transfer_cost) = self.new_wasmless_transfer_cost {
            let mut chainspec_new_wasmless_transfer_cost = ChainSpec_NewWasmlessTransferCost::new();
            chainspec_new_wasmless_transfer_cost
                .set_new_wasmless_transfer_cost(new_wasmless_transfer_cost);
            upgrade_point.set_new_wasmless_transfer_cost(chainspec_new_wasmless_transfer_cost);
        }

        upgrade_point.set_protocol_version(self.new_protocol_version);
        upgrade_point.set_upgrade_installer(self.upgrade_installer);

        let mut upgrade_request = UpgradeRequest::new();
        upgrade_request.set_protocol_version(self.current_protocol_version);
        upgrade_request.set_upgrade_point(upgrade_point);
        upgrade_request
    }
}
