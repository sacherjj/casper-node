use std::convert::TryFrom;

use casper_engine_test_support::internal::{
    utils, InMemoryWasmTestBuilder, RewardItem, SlashItem, StepRequestBuilder, WasmTestBuilder,
    DEFAULT_ACCOUNTS,
};
use casper_execution_engine::{
    core::engine_state::genesis::GenesisAccount, shared::motes::Motes,
    storage::global_state::in_memory::InMemoryGlobalState,
};
use casper_types::{
    account::AccountHash,
    auction::{
        Bids, SeigniorageRecipientsSnapshot, BIDS_KEY, BLOCK_REWARD,
        SEIGNIORAGE_RECIPIENTS_SNAPSHOT_KEY, VALIDATOR_REWARD_PURSE_KEY,
    },
    mint::TOTAL_SUPPLY_KEY,
    CLValue, ContractHash, Key, ProtocolVersion, PublicKey, U512,
};

const ACCOUNT_1_PK: PublicKey = PublicKey::Ed25519([200; 32]);
const ACCOUNT_1_ADDR: AccountHash = AccountHash::new([200; 32]);
const ACCOUNT_1_BALANCE: u64 = 100_000_000;
const ACCOUNT_1_BOND: u64 = 100_000_000;

const ACCOUNT_2_PK: PublicKey = PublicKey::Ed25519([202; 32]);
const ACCOUNT_2_ADDR: AccountHash = AccountHash::new([202; 32]);
const ACCOUNT_2_BALANCE: u64 = 200_000_000;
const ACCOUNT_2_BOND: u64 = 200_000_000;

fn get_named_key(
    builder: &mut InMemoryWasmTestBuilder,
    contract_hash: ContractHash,
    name: &str,
) -> Key {
    *builder
        .get_contract(contract_hash)
        .expect("should have contract")
        .named_keys()
        .get(name)
        .expect("should have bid purses")
}

fn initialize_builder() -> WasmTestBuilder<InMemoryGlobalState> {
    let mut builder = InMemoryWasmTestBuilder::default();

    let accounts = {
        let mut tmp: Vec<GenesisAccount> = DEFAULT_ACCOUNTS.clone();
        let account_1 = GenesisAccount::new(
            ACCOUNT_1_PK,
            ACCOUNT_1_ADDR,
            Motes::new(ACCOUNT_1_BALANCE.into()),
            Motes::new(ACCOUNT_1_BOND.into()),
        );
        let account_2 = GenesisAccount::new(
            ACCOUNT_2_PK,
            ACCOUNT_2_ADDR,
            Motes::new(ACCOUNT_2_BALANCE.into()),
            Motes::new(ACCOUNT_2_BOND.into()),
        );
        tmp.push(account_1);
        tmp.push(account_2);
        tmp
    };
    let run_genesis_request = utils::create_run_genesis_request(accounts);
    builder.run_genesis(&run_genesis_request);
    builder
}

/// Should be able to step slashing, rewards, and run auction.
#[ignore]
#[test]
fn should_step() {
    let mut builder = initialize_builder();

    let step_request = StepRequestBuilder::new()
        .with_parent_state_hash(builder.get_post_state_hash())
        .with_protocol_version(ProtocolVersion::V1_0_0)
        .with_slash_item(SlashItem::new(ACCOUNT_1_PK))
        .with_reward_item(RewardItem::new(ACCOUNT_1_PK, BLOCK_REWARD / 2))
        .with_reward_item(RewardItem::new(ACCOUNT_2_PK, BLOCK_REWARD / 2))
        .build();

    let auction_hash = builder.get_auction_contract_hash();

    let reward_purse_key = get_named_key(&mut builder, auction_hash, VALIDATOR_REWARD_PURSE_KEY)
        .into_uref()
        .expect("should be uref");

    let before_balance = builder.get_purse_balance(reward_purse_key);
    let before_auction_seigniorage: SeigniorageRecipientsSnapshot =
        builder.get_value(auction_hash, SEIGNIORAGE_RECIPIENTS_SNAPSHOT_KEY);

    let bids_before_slashing: Bids = builder.get_value(auction_hash, BIDS_KEY);
    assert!(
        bids_before_slashing.contains_key(&ACCOUNT_1_PK),
        "should have entry in the genesis bids table {:?}",
        bids_before_slashing
    );

    let bids_before_slashing: Bids = builder.get_value(auction_hash, BIDS_KEY);
    assert!(
        bids_before_slashing.contains_key(&ACCOUNT_1_PK),
        "should have entry in bids table before slashing {:?}",
        bids_before_slashing
    );

    builder.step(step_request);

    let bids_after_slashing: Bids = builder.get_value(auction_hash, BIDS_KEY);
    assert!(
        !bids_after_slashing.contains_key(&ACCOUNT_1_PK),
        "should not have entry in bids table after slashing {:?}",
        bids_after_slashing
    );

    // reward purse balance should not be the same after reward distribution
    let after_balance = builder.get_purse_balance(reward_purse_key);
    assert_ne!(
        before_balance, after_balance,
        "reward balance should change"
    );

    let bids_after_slashing: Bids = builder.get_value(auction_hash, BIDS_KEY);
    assert_ne!(
        bids_before_slashing, bids_after_slashing,
        "bids table should be different before and after slashing"
    );

    // seigniorage snapshot should have changed after auction
    let after_auction_seigniorage: SeigniorageRecipientsSnapshot =
        builder.get_value(auction_hash, SEIGNIORAGE_RECIPIENTS_SNAPSHOT_KEY);
    assert!(
        !before_auction_seigniorage
            .keys()
            .all(|key| after_auction_seigniorage.contains_key(key)),
        "run auction should have changed seigniorage keys"
    );
}

/// Should be able to step slashing, rewards, and run auction.
#[ignore]
#[test]
fn should_adjust_total_supply() {
    let mut builder = initialize_builder();
    let maybe_post_state_hash = Some(builder.get_post_state_hash());

    let mint_hash = builder.get_mint_contract_hash();

    // should check total supply before step
    let total_supply_key = get_named_key(&mut builder, mint_hash, TOTAL_SUPPLY_KEY)
        .into_uref()
        .expect("should be uref");

    let starting_total_supply = CLValue::try_from(
        builder
            .query(maybe_post_state_hash, total_supply_key.into(), &[])
            .expect("should have total supply"),
    )
    .expect("should be a CLValue")
    .into_t::<U512>()
    .expect("should be U512");

    // slash
    let step_request = StepRequestBuilder::new()
        .with_parent_state_hash(builder.get_post_state_hash())
        .with_protocol_version(ProtocolVersion::V1_0_0)
        .with_slash_item(SlashItem::new(ACCOUNT_1_PK))
        .with_slash_item(SlashItem::new(ACCOUNT_2_PK))
        .with_reward_item(RewardItem::new(ACCOUNT_1_PK, 0))
        .with_reward_item(RewardItem::new(ACCOUNT_2_PK, BLOCK_REWARD / 2))
        .build();

    builder.step(step_request);
    let maybe_post_state_hash = Some(builder.get_post_state_hash());

    // should check total supply after step
    let modified_total_supply = CLValue::try_from(
        builder
            .query(maybe_post_state_hash, total_supply_key.into(), &[])
            .expect("should have total supply"),
    )
    .expect("should be a CLValue")
    .into_t::<U512>()
    .expect("should be U512");

    assert!(
        modified_total_supply < starting_total_supply,
        "total supply should be reduced due to slashing"
    );
}
