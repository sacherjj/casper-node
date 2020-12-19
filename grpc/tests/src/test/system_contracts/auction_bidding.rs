use assert_matches::assert_matches;

use casper_engine_test_support::{
    internal::{
        utils, ExecuteRequestBuilder, InMemoryWasmTestBuilder, UpgradeRequestBuilder,
        DEFAULT_ACCOUNTS, DEFAULT_ACCOUNT_PUBLIC_KEY, DEFAULT_PAYMENT, DEFAULT_PROTOCOL_VERSION,
        DEFAULT_RUN_GENESIS_REQUEST, DEFAULT_UNBONDING_DELAY,
    },
    DEFAULT_ACCOUNT_ADDR, MINIMUM_ACCOUNT_CREATION_BALANCE,
};
use casper_execution_engine::{
    core::{
        engine_state::{genesis::GenesisAccount, Error as EngineError},
        execution::Error,
    },
    shared::motes::Motes,
};
use casper_types::{
    account::AccountHash,
    auction::{
        Bids, DelegationRate, EraId, UnbondingPurses, ARG_UNBOND_PURSE, ARG_VALIDATOR_PUBLIC_KEYS,
        BIDS_KEY, INITIAL_ERA_ID, METHOD_RUN_AUCTION, METHOD_SLASH, UNBONDING_PURSES_KEY,
    },
    runtime_args,
    system_contract_errors::auction,
    ApiError, ProtocolVersion, PublicKey, RuntimeArgs, URef, U512,
};

const CONTRACT_TRANSFER_TO_ACCOUNT: &str = "transfer_to_account_u512.wasm";
const CONTRACT_ADD_BID: &str = "add_bid.wasm";
const CONTRACT_WITHDRAW_BID: &str = "withdraw_bid.wasm";
const CONTRACT_AUCTION_BIDDING: &str = "auction_bidding.wasm";
const CONTRACT_AUCTION_BIDS: &str = "auction_bids.wasm";
const CONTRACT_CREATE_PURSE_01: &str = "create_purse_01.wasm";

const GENESIS_VALIDATOR_STAKE: u64 = 50_000;
const GENESIS_ACCOUNT_STAKE: u64 = 100_000;
const TRANSFER_AMOUNT: u64 = MINIMUM_ACCOUNT_CREATION_BALANCE;

const TEST_BOND_FROM_MAIN_PURSE: &str = "bond-from-main-purse";
const TEST_SEED_NEW_ACCOUNT: &str = "seed_new_account";

const ARG_AMOUNT: &str = "amount";
const ARG_PUBLIC_KEY: &str = "public_key";
const ARG_ENTRY_POINT: &str = "entry_point";
const ARG_ACCOUNT_HASH: &str = "account_hash";
const ARG_RUN_AUCTION: &str = "run_auction";
const ARG_DELEGATION_RATE: &str = "delegation_rate";
const ARG_PURSE_NAME: &str = "purse_name";

const SYSTEM_ADDR: AccountHash = AccountHash::new([0u8; 32]);
const UNBONDING_PURSE_NAME: &str = "unbonding_purse";

#[ignore]
#[test]
fn should_run_successful_bond_and_unbond_and_slashing() {
    let default_public_key_arg = *DEFAULT_ACCOUNT_PUBLIC_KEY;
    let mut builder = InMemoryWasmTestBuilder::default();
    builder.run_genesis(&DEFAULT_RUN_GENESIS_REQUEST);

    let exec_request = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_TRANSFER_TO_ACCOUNT,
        runtime_args! {
            "target" => SYSTEM_ADDR,
            "amount" => U512::from(TRANSFER_AMOUNT)
        },
    )
    .build();

    builder.exec(exec_request).expect_success().commit();

    let _default_account = builder
        .get_account(*DEFAULT_ACCOUNT_ADDR)
        .expect("should get account 1");

    let auction = builder.get_auction_contract_hash();

    let exec_request_1 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_ADD_BID,
        runtime_args! {
            ARG_AMOUNT => U512::from(GENESIS_ACCOUNT_STAKE),
            ARG_PUBLIC_KEY => default_public_key_arg,
            ARG_DELEGATION_RATE => DelegationRate::from(42u8),
        },
    )
    .build();

    builder.exec(exec_request_1).expect_success().commit();

    let bids: Bids = builder.get_value(auction, BIDS_KEY);
    let default_account_bid = bids
        .get(&*DEFAULT_ACCOUNT_PUBLIC_KEY)
        .expect("should have bid");
    let bid_purse = *default_account_bid.bonding_purse();
    assert_eq!(
        builder.get_purse_balance(bid_purse),
        GENESIS_ACCOUNT_STAKE.into()
    );

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 0);

    //
    // Partial unbond
    //

    let unbond_amount = U512::from(GENESIS_ACCOUNT_STAKE) - 1;

    let exec_request_2 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_CREATE_PURSE_01,
        runtime_args! {
            ARG_PURSE_NAME => UNBONDING_PURSE_NAME,
        },
    )
    .build();

    builder.exec(exec_request_2).expect_success().commit();
    let unbonding_purse = builder
        .get_account(*DEFAULT_ACCOUNT_ADDR)
        .expect("should have default account")
        .named_keys()
        .get(UNBONDING_PURSE_NAME)
        .expect("should have unbonding purse")
        .into_uref()
        .expect("unbonding purse should be an uref");

    let exec_request_3 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_WITHDRAW_BID,
        runtime_args! {
            ARG_AMOUNT => unbond_amount,
            ARG_PUBLIC_KEY => default_public_key_arg,
            ARG_UNBOND_PURSE => Some(unbonding_purse),
        },
    )
    .build();

    builder.exec(exec_request_3).expect_success().commit();

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 1);

    let unbond_list = unbond_purses
        .get(&*DEFAULT_ACCOUNT_PUBLIC_KEY)
        .expect("should have unbond");
    assert_eq!(unbond_list.len(), 1);
    assert_eq!(
        unbond_list[0].validator_public_key(),
        &default_public_key_arg,
    );
    assert!(unbond_list[0].is_validator());
    assert_eq!(
        builder.get_purse_balance(*unbond_list[0].unbonding_purse()),
        U512::zero(),
    );

    assert_eq!(unbond_list[0].era_of_creation(), INITIAL_ERA_ID,);

    let unbond_era_1 = unbond_list[0].era_of_creation();

    let exec_request_3 = ExecuteRequestBuilder::contract_call_by_hash(
        SYSTEM_ADDR,
        auction,
        METHOD_RUN_AUCTION,
        runtime_args! {},
    )
    .build();

    builder.exec(exec_request_3).expect_success().commit();

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 1);

    let unbond_list = unbond_purses
        .get(&*DEFAULT_ACCOUNT_PUBLIC_KEY)
        .expect("should have unbond");
    assert_eq!(unbond_list.len(), 1);
    assert_eq!(
        unbond_list[0].validator_public_key(),
        &default_public_key_arg,
    );
    assert!(unbond_list[0].is_validator());
    assert_eq!(
        builder.get_purse_balance(*unbond_list[0].unbonding_purse()),
        U512::zero(),
    );
    assert_eq!(unbond_list[0].amount(), &unbond_amount,);

    let unbond_era_2 = unbond_list[0].era_of_creation();

    assert_eq!(unbond_era_2, unbond_era_1);

    let exec_request_4 = ExecuteRequestBuilder::contract_call_by_hash(
        SYSTEM_ADDR,
        auction,
        METHOD_SLASH,
        runtime_args! {
            ARG_VALIDATOR_PUBLIC_KEYS => vec![
               default_public_key_arg,
            ]
        },
    )
    .build();

    builder.exec(exec_request_4).expect_success().commit();

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert!(
        !unbond_purses.contains_key(&*DEFAULT_ACCOUNT_PUBLIC_KEY),
        "should remove slashed from unbonds"
    );

    let bids: Bids = builder.get_value(auction, BIDS_KEY);
    assert!(bids.is_empty());
}

#[ignore]
#[test]
fn should_fail_bonding_with_insufficient_funds() {
    let account_1_public_key: PublicKey = PublicKey::Ed25519([123; 32]);
    let account_1_hash = AccountHash::from(account_1_public_key);

    let exec_request_1 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_AUCTION_BIDDING,
        runtime_args! {
            ARG_ENTRY_POINT => TEST_SEED_NEW_ACCOUNT,
            ARG_ACCOUNT_HASH => account_1_hash,
            ARG_AMOUNT => *DEFAULT_PAYMENT + GENESIS_ACCOUNT_STAKE,
        },
    )
    .build();
    let exec_request_2 = ExecuteRequestBuilder::standard(
        account_1_hash,
        CONTRACT_AUCTION_BIDDING,
        runtime_args! {
            ARG_ENTRY_POINT => TEST_BOND_FROM_MAIN_PURSE,
            ARG_AMOUNT => *DEFAULT_PAYMENT + GENESIS_ACCOUNT_STAKE,
            ARG_PUBLIC_KEY => account_1_public_key,
        },
    )
    .build();

    let mut builder = InMemoryWasmTestBuilder::default();

    builder
        .run_genesis(&DEFAULT_RUN_GENESIS_REQUEST)
        .exec(exec_request_1)
        .commit();

    builder.exec(exec_request_2).commit();

    let response = builder
        .get_exec_response(1)
        .expect("should have a response")
        .to_owned();

    assert_eq!(response.len(), 1);
    let exec_result = response[0].as_error().expect("should have error");
    let error = assert_matches!(exec_result, EngineError::Exec(Error::Revert(e)) => *e, "{:?}", exec_result);
    assert_eq!(error, ApiError::from(auction::Error::TransferToBidPurse));
}

#[ignore]
#[test]
fn should_fail_unbonding_validator_with_locked_funds() {
    let account_1_public_key = PublicKey::Ed25519([42; 32]);
    let account_1_hash = AccountHash::from(account_1_public_key);
    let account_1_balance = U512::from(MINIMUM_ACCOUNT_CREATION_BALANCE);

    let accounts = {
        let mut tmp: Vec<GenesisAccount> = DEFAULT_ACCOUNTS.clone();
        let account = GenesisAccount::new(
            account_1_public_key,
            account_1_hash,
            Motes::new(account_1_balance),
            Motes::new(GENESIS_VALIDATOR_STAKE.into()),
        );
        tmp.push(account);
        tmp
    };

    let run_genesis_request = utils::create_run_genesis_request(accounts);

    let exec_request_1 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_CREATE_PURSE_01,
        runtime_args! {
            ARG_PURSE_NAME => UNBONDING_PURSE_NAME,
        },
    )
    .build();

    let mut builder = InMemoryWasmTestBuilder::default();

    builder.run_genesis(&run_genesis_request);

    builder.exec(exec_request_1).expect_success().commit();

    let unbonding_purse = builder
        .get_account(*DEFAULT_ACCOUNT_ADDR)
        .expect("should have default account")
        .named_keys()
        .get(UNBONDING_PURSE_NAME)
        .expect("should have unbonding purse")
        .into_uref()
        .expect("unbonding purse should be an uref");

    let exec_request_2 = ExecuteRequestBuilder::standard(
        account_1_hash,
        CONTRACT_WITHDRAW_BID,
        runtime_args! {
            ARG_AMOUNT => U512::from(42),
            ARG_PUBLIC_KEY => account_1_public_key,
            ARG_UNBOND_PURSE => Some(unbonding_purse)
        },
    )
    .build();

    builder.exec(exec_request_2).commit();

    let response = builder
        .get_exec_response(1)
        .expect("should have a response")
        .to_owned();

    let error_message = utils::get_error_message(response);

    // pos::Error::NotBonded => 0
    assert!(
        error_message.contains(&format!(
            "{:?}",
            ApiError::from(auction::Error::ValidatorFundsLocked)
        )),
        "error {:?}",
        error_message
    );
}

#[ignore]
#[test]
fn should_fail_unbonding_validator_without_bonding_first() {
    let exec_request = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_WITHDRAW_BID,
        runtime_args! {
            ARG_AMOUNT => U512::from(42),
            ARG_PUBLIC_KEY => *DEFAULT_ACCOUNT_PUBLIC_KEY,
            ARG_UNBOND_PURSE => Option::<URef>::None,
        },
    )
    .build();

    let mut builder = InMemoryWasmTestBuilder::default();

    builder.run_genesis(&DEFAULT_RUN_GENESIS_REQUEST);

    builder.exec(exec_request).commit();

    let response = builder
        .get_exec_response(0)
        .expect("should have a response")
        .to_owned();

    let error_message = utils::get_error_message(response);

    assert!(
        error_message.contains(&format!(
            "{:?}",
            ApiError::from(auction::Error::ValidatorNotFound)
        )),
        "error {:?}",
        error_message
    );
}

#[ignore]
#[test]
fn should_run_successful_bond_and_unbond_with_release() {
    let default_public_key_arg = *DEFAULT_ACCOUNT_PUBLIC_KEY;

    let mut builder = InMemoryWasmTestBuilder::default();
    builder.run_genesis(&DEFAULT_RUN_GENESIS_REQUEST);

    let create_purse_request_1 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_CREATE_PURSE_01,
        runtime_args! {
            ARG_PURSE_NAME => UNBONDING_PURSE_NAME,
        },
    )
    .build();

    builder
        .exec(create_purse_request_1)
        .expect_success()
        .commit();
    let unbonding_purse = builder
        .get_account(*DEFAULT_ACCOUNT_ADDR)
        .expect("should have default account")
        .named_keys()
        .get(UNBONDING_PURSE_NAME)
        .expect("should have unbonding purse")
        .into_uref()
        .expect("unbonding purse should be an uref");

    let exec_request = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_TRANSFER_TO_ACCOUNT,
        runtime_args! {
            "target" => SYSTEM_ADDR,
            "amount" => U512::from(TRANSFER_AMOUNT)
        },
    )
    .build();

    builder.exec(exec_request).expect_success().commit();

    let _default_account = builder
        .get_account(*DEFAULT_ACCOUNT_ADDR)
        .expect("should get account 1");

    let auction = builder.get_auction_contract_hash();

    let exec_request_1 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_ADD_BID,
        runtime_args! {
            ARG_AMOUNT => U512::from(GENESIS_ACCOUNT_STAKE),
            ARG_PUBLIC_KEY => default_public_key_arg,
            ARG_DELEGATION_RATE => DelegationRate::from(42u8),
        },
    )
    .build();

    builder.exec(exec_request_1).expect_success().commit();

    let bids: Bids = builder.get_value(auction, BIDS_KEY);
    let bid = bids.get(&default_public_key_arg).expect("should have bid");
    let bid_purse = *bid.bonding_purse();
    assert_eq!(
        builder.get_purse_balance(bid_purse),
        GENESIS_ACCOUNT_STAKE.into()
    );

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 0);

    //
    // Advance era by calling run_auction
    //
    let run_auction_request_1 = ExecuteRequestBuilder::standard(
        SYSTEM_ADDR,
        CONTRACT_AUCTION_BIDS,
        runtime_args! {
            ARG_ENTRY_POINT => ARG_RUN_AUCTION,
        },
    )
    .build();

    builder
        .exec(run_auction_request_1)
        .commit()
        .expect_success();

    //
    // Partial unbond
    //

    let unbond_amount = U512::from(GENESIS_ACCOUNT_STAKE) - 1;

    let exec_request_2 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_WITHDRAW_BID,
        runtime_args! {
            ARG_AMOUNT => unbond_amount,
            ARG_PUBLIC_KEY => default_public_key_arg,
            ARG_UNBOND_PURSE => Some(unbonding_purse),
        },
    )
    .build();

    builder.exec(exec_request_2).expect_success().commit();

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 1);

    let unbond_list = unbond_purses
        .get(&*DEFAULT_ACCOUNT_PUBLIC_KEY)
        .expect("should have unbond");
    assert_eq!(unbond_list.len(), 1);
    assert_eq!(
        unbond_list[0].validator_public_key(),
        &default_public_key_arg,
    );
    assert!(unbond_list[0].is_validator());
    assert_eq!(
        builder.get_purse_balance(*unbond_list[0].unbonding_purse()),
        U512::zero(),
    );

    assert_eq!(unbond_list[0].era_of_creation(), INITIAL_ERA_ID + 1);

    let unbond_era_1 = unbond_list[0].era_of_creation();

    let exec_request_3 = ExecuteRequestBuilder::contract_call_by_hash(
        SYSTEM_ADDR,
        auction,
        METHOD_RUN_AUCTION,
        runtime_args! {},
    )
    .build();

    builder.exec(exec_request_3).expect_success().commit();

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 1);

    let unbond_list = unbond_purses
        .get(&default_public_key_arg)
        .expect("should have unbond");
    assert_eq!(unbond_list.len(), 1);
    assert_eq!(
        unbond_list[0].validator_public_key(),
        &default_public_key_arg,
    );
    assert!(unbond_list[0].is_validator());
    assert_eq!(unbond_list[0].unbonding_purse(), &unbonding_purse,);
    assert_eq!(
        builder.get_purse_balance(unbonding_purse),
        U512::zero(), // Not paid yet
    );

    let unbond_era_2 = unbond_list[0].era_of_creation();

    assert_eq!(unbond_era_2, unbond_era_1); // era of withdrawal didn't change since first run

    //
    // Advance state to hit the unbonding period
    //

    for _ in 0..DEFAULT_UNBONDING_DELAY {
        let run_auction_request_1 = ExecuteRequestBuilder::standard(
            SYSTEM_ADDR,
            CONTRACT_AUCTION_BIDS,
            runtime_args! {
                ARG_ENTRY_POINT => ARG_RUN_AUCTION,
            },
        )
        .build();

        builder
            .exec(run_auction_request_1)
            .commit()
            .expect_success();
    }

    // Should pay out

    let exec_request_4 = ExecuteRequestBuilder::contract_call_by_hash(
        SYSTEM_ADDR,
        auction,
        METHOD_RUN_AUCTION,
        runtime_args! {},
    )
    .build();

    builder.exec(exec_request_4).expect_success().commit();

    assert_eq!(builder.get_purse_balance(unbonding_purse), unbond_amount);

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert!(
        !unbond_purses.contains_key(&*DEFAULT_ACCOUNT_PUBLIC_KEY),
        "Unbond entry should be removed"
    );

    let bids: Bids = builder.get_value(auction, BIDS_KEY);
    assert!(!bids.is_empty());

    let bid = bids.get(&default_public_key_arg).expect("should have bid");
    let bid_purse = *bid.bonding_purse();
    assert_eq!(
        builder.get_purse_balance(bid_purse),
        U512::from(GENESIS_ACCOUNT_STAKE) - unbond_amount, // remaining funds
    );
}

#[ignore]
#[test]
fn should_run_successful_unbond_funds_after_changing_unbonding_delay() {
    let default_public_key_arg = *DEFAULT_ACCOUNT_PUBLIC_KEY;

    let mut builder = InMemoryWasmTestBuilder::default();
    builder.run_genesis(&DEFAULT_RUN_GENESIS_REQUEST);

    let new_unbonding_delay: EraId = DEFAULT_UNBONDING_DELAY + 5;

    let old_protocol_version = *DEFAULT_PROTOCOL_VERSION;
    let sem_ver = old_protocol_version.value();
    let new_protocol_version =
        ProtocolVersion::from_parts(sem_ver.major, sem_ver.minor, sem_ver.patch + 1);
    let default_activation_point = 0;

    let mut upgrade_request = {
        UpgradeRequestBuilder::new()
            .with_current_protocol_version(old_protocol_version)
            .with_new_protocol_version(new_protocol_version)
            .with_activation_point(default_activation_point)
            .with_new_unbonding_delay(new_unbonding_delay)
            .build()
    };

    builder.upgrade_with_upgrade_request(&mut upgrade_request);

    let create_purse_request_1 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_CREATE_PURSE_01,
        runtime_args! {
            ARG_PURSE_NAME => UNBONDING_PURSE_NAME,
        },
    )
    .with_protocol_version(new_protocol_version)
    .build();

    builder
        .exec(create_purse_request_1)
        .expect_success()
        .commit();
    let unbonding_purse = builder
        .get_account(*DEFAULT_ACCOUNT_ADDR)
        .expect("should have default account")
        .named_keys()
        .get(UNBONDING_PURSE_NAME)
        .expect("should have unbonding purse")
        .into_uref()
        .expect("unbonding purse should be an uref");

    let exec_request = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_TRANSFER_TO_ACCOUNT,
        runtime_args! {
            "target" => SYSTEM_ADDR,
            "amount" => U512::from(TRANSFER_AMOUNT)
        },
    )
    .with_protocol_version(new_protocol_version)
    .build();

    builder.exec(exec_request).expect_success().commit();

    let _default_account = builder
        .get_account(*DEFAULT_ACCOUNT_ADDR)
        .expect("should get account 1");

    let auction = builder.get_auction_contract_hash();

    let exec_request_1 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_ADD_BID,
        runtime_args! {
            ARG_AMOUNT => U512::from(GENESIS_ACCOUNT_STAKE),
            ARG_PUBLIC_KEY => default_public_key_arg,
            ARG_DELEGATION_RATE => DelegationRate::from(42u8),
        },
    )
    .with_protocol_version(new_protocol_version)
    .build();

    builder.exec(exec_request_1).expect_success().commit();

    let bids: Bids = builder.get_value(auction, BIDS_KEY);
    let bid = bids.get(&default_public_key_arg).expect("should have bid");
    let bid_purse = *bid.bonding_purse();
    assert_eq!(
        builder.get_purse_balance(bid_purse),
        GENESIS_ACCOUNT_STAKE.into()
    );

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 0);

    //
    // Advance era by calling run_auction
    //
    let run_auction_request_1 = ExecuteRequestBuilder::standard(
        SYSTEM_ADDR,
        CONTRACT_AUCTION_BIDS,
        runtime_args! {
            ARG_ENTRY_POINT => ARG_RUN_AUCTION,
        },
    )
    .with_protocol_version(new_protocol_version)
    .build();

    builder
        .exec(run_auction_request_1)
        .commit()
        .expect_success();

    //
    // Partial unbond
    //

    let unbond_amount = U512::from(GENESIS_ACCOUNT_STAKE) - 1;

    let exec_request_2 = ExecuteRequestBuilder::standard(
        *DEFAULT_ACCOUNT_ADDR,
        CONTRACT_WITHDRAW_BID,
        runtime_args! {
            ARG_AMOUNT => unbond_amount,
            ARG_PUBLIC_KEY => default_public_key_arg,
            ARG_UNBOND_PURSE => Some(unbonding_purse),
        },
    )
    .with_protocol_version(new_protocol_version)
    .build();

    builder.exec(exec_request_2).expect_success().commit();

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 1);

    let unbond_list = unbond_purses
        .get(&*DEFAULT_ACCOUNT_PUBLIC_KEY)
        .expect("should have unbond");
    assert_eq!(unbond_list.len(), 1);
    assert_eq!(
        unbond_list[0].validator_public_key(),
        &default_public_key_arg,
    );
    assert!(unbond_list[0].is_validator());
    assert_eq!(
        builder.get_purse_balance(*unbond_list[0].unbonding_purse()),
        U512::zero(),
    );

    assert_eq!(unbond_list[0].era_of_creation(), INITIAL_ERA_ID + 1);

    let unbond_era_1 = unbond_list[0].era_of_creation();

    let exec_request_3 = ExecuteRequestBuilder::contract_call_by_hash(
        SYSTEM_ADDR,
        auction,
        METHOD_RUN_AUCTION,
        runtime_args! {},
    )
    .with_protocol_version(new_protocol_version)
    .build();

    builder.exec(exec_request_3).expect_success().commit();

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert_eq!(unbond_purses.len(), 1);

    let unbond_list = unbond_purses
        .get(&default_public_key_arg)
        .expect("should have unbond");
    assert_eq!(unbond_list.len(), 1);
    assert_eq!(
        unbond_list[0].validator_public_key(),
        &default_public_key_arg,
    );
    assert!(unbond_list[0].is_validator());
    assert_eq!(unbond_list[0].unbonding_purse(), &unbonding_purse,);
    assert_eq!(
        builder.get_purse_balance(unbonding_purse),
        U512::zero(), // Not paid yet
    );

    let unbond_era_2 = unbond_list[0].era_of_creation();

    assert_eq!(unbond_era_2, unbond_era_1); // era of withdrawal didn't change since first run

    //
    // Advance state to hit the unbonding period
    //

    for _ in 0..DEFAULT_UNBONDING_DELAY {
        let run_auction_request = ExecuteRequestBuilder::standard(
            SYSTEM_ADDR,
            CONTRACT_AUCTION_BIDS,
            runtime_args! {
                ARG_ENTRY_POINT => ARG_RUN_AUCTION,
            },
        )
        .with_protocol_version(new_protocol_version)
        .build();

        builder.exec(run_auction_request).commit().expect_success();
    }

    // Won't pay out (yet) as we increased unbonding period
    let run_auction_request_1 = ExecuteRequestBuilder::contract_call_by_hash(
        SYSTEM_ADDR,
        auction,
        METHOD_RUN_AUCTION,
        runtime_args! {},
    )
    .with_protocol_version(new_protocol_version)
    .build();

    builder
        .exec(run_auction_request_1)
        .expect_success()
        .commit();

    // Not paid yet
    assert_eq!(
        builder.get_purse_balance(unbonding_purse),
        U512::zero(),
        "should not pay after reaching default unbond delay era"
    );

    // -1 below is the extra run auction above in `run_auction_request_1`
    for _ in 0..new_unbonding_delay - DEFAULT_UNBONDING_DELAY - 1 {
        let run_auction_request = ExecuteRequestBuilder::contract_call_by_hash(
            SYSTEM_ADDR,
            auction,
            METHOD_RUN_AUCTION,
            runtime_args! {},
        )
        .with_protocol_version(new_protocol_version)
        .build();

        builder.exec(run_auction_request).expect_success().commit();
    }

    assert_eq!(builder.get_purse_balance(unbonding_purse), unbond_amount);

    let unbond_purses: UnbondingPurses = builder.get_value(auction, UNBONDING_PURSES_KEY);
    assert!(
        !unbond_purses.contains_key(&*DEFAULT_ACCOUNT_PUBLIC_KEY),
        "Unbond entry should be removed"
    );

    let bids: Bids = builder.get_value(auction, BIDS_KEY);
    assert!(!bids.is_empty());

    let bid = bids.get(&default_public_key_arg).expect("should have bid");
    let bid_purse = *bid.bonding_purse();
    assert_eq!(
        builder.get_purse_balance(bid_purse),
        U512::from(GENESIS_ACCOUNT_STAKE) - unbond_amount, // remaining funds
    );
}
