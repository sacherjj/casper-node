use std::collections::BTreeMap;

use casper_types::{
    bytesrepr::{self, FromBytes, ToBytes},
    ContractHash, HashAddr,
};

use crate::shared::wasm_config::WasmConfig;

const DEFAULT_ADDRESS: [u8; 32] = [0; 32];
pub const DEFAULT_WASMLESS_TRANSFER_COST: u64 = 10_000;

/// Represents a protocol's data. Intended to be associated with a given protocol version.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ProtocolData {
    wasm_config: WasmConfig,
    mint: ContractHash,
    proof_of_stake: ContractHash,
    standard_payment: ContractHash,
    auction: ContractHash,
    wasmless_transfer_cost: u64,
}

/// Provides a default instance with non existing urefs and empty costs table.
///
/// Used in contexts where PoS or Mint contract is not ready yet, and pos, and
/// mint installers are ran. For use with caution.
impl Default for ProtocolData {
    fn default() -> ProtocolData {
        ProtocolData {
            wasm_config: WasmConfig::default(),
            mint: DEFAULT_ADDRESS,
            proof_of_stake: DEFAULT_ADDRESS,
            standard_payment: DEFAULT_ADDRESS,
            auction: DEFAULT_ADDRESS,
            wasmless_transfer_cost: DEFAULT_WASMLESS_TRANSFER_COST,
        }
    }
}

impl ProtocolData {
    /// Creates a new `ProtocolData` value from a given `WasmCosts` value.
    pub fn new(
        wasm_config: WasmConfig,
        mint: ContractHash,
        proof_of_stake: ContractHash,
        standard_payment: ContractHash,
        auction: ContractHash,
        wasmless_transfer_cost: u64,
    ) -> Self {
        ProtocolData {
            wasm_config,
            mint,
            proof_of_stake,
            standard_payment,
            auction,
            wasmless_transfer_cost,
        }
    }

    /// Creates a new, partially-valid [`ProtocolData`] value where only the mint URef is known.
    ///
    /// Used during `commit_genesis` before all system contracts' URefs are known.
    pub fn partial_with_mint(mint: ContractHash) -> Self {
        ProtocolData {
            mint,
            ..Default::default()
        }
    }

    /// Creates a new, partially-valid [`ProtocolData`] value where all but the standard payment
    /// uref is known.
    ///
    /// Used during `commit_genesis` before all system contracts' URefs are known.
    pub fn partial_without_standard_payment(
        wasm_config: WasmConfig,
        mint: ContractHash,
        proof_of_stake: ContractHash,
    ) -> Self {
        ProtocolData {
            wasm_config,
            mint,
            proof_of_stake,
            ..Default::default()
        }
    }

    /// Gets the `WasmCosts` value from a given [`ProtocolData`] value.
    pub fn wasm_config(&self) -> &WasmConfig {
        &self.wasm_config
    }

    pub fn mint(&self) -> ContractHash {
        self.mint
    }

    pub fn proof_of_stake(&self) -> ContractHash {
        self.proof_of_stake
    }

    pub fn standard_payment(&self) -> ContractHash {
        self.standard_payment
    }

    pub fn auction(&self) -> ContractHash {
        self.auction
    }

    /// Retrieves all valid system contracts stored in protocol version
    pub fn system_contracts(&self) -> Vec<ContractHash> {
        let mut vec = Vec::with_capacity(3);
        if self.mint != DEFAULT_ADDRESS {
            vec.push(self.mint)
        }
        if self.proof_of_stake != DEFAULT_ADDRESS {
            vec.push(self.proof_of_stake)
        }
        if self.standard_payment != DEFAULT_ADDRESS {
            vec.push(self.standard_payment)
        }
        if self.auction != DEFAULT_ADDRESS {
            vec.push(self.auction)
        }
        vec
    }

    pub fn update_from(&mut self, updates: BTreeMap<ContractHash, ContractHash>) -> bool {
        for (old_hash, new_hash) in updates {
            if old_hash == self.mint {
                self.mint = new_hash;
            } else if old_hash == self.proof_of_stake {
                self.proof_of_stake = new_hash;
            } else if old_hash == self.standard_payment {
                self.standard_payment = new_hash;
            } else if old_hash == self.auction {
                self.auction = new_hash;
            } else {
                return false;
            }
        }
        true
    }

    pub fn wasmless_transfer_cost(&self) -> u64 {
        self.wasmless_transfer_cost
    }
}

impl ToBytes for ProtocolData {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut ret = bytesrepr::unchecked_allocate_buffer(self);
        ret.append(&mut self.wasm_config.to_bytes()?);
        ret.append(&mut self.mint.to_bytes()?);
        ret.append(&mut self.proof_of_stake.to_bytes()?);
        ret.append(&mut self.standard_payment.to_bytes()?);
        ret.append(&mut self.auction.to_bytes()?);
        ret.append(&mut self.wasmless_transfer_cost.to_bytes()?);

        Ok(ret)
    }

    fn serialized_length(&self) -> usize {
        self.wasm_config.serialized_length()
            + self.mint.serialized_length()
            + self.proof_of_stake.serialized_length()
            + self.standard_payment.serialized_length()
            + self.auction.serialized_length()
            + self.wasmless_transfer_cost.serialized_length()
    }
}

impl FromBytes for ProtocolData {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (wasm_config, rem) = WasmConfig::from_bytes(bytes)?;
        let (mint, rem) = HashAddr::from_bytes(rem)?;
        let (proof_of_stake, rem) = HashAddr::from_bytes(rem)?;
        let (standard_payment, rem) = HashAddr::from_bytes(rem)?;
        let (auction, rem) = HashAddr::from_bytes(rem)?;
        let (wasmless_transfer_cost, rem) = FromBytes::from_bytes(rem)?;

        Ok((
            ProtocolData {
                wasm_config,
                mint,
                proof_of_stake,
                standard_payment,
                auction,
                wasmless_transfer_cost,
            },
            rem,
        ))
    }
}

#[cfg(test)]
pub(crate) mod gens {
    use proptest::{num, prop_compose};

    use crate::shared::wasm_config::gens::wasm_config_arb;
    use casper_types::gens;

    use super::ProtocolData;

    prop_compose! {
        pub fn protocol_data_arb()(
            wasm_config in wasm_config_arb(),
            mint in gens::u8_slice_32(),
            proof_of_stake in gens::u8_slice_32(),
            standard_payment in gens::u8_slice_32(),
            auction in gens::u8_slice_32(),
            wasmless_transfer_cost in num::u64::ANY,
        ) -> ProtocolData {
            ProtocolData {
                wasm_config,
                mint,
                proof_of_stake,
                standard_payment,
                auction,
                wasmless_transfer_cost,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::proptest;

    use crate::shared::wasm_config::WasmConfig;
    use casper_types::{bytesrepr, ContractHash};

    use super::{gens, ProtocolData, DEFAULT_WASMLESS_TRANSFER_COST};

    #[test]
    fn should_return_all_system_contracts() {
        let mint_reference = [1u8; 32];
        let proof_of_stake_reference = [2u8; 32];
        let standard_payment_reference = [3u8; 32];
        let auction_reference = [4u8; 32];
        let protocol_data = {
            let wasm_config = WasmConfig::default();
            ProtocolData::new(
                wasm_config,
                mint_reference,
                proof_of_stake_reference,
                standard_payment_reference,
                auction_reference,
                DEFAULT_WASMLESS_TRANSFER_COST,
            )
        };

        let actual = {
            let mut items = protocol_data.system_contracts();
            items.sort_unstable();
            items
        };

        assert_eq!(actual.len(), 4);
        assert_eq!(actual[0], mint_reference);
        assert_eq!(actual[1], proof_of_stake_reference);
        assert_eq!(actual[2], standard_payment_reference);
        assert_eq!(actual[3], auction_reference);
    }

    #[test]
    fn should_return_only_valid_system_contracts() {
        let expected: Vec<ContractHash> = vec![];
        assert_eq!(ProtocolData::default().system_contracts(), expected);

        let mint_reference = [0u8; 32]; // <-- invalid addr
        let proof_of_stake_reference = [2u8; 32];
        let standard_payment_reference = [3u8; 32];
        let auction_reference = [4u8; 32];
        let protocol_data = {
            let wasm_config = WasmConfig::default();
            ProtocolData::new(
                wasm_config,
                mint_reference,
                proof_of_stake_reference,
                standard_payment_reference,
                auction_reference,
                DEFAULT_WASMLESS_TRANSFER_COST,
            )
        };

        let actual = {
            let mut items = protocol_data.system_contracts();
            items.sort_unstable();
            items
        };

        assert_eq!(actual.len(), 3);
        assert_eq!(actual[0], proof_of_stake_reference);
        assert_eq!(actual[1], standard_payment_reference);
        assert_eq!(actual[2], auction_reference);
    }

    proptest! {
        #[test]
        fn should_serialize_and_deserialize_with_arbitrary_values(
            protocol_data in gens::protocol_data_arb()
        ) {
            bytesrepr::test_serialization_roundtrip(&protocol_data);
        }
    }
}
