use alloc::{collections::BTreeMap, vec::Vec};

use serde::{Deserialize, Serialize};

use crate::{
    auction::{types::DelegationRate, Delegator, EraId},
    bytesrepr::{self, FromBytes, ToBytes},
    system_contract_errors::auction::Error,
    CLType, CLTyped, PublicKey, URef, U512,
};

/// An entry in a founding validator map.
#[derive(PartialEq, Debug, Serialize, Deserialize, Clone)]
pub struct Bid {
    /// The purse that was used for bonding.
    bonding_purse: URef,
    /// The amount of tokens staked by a validator (not including delegators).
    staked_amount: U512,
    /// Delegation rate
    delegation_rate: DelegationRate,
    /// A flag that represents a winning entry.
    ///
    /// `Some` indicates locked funds for a specific era and an autowin status, and `None` case
    /// means that funds are unlocked and autowin status is removed.
    release_era: Option<EraId>,
    /// This validator's delegators, indexed by their public keys
    delegators: BTreeMap<PublicKey, Delegator>,
    /// This validator's seigniorage reward
    reward: U512,
}

impl Bid {
    /// Creates new instance of a bid with locked funds.
    pub fn locked(bonding_purse: URef, staked_amount: U512, release_era: EraId) -> Self {
        let delegation_rate = 0;
        let release_era = Some(release_era);
        let delegators = BTreeMap::new();
        let reward = U512::zero();
        Self {
            bonding_purse,
            staked_amount,
            delegation_rate,
            release_era,
            delegators,
            reward,
        }
    }

    /// Creates new instance of a bid with unlocked funds.
    pub fn unlocked(
        bonding_purse: URef,
        staked_amount: U512,
        delegation_rate: DelegationRate,
    ) -> Self {
        let release_era = None;
        let delegators = BTreeMap::new();
        let reward = U512::zero();
        Self {
            bonding_purse,
            staked_amount,
            delegation_rate,
            release_era,
            delegators,
            reward,
        }
    }

    /// Gets the bonding purse of the provided bid
    pub fn bonding_purse(&self) -> &URef {
        &self.bonding_purse
    }

    /// Gets the staked amount of the provided bid
    pub fn staked_amount(&self) -> &U512 {
        &self.staked_amount
    }

    /// Gets the delegation rate of the provided bid
    pub fn delegation_rate(&self) -> &DelegationRate {
        &self.delegation_rate
    }

    /// Returns `true` if the provided bid is locked.
    pub fn is_locked(&self) -> bool {
        self.release_era.is_some()
    }

    /// Returns the release era of the provided bid, if it is locked.
    pub fn release_era(&self) -> Option<EraId> {
        self.release_era
    }

    /// Returns a reference to the delegators of the provided bid
    pub fn delegators(&self) -> &BTreeMap<PublicKey, Delegator> {
        &self.delegators
    }

    /// Returns a mutable reference to the delegators of the provided bid
    pub fn delegators_mut(&mut self) -> &mut BTreeMap<PublicKey, Delegator> {
        &mut self.delegators
    }

    /// Returns the seigniorage reward of the provided bid
    pub fn reward(&self) -> &U512 {
        &self.reward
    }

    /// Decreases the stake of the provided bid
    pub fn decrease_stake(&mut self, amount: U512) -> Result<U512, Error> {
        if self.is_locked() {
            return Err(Error::ValidatorFundsLocked);
        }

        let updated_staked_amount = self
            .staked_amount
            .checked_sub(amount)
            .ok_or(Error::InvalidAmount)?;

        self.staked_amount = updated_staked_amount;

        Ok(updated_staked_amount)
    }

    /// Increases the stake of the provided bid
    pub fn increase_stake(&mut self, amount: U512) -> Result<U512, Error> {
        let updated_staked_amount = self
            .staked_amount
            .checked_add(amount)
            .ok_or(Error::InvalidAmount)?;

        self.staked_amount = updated_staked_amount;

        Ok(updated_staked_amount)
    }

    /// Increases the seigniorage reward of the provided bid
    pub fn increase_reward(&mut self, amount: U512) -> Result<U512, Error> {
        let updated_reward = self
            .reward
            .checked_add(amount)
            .ok_or(Error::InvalidAmount)?;

        self.reward = updated_reward;

        Ok(updated_reward)
    }

    /// Zeros the seigniorage reward of the provided bid
    pub fn zero_reward(&mut self) {
        self.reward = U512::zero()
    }

    /// Updates the delegation rate of the provided bid
    pub fn with_delegation_rate(&mut self, delegation_rate: DelegationRate) -> &mut Self {
        self.delegation_rate = delegation_rate;
        self
    }

    /// Unlocks the provided bid if the provided era is greater than the bid's release era.
    ///
    /// Returns `true` if the provided bid was unlocked.
    pub fn unlock(&mut self, era: EraId) -> bool {
        match self.release_era {
            Some(release_era) if era >= release_era => {
                self.release_era = None;
                true
            }
            Some(_) => false,
            None => false,
        }
    }

    /// Returns the total staked amount of validator + all delegators
    pub fn total_staked_amount(&self) -> Result<U512, Error> {
        self.delegators
            .iter()
            .fold(Some(U512::zero()), |maybe_a, (_, b)| {
                maybe_a.and_then(|a| a.checked_add(*b.staked_amount()))
            })
            .and_then(|delegators_sum| delegators_sum.checked_add(*self.staked_amount()))
            .ok_or(Error::InvalidAmount)
    }
}

impl CLTyped for Bid {
    fn cl_type() -> CLType {
        CLType::Any
    }
}

impl ToBytes for Bid {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut result = bytesrepr::allocate_buffer(self)?;
        result.extend(self.bonding_purse.to_bytes()?);
        result.extend(self.staked_amount.to_bytes()?);
        result.extend(self.delegation_rate.to_bytes()?);
        result.extend(self.release_era.to_bytes()?);
        result.extend(self.delegators.to_bytes()?);
        result.extend(self.reward.to_bytes()?);
        Ok(result)
    }

    fn serialized_length(&self) -> usize {
        self.bonding_purse.serialized_length()
            + self.staked_amount.serialized_length()
            + self.delegation_rate.serialized_length()
            + self.release_era.serialized_length()
            + self.delegators.serialized_length()
            + self.reward.serialized_length()
    }
}

impl FromBytes for Bid {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (bonding_purse, bytes) = FromBytes::from_bytes(bytes)?;
        let (staked_amount, bytes) = FromBytes::from_bytes(bytes)?;
        let (delegation_rate, bytes) = FromBytes::from_bytes(bytes)?;
        let (release_era, bytes) = FromBytes::from_bytes(bytes)?;
        let (delegators, bytes) = FromBytes::from_bytes(bytes)?;
        let (reward, bytes) = FromBytes::from_bytes(bytes)?;
        Ok((
            Bid {
                bonding_purse,
                staked_amount,
                delegation_rate,
                release_era,
                delegators,
                reward,
            },
            bytes,
        ))
    }
}

#[cfg(test)]
mod tests {
    use alloc::collections::BTreeMap;

    use crate::{
        auction::{Bid, DelegationRate, EraId},
        bytesrepr, AccessRights, URef, U512,
    };

    #[test]
    fn serialization_roundtrip() {
        let founding_validator = Bid {
            bonding_purse: URef::new([42; 32], AccessRights::READ_ADD_WRITE),
            staked_amount: U512::one(),
            delegation_rate: DelegationRate::max_value(),
            release_era: Some(EraId::max_value() - 1),
            delegators: BTreeMap::default(),
            reward: U512::one(),
        };
        bytesrepr::test_serialization_roundtrip(&founding_validator);
    }
}
