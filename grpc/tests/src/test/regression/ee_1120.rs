use std::{collections::BTreeSet, iter::FromIterator};

use once_cell::sync::Lazy;

use casper_engine_test_support::{
    internal::{utils, ExecuteRequestBuilder, InMemoryWasmTestBuilder, DEFAULT_ACCOUNTS},
    DEFAULT_ACCOUNT_ADDR, DEFAULT_ACCOUNT_INITIAL_BALANCE, MINIMUM_ACCOUNT_CREATION_BALANCE,
};
use casper_execution_engine::{core::engine_state::genesis::GenesisAccount, shared::motes::Motes};
use casper_types::{
    account::AccountHash,
    auction::{
        Bids, UnbondingPurses, ARG_DELEGATOR, ARG_UNBOND_PURSE, ARG_VALIDATOR,
        ARG_VALIDATOR_PUBLIC_KEYS, BIDS_KEY, METHOD_SLASH, UNBONDING_PURSES_KEY,
    },
    runtime_args, PublicKey, RuntimeArgs, URef, U512,
};

const CONTRACT_TRANSFER_TO_ACCOUNT: &str = "transfer_to_account_u512.wasm";
const CONTRACT_DELEGATE: &str = "delegate.wasm";
const CONTRACT_UNDELEGATE: &str = "undelegate.wasm";

const DELEGATE_AMOUNT_1: u64 = 95_000;
const DELEGATE_AMOUNT_2: u64 = 42_000;
const DELEGATE_AMOUNT_3: u64 = 13_000;
const UNDELEGATE_AMOUNT_1: u64 = 17_000;
const UNDELEGATE_AMOUNT_2: u64 = 24_500;
const UNDELEGATE_AMOUNT_3: u64 = 7_500;

const TRANSFER_AMOUNT: u64 = MINIMUM_ACCOUNT_CREATION_BALANCE;

const ARG_AMOUNT: &str = "amount";

const SYSTEM_ADDR: AccountHash = AccountHash::new([0u8; 32]);
const VALIDATOR_1: PublicKey = PublicKey::Ed25519([3; 32]);
const VALIDATOR_2: PublicKey = PublicKey::Ed25519([4; 32]);
const DELEGATOR_1: PublicKey = PublicKey::Ed25519([5; 32]);
static VALIDATOR_1_ADDR: Lazy<AccountHash> = Lazy::new(|| VALIDATOR_1.into());
static VALIDATOR_2_ADDR: Lazy<AccountHash> = Lazy::new(|| VALIDATOR_2.into());
static DELEGATOR_1_ADDR: Lazy<AccountHash> = Lazy::new(|| DELEGATOR_1.into());
const VALIDATOR_1_STAKE: u64 = 250_000;
const VALIDATOR_2_STAKE: u64 = 350_000;

#[ignore]
#[test]
fn should_run_ee_1120_slash_delegators() {
    let accounts = {
        let validator_1 = GenesisAccount::new(
            VALIDATOR_1,
            *VALIDATOR_1_ADDR,
            Motes::new(DEFAULT_ACCOUNT_INITIAL_BALANCE.into()),
            Motes::new(VALIDATOR_1_STAKE.into()),
        );
        let validator_2 = GenesisAccount::new(
            VALIDATOR_2,
            *VALIDATOR_2_ADDR,
            Motes::new(DEFAULT_ACCOUNT_INITIAL_BALANCE.into()),
            Motes::new(VALIDATOR_2_STAKE.into()),
        );

        let mut tmp: Vec<GenesisAccount> = DEFAULT_ACCOUNTS.clone();
        tmp.push(validator_1);
        tmp.push(validator_2);
        tmp
    };
    let run_genesis_request = utils::create_run_genesis_request(accounts);

    let mut builder = InMemoryWasmTestBuilder::default();
    builder.run_genesis(&run_genesis_request);

    let transfer_request_1 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_TRANSFER_TO_ACCOUNT,
        runtime_args! {
            "target" => SYSTEM_ADDR,
            "amount" => U512::from(TRANSFER_AMOUNT)
        },
    )
    .build();

    builder.exec(transfer_request_1).expect_success().commit();

    let transfer_request_2 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_TRANSFER_TO_ACCOUNT,
        runtime_args! {
            "target" => *DELEGATOR_1_ADDR,
            "amount" => U512::from(TRANSFER_AMOUNT)
        },
    )
    .build();

    builder.exec(transfer_request_2).expect_success().commit();

    let auction = builder.get_auction_contract_hash();

    // Validator delegates funds to other genesis validator

    let delegate_exec_request_1 = ExecuteRequestBuilder::standard(
        *DELEGATOR_1_ADDR,
        CONTRACT_DELEGATE,
        runtime_args! {
            ARG_AMOUNT => U512::from(DELEGATE_AMOUNT_1),
            ARG_VALIDATOR => VALIDATOR_2,
            ARG_DELEGATOR => DELEGATOR_1,
        },
    )
    .build();

    let delegate_exec_request_2 = ExecuteRequestBuilder::standard(
        *DELEGATOR_1_ADDR,
        CONTRACT_DELEGATE,
        runtime_args! {
            ARG_AMOUNT => U512::from(DELEGATE_AMOUNT_2),
            ARG_VALIDATOR => VALIDATOR_1,
            ARG_DELEGATOR => DELEGATOR_1,
        },
    )
    .build();

    let delegate_exec_request_3 = ExecuteRequestBuilder::standard(
        *VALIDATOR_2_ADDR,
        CONTRACT_DELEGATE,
        runtime_args! {
            ARG_AMOUNT => U512::from(DELEGATE_AMOUNT_3),
            ARG_VALIDATOR => VALIDATOR_1,
            ARG_DELEGATOR => VALIDATOR_2,
        },
    )
    .build();

    builder
        .exec(delegate_exec_request_1)
        .expect_success()
        .commit();

    builder
        .exec(delegate_exec_request_2)
        .expect_success()
        .commit();

    builder
        .exec(delegate_exec_request_3)
        .expect_success()
        .commit();

    // Ensure that initial bid entries exist for validator 1 and validator 2
    let initial_bids: Bids = builder.get_value(auction, BIDS_KEY);
    assert_eq!(
        initial_bids.keys().copied().collect::<BTreeSet<_>>(),
        BTreeSet::from_iter(vec![VALIDATOR_2, VALIDATOR_1])
    );

    let initial_unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(initial_unbond_purses.len(), 0);

    // DELEGATOR_1 partially unbonds from VALIDATOR_1
    let undelegate_request_1 = ExecuteRequestBuilder::standard(
        *DELEGATOR_1_ADDR,
        CONTRACT_UNDELEGATE,
        runtime_args! {
            ARG_AMOUNT => U512::from(UNDELEGATE_AMOUNT_1),
            ARG_VALIDATOR => VALIDATOR_1,
            ARG_DELEGATOR => DELEGATOR_1,
            ARG_UNBOND_PURSE => Option::<URef>::None,
        },
    )
    .build();

    // DELEGATOR_1 partially unbonds from VALIDATOR_2
    let undelegate_request_2 = ExecuteRequestBuilder::standard(
        *DELEGATOR_1_ADDR,
        CONTRACT_UNDELEGATE,
        runtime_args! {
            ARG_AMOUNT => U512::from(UNDELEGATE_AMOUNT_2),
            ARG_VALIDATOR => VALIDATOR_2,
            ARG_DELEGATOR => DELEGATOR_1,
            ARG_UNBOND_PURSE => Option::<URef>::None,
        },
    )
    .build();

    // VALIDATOR_2 partially unbonds from VALIDATOR_1
    let undelegate_request_3 = ExecuteRequestBuilder::standard(
        *VALIDATOR_2_ADDR,
        CONTRACT_UNDELEGATE,
        runtime_args! {
            ARG_AMOUNT => U512::from(UNDELEGATE_AMOUNT_3),
            ARG_VALIDATOR => VALIDATOR_1,
            ARG_DELEGATOR => VALIDATOR_2,
            ARG_UNBOND_PURSE => Option::<URef>::None,
        },
    )
    .build();

    builder.exec(undelegate_request_1).commit().expect_success();
    builder.exec(undelegate_request_2).commit().expect_success();
    builder.exec(undelegate_request_3).commit().expect_success();

    // Check unbonding purses before slashing

    let unbond_purses_before: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses_before.len(), 2);

    let validator_1_unbond_list_before = unbond_purses_before
        .get(&VALIDATOR_1)
        .cloned()
        .expect("should have unbond");
    assert_eq!(validator_1_unbond_list_before.len(), 2); // two entries in order: undelegate, and withdraw bid

    // Added through `undelegate_request_1`
    assert_eq!(
        validator_1_unbond_list_before[0].validator_public_key(),
        &VALIDATOR_1
    );
    assert_eq!(
        validator_1_unbond_list_before[0].unbonder_public_key(),
        &DELEGATOR_1
    );
    assert_eq!(
        validator_1_unbond_list_before[0].amount(),
        &U512::from(UNDELEGATE_AMOUNT_1)
    );

    // Added through `undelegate_request_3`
    assert_eq!(
        validator_1_unbond_list_before[1].validator_public_key(),
        &VALIDATOR_1
    );
    assert_eq!(
        validator_1_unbond_list_before[1].unbonder_public_key(),
        &VALIDATOR_2
    );
    assert_eq!(
        validator_1_unbond_list_before[1].amount(),
        &U512::from(UNDELEGATE_AMOUNT_3)
    );

    let validator_2_unbond_list = unbond_purses_before
        .get(&VALIDATOR_2)
        .cloned()
        .expect("should have unbond");

    assert_eq!(validator_2_unbond_list.len(), 1); // one entry: undelegate
    assert_eq!(
        validator_2_unbond_list[0].validator_public_key(),
        &VALIDATOR_2
    );
    assert_eq!(
        validator_2_unbond_list[0].unbonder_public_key(),
        &DELEGATOR_1
    );
    assert_eq!(
        validator_2_unbond_list[0].amount(),
        &U512::from(UNDELEGATE_AMOUNT_2),
    );

    // Check bids before slashing

    let bids_before: Bids = builder.get_value(auction, BIDS_KEY);
    assert_eq!(
        bids_before.keys().collect::<Vec<_>>(),
        initial_bids.keys().collect::<Vec<_>>()
    );

    let slash_request_1 = ExecuteRequestBuilder::contract_call_by_hash(
        SYSTEM_ADDR,
        auction,
        METHOD_SLASH,
        runtime_args! {
            ARG_VALIDATOR_PUBLIC_KEYS => vec![
                VALIDATOR_2
            ]
        },
    )
    .build();

    builder.exec(slash_request_1).expect_success().commit();

    // Compare bids after slashing validator 2
    let bids_after: Bids = builder.get_value(auction, BIDS_KEY);
    assert_ne!(bids_before, bids_after);
    assert_eq!(bids_after.len(), 1);
    assert!(!bids_after.contains_key(&VALIDATOR_2));

    assert!(bids_after.contains_key(&VALIDATOR_1));
    assert_eq!(bids_after[&VALIDATOR_1].delegators().len(), 2);

    // validator 2's delegation bid on validator 1 was not slashed.
    assert!(bids_after[&VALIDATOR_1]
        .delegators()
        .contains_key(&VALIDATOR_2));
    assert!(bids_after[&VALIDATOR_1]
        .delegators()
        .contains_key(&DELEGATOR_1));

    let unbond_purses_after: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_ne!(unbond_purses_before, unbond_purses_after);

    let validator_1_unbond_list_after = unbond_purses_after
        .get(&VALIDATOR_1)
        .expect("should have validator 1 entry");
    assert_eq!(validator_1_unbond_list_after.len(), 2);
    assert_eq!(
        validator_1_unbond_list_after[0].unbonder_public_key(),
        &DELEGATOR_1
    );

    // validator 2's delegation unbond from validator 1 was not slashed
    assert_eq!(
        validator_1_unbond_list_after[1].unbonder_public_key(),
        &VALIDATOR_2
    );

    // delegator 1 had a delegation unbond slashed for validator 2's behavior.
    // delegator 1 still has an active delegation unbond from validator 2.
    assert_eq!(
        validator_1_unbond_list_after,
        &validator_1_unbond_list_before
    );

    // slash validator 1 to clear remaining bids and unbonding purses
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

    let bids_after: Bids = builder.get_value(auction, BIDS_KEY);
    assert!(bids_after.is_empty());
    let unbond_purses_after: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert!(unbond_purses_after.is_empty());
}
