use once_cell::sync::Lazy;

use casper_engine_test_support::{
    internal::{
        utils, ExecuteRequestBuilder, InMemoryWasmTestBuilder, DEFAULT_ACCOUNTS,
        DEFAULT_ACCOUNT_PUBLIC_KEY, DEFAULT_LOCKED_FUNDS_PERIOD,
    },
    DEFAULT_ACCOUNT_ADDR, DEFAULT_ACCOUNT_INITIAL_BALANCE, MINIMUM_ACCOUNT_CREATION_BALANCE,
};
use casper_execution_engine::{core::engine_state::genesis::GenesisAccount, shared::motes::Motes};
use casper_types::{
    account::AccountHash,
    auction::{
        Bids, UnbondingPurses, ARG_DELEGATOR, ARG_UNBOND_PURSE, ARG_VALIDATOR,
        ARG_VALIDATOR_PUBLIC_KEYS, BIDS_KEY, METHOD_RUN_AUCTION, METHOD_SLASH,
        UNBONDING_PURSES_KEY,
    },
    mint::TOTAL_SUPPLY_KEY,
    runtime_args, PublicKey, RuntimeArgs, URef, U512,
};

const CONTRACT_TRANSFER_TO_ACCOUNT: &str = "transfer_to_account_u512.wasm";
const CONTRACT_WITHDRAW_BID: &str = "withdraw_bid.wasm";
const CONTRACT_DELEGATE: &str = "delegate.wasm";
const CONTRACT_UNDELEGATE: &str = "undelegate.wasm";

const DELEGATE_AMOUNT_1: u64 = 95_000;
const UNDELEGATE_AMOUNT_1: u64 = 17_000;

const TRANSFER_AMOUNT: u64 = MINIMUM_ACCOUNT_CREATION_BALANCE;

const ARG_AMOUNT: &str = "amount";
const ARG_PUBLIC_KEY: &str = "public_key";

const SYSTEM_ADDR: AccountHash = AccountHash::new([0u8; 32]);
const VALIDATOR_1: PublicKey = PublicKey::Ed25519([3; 32]);
static VALIDATOR_1_ADDR: Lazy<AccountHash> = Lazy::new(|| VALIDATOR_1.into());
const VALIDATOR_1_STAKE: u64 = 250_000;

#[ignore]
#[test]
fn should_run_ee_1119_dont_slash_delegated_validators() {
    let accounts = {
        let validator_1 = GenesisAccount::new(
            VALIDATOR_1,
            *VALIDATOR_1_ADDR,
            Motes::new(DEFAULT_ACCOUNT_INITIAL_BALANCE.into()),
            Motes::new(VALIDATOR_1_STAKE.into()),
        );

        let mut tmp: Vec<GenesisAccount> = DEFAULT_ACCOUNTS.clone();
        tmp.push(validator_1);
        tmp
    };
    let run_genesis_request = utils::create_run_genesis_request(accounts);

    let mut builder = InMemoryWasmTestBuilder::default();
    builder.run_genesis(&run_genesis_request);

    let fund_system_exec_request = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_TRANSFER_TO_ACCOUNT,
        runtime_args! {
            "target" => SYSTEM_ADDR,
            "amount" => U512::from(TRANSFER_AMOUNT)
        },
    )
    .build();

    builder
        .exec(fund_system_exec_request)
        .expect_success()
        .commit();

    let auction = builder.get_auction_contract_hash();

    //
    // Validator delegates funds on other genesis validator
    //

    let delegate_exec_request = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_DELEGATE,
        runtime_args! {
            ARG_AMOUNT => U512::from(DELEGATE_AMOUNT_1),
            ARG_VALIDATOR => VALIDATOR_1,
            ARG_DELEGATOR => *DEFAULT_ACCOUNT_PUBLIC_KEY,
        },
    )
    .build();

    builder
        .exec(delegate_exec_request)
        .expect_success()
        .commit();

    let bids: Bids = builder.get_value(auction, BIDS_KEY);
    let validator_1_bid = bids.get(&VALIDATOR_1).expect("should have bid");
    let bid_purse = validator_1_bid.bonding_purse();
    assert_eq!(
        builder.get_purse_balance(*bid_purse),
        U512::from(VALIDATOR_1_STAKE),
    );

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 0);

    //
    // Unlock funds of genesis validators
    //

    for _ in 0..=DEFAULT_LOCKED_FUNDS_PERIOD {
        let run_auction_request = ExecuteRequestBuilder::contract_call_by_hash(
            SYSTEM_ADDR,
            auction,
            METHOD_RUN_AUCTION,
            runtime_args! {},
        )
        .build();

        builder.exec(run_auction_request).expect_success().commit();
    }

    //
    // Partial unbond through undelegate on other genesis validator
    //

    let unbond_amount = U512::from(VALIDATOR_1_STAKE) - 1;

    let undelegate_exec_request = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_UNDELEGATE,
        runtime_args! {
            ARG_AMOUNT => U512::from(UNDELEGATE_AMOUNT_1),
            ARG_VALIDATOR => VALIDATOR_1,
            ARG_DELEGATOR => *DEFAULT_ACCOUNT_PUBLIC_KEY,
            ARG_UNBOND_PURSE => Option::<URef>::None,
        },
    )
    .build();
    builder
        .exec(undelegate_exec_request)
        .commit()
        .expect_success();

    //
    // Other genesis validator withdraws withdraws his bid
    //

    let withdraw_bid_request = ExecuteRequestBuilder::standard(
        *VALIDATOR_1_ADDR,
        CONTRACT_WITHDRAW_BID,
        runtime_args! {
            ARG_AMOUNT => unbond_amount,
            ARG_PUBLIC_KEY => VALIDATOR_1,
            ARG_UNBOND_PURSE => Option::<URef>::None,
        },
    )
    .build();

    builder.exec(withdraw_bid_request).expect_success().commit();

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 1);

    let unbond_list = unbond_purses
        .get(&VALIDATOR_1)
        .cloned()
        .expect("should have unbond");
    assert_eq!(unbond_list.len(), 2); // two entries in order: undelegate, and withdraw bid

    // undelegate entry

    assert_eq!(unbond_list[0].validator_public_key(), &VALIDATOR_1,);
    assert_eq!(
        unbond_list[0].unbonder_public_key(),
        &*DEFAULT_ACCOUNT_PUBLIC_KEY,
    );
    assert!(!unbond_list[0].is_validator());

    //
    // withdraw_bid entry
    //

    assert_eq!(unbond_list[1].validator_public_key(), &VALIDATOR_1,);
    assert_eq!(unbond_list[1].unbonder_public_key(), &VALIDATOR_1,);
    assert!(unbond_list[1].is_validator());
    assert_eq!(unbond_list[1].amount(), &unbond_amount);

    assert!(
        !unbond_purses.contains_key(&*DEFAULT_ACCOUNT_PUBLIC_KEY),
        "should not be part of unbonds"
    );

    let slash_request_1 = ExecuteRequestBuilder::contract_call_by_hash(
        SYSTEM_ADDR,
        auction,
        METHOD_SLASH,
        runtime_args! {
            ARG_VALIDATOR_PUBLIC_KEYS => vec![
               *DEFAULT_ACCOUNT_PUBLIC_KEY,
            ]
        },
    )
    .build();

    builder.exec(slash_request_1).expect_success().commit();

    let unbond_purses_noop: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(
        unbond_purses, unbond_purses_noop,
        "slashing default validator should be noop because no unbonding was done"
    );

    let bids: Bids = builder.get_value(auction, BIDS_KEY);
    assert!(!bids.is_empty());
    assert!(bids.contains_key(&VALIDATOR_1)); // still bid upon

    //
    // Slash - only `withdraw_bid` amount is slashed
    //
    let total_supply_before_slashing: U512 =
        builder.get_value(builder.get_mint_contract_hash(), TOTAL_SUPPLY_KEY);

    let slash_request_2 = ExecuteRequestBuilder::contract_call_by_hash(
        SYSTEM_ADDR,
        auction,
        METHOD_SLASH,
        runtime_args! {
            ARG_VALIDATOR_PUBLIC_KEYS => vec![
               VALIDATOR_1
            ]
        },
    )
    .build();

    builder.exec(slash_request_2).expect_success().commit();

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 0);

    assert!(
        !unbond_purses.contains_key(&*DEFAULT_ACCOUNT_PUBLIC_KEY),
        "delegator should not be part of unbond list after slashing validator"
    );

    assert!(
        !unbond_purses.contains_key(&VALIDATOR_1),
        "should not be a part of unbond list because delegator was slashed"
    );

    let bids: Bids = builder.get_value(auction, BIDS_KEY);
    assert!(!bids.contains_key(&VALIDATOR_1)); // still bid upon

    let total_supply_after_slashing: U512 =
        builder.get_value(builder.get_mint_contract_hash(), TOTAL_SUPPLY_KEY);
    assert_eq!(
        total_supply_before_slashing - total_supply_after_slashing,
        U512::from(VALIDATOR_1_STAKE + UNDELEGATE_AMOUNT_1),
    );
}
