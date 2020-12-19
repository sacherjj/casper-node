use once_cell::sync::Lazy;

use casper_engine_test_support::{
    internal::{utils, ExecuteRequestBuilder, InMemoryWasmTestBuilder, DEFAULT_ACCOUNTS},
    MINIMUM_ACCOUNT_CREATION_BALANCE,
};
use casper_execution_engine::{core::engine_state::GenesisAccount, shared::motes::Motes};
use casper_types::{
    account::AccountHash,
    auction::{ARG_DELEGATOR, ARG_VALIDATOR},
    runtime_args, PublicKey, RuntimeArgs, U512,
};

const ARG_TARGET: &str = "target";
const ARG_AMOUNT: &str = "amount";

const CONTRACT_TRANSFER_TO_ACCOUNT: &str = "transfer_to_account_u512.wasm";
const CONTRACT_DELEGATE: &str = "delegate.wasm";
const TRANSFER_AMOUNT: u64 = MINIMUM_ACCOUNT_CREATION_BALANCE;
const SYSTEM_ADDR: AccountHash = AccountHash::new([0u8; 32]);

const FAUCET: PublicKey = PublicKey::Ed25519([1; 32]);
const VALIDATOR_1: PublicKey = PublicKey::Ed25519([3; 32]);
const VALIDATOR_2: PublicKey = PublicKey::Ed25519([5; 32]);
const VALIDATOR_3: PublicKey = PublicKey::Ed25519([7; 32]);
const DELEGATOR_1: PublicKey = PublicKey::Ed25519([204; 32]);
const DELEGATOR_2: PublicKey = PublicKey::Ed25519([206; 32]);
const DELEGATOR_3: PublicKey = PublicKey::Ed25519([208; 32]);

// These values were chosen to correspond to the values in accounts.csv
// at the time of their introduction.

static FAUCET_ADDR: Lazy<AccountHash> = Lazy::new(|| FAUCET.into());
static VALIDATOR_1_ADDR: Lazy<AccountHash> = Lazy::new(|| VALIDATOR_1.into());
static VALIDATOR_2_ADDR: Lazy<AccountHash> = Lazy::new(|| VALIDATOR_2.into());
static VALIDATOR_3_ADDR: Lazy<AccountHash> = Lazy::new(|| VALIDATOR_3.into());
static FAUCET_BALANCE: Lazy<U512> = Lazy::new(|| U512::from(100_000_000_000_000_000u64));
static VALIDATOR_1_BALANCE: Lazy<U512> = Lazy::new(|| U512::from(100_000_000_000_000_000u64));
static VALIDATOR_2_BALANCE: Lazy<U512> = Lazy::new(|| U512::from(100_000_000_000_000_000u64));
static VALIDATOR_3_BALANCE: Lazy<U512> = Lazy::new(|| U512::from(100_000_000_000_000_000u64));
static VALIDATOR_1_STAKE: Lazy<U512> = Lazy::new(|| U512::from(500_000_000_000_000_000u64));
static VALIDATOR_2_STAKE: Lazy<U512> = Lazy::new(|| U512::from(400_000_000_000_000u64));
static VALIDATOR_3_STAKE: Lazy<U512> = Lazy::new(|| U512::from(300_000_000_000_000u64));
static DELEGATOR_1_ADDR: Lazy<AccountHash> = Lazy::new(|| DELEGATOR_1.into());
static DELEGATOR_2_ADDR: Lazy<AccountHash> = Lazy::new(|| DELEGATOR_2.into());
static DELEGATOR_3_ADDR: Lazy<AccountHash> = Lazy::new(|| DELEGATOR_3.into());
static DELEGATOR_1_BALANCE: Lazy<U512> = Lazy::new(|| U512::from(1_000_000_000_000_000u64));
static DELEGATOR_2_BALANCE: Lazy<U512> = Lazy::new(|| U512::from(1_000_000_000_000_000u64));
static DELEGATOR_3_BALANCE: Lazy<U512> = Lazy::new(|| U512::from(1_000_000_000_000_000u64));
static DELEGATOR_1_STAKE: Lazy<U512> = Lazy::new(|| U512::from(500_000_000_000_000u64));
static DELEGATOR_2_STAKE: Lazy<U512> = Lazy::new(|| U512::from(400_000_000_000_000u64));
static DELEGATOR_3_STAKE: Lazy<U512> = Lazy::new(|| U512::from(300_000_000_000_000u64));

#[ignore]
#[test]
fn validator_scores_should_reflect_delegates() {
    let accounts = {
        let faucet = GenesisAccount::new(
            FAUCET,
            *FAUCET_ADDR,
            Motes::new(*FAUCET_BALANCE),
            Motes::new(U512::zero()),
        );
        let validator_1 = GenesisAccount::new(
            VALIDATOR_1,
            *VALIDATOR_1_ADDR,
            Motes::new(*VALIDATOR_1_BALANCE),
            Motes::new(*VALIDATOR_1_STAKE),
        );
        let validator_2 = GenesisAccount::new(
            VALIDATOR_2,
            *VALIDATOR_2_ADDR,
            Motes::new(*VALIDATOR_2_BALANCE),
            Motes::new(*VALIDATOR_2_STAKE),
        );
        let validator_3 = GenesisAccount::new(
            VALIDATOR_3,
            *VALIDATOR_3_ADDR,
            Motes::new(*VALIDATOR_3_BALANCE),
            Motes::new(*VALIDATOR_3_STAKE),
        );
        let mut tmp: Vec<GenesisAccount> = DEFAULT_ACCOUNTS.clone();
        tmp.push(faucet);
        tmp.push(validator_1);
        tmp.push(validator_2);
        tmp.push(validator_3);
        tmp
    };

    let system_fund_request = ExecuteRequestBuilder::standard(
        *FAUCET_ADDR,
        CONTRACT_TRANSFER_TO_ACCOUNT,
        runtime_args! {
            ARG_TARGET => SYSTEM_ADDR,
            ARG_AMOUNT => U512::from(TRANSFER_AMOUNT)
        },
    )
    .build();

    let delegator_1_fund_request = ExecuteRequestBuilder::standard(
        *FAUCET_ADDR,
        CONTRACT_TRANSFER_TO_ACCOUNT,
        runtime_args! {
            ARG_TARGET => *DELEGATOR_1_ADDR,
            ARG_AMOUNT => *DELEGATOR_1_BALANCE
        },
    )
    .build();

    let delegator_2_fund_request = ExecuteRequestBuilder::standard(
        *FAUCET_ADDR,
        CONTRACT_TRANSFER_TO_ACCOUNT,
        runtime_args! {
            ARG_TARGET => *DELEGATOR_2_ADDR,
            ARG_AMOUNT => *DELEGATOR_2_BALANCE
        },
    )
    .build();

    let delegator_3_fund_request = ExecuteRequestBuilder::standard(
        *FAUCET_ADDR,
        CONTRACT_TRANSFER_TO_ACCOUNT,
        runtime_args! {
            ARG_TARGET => *DELEGATOR_3_ADDR,
            ARG_AMOUNT => *DELEGATOR_3_BALANCE
        },
    )
    .build();

    let post_genesis_requests = vec![
        system_fund_request,
        delegator_1_fund_request,
        delegator_2_fund_request,
        delegator_3_fund_request,
    ];

    let mut builder = InMemoryWasmTestBuilder::default();

    let run_genesis_request = utils::create_run_genesis_request(accounts);
    builder.run_genesis(&run_genesis_request);

    for request in post_genesis_requests {
        builder.exec(request).commit().expect_success();
    }

    let mut era = builder.get_era();
    let auction_delay = builder.get_auction_delay();

    // Check initial weights
    {
        let era_weights = builder
            .get_validator_weights(era)
            .expect("should get validator weights");

        assert_eq!(era_weights.get(&VALIDATOR_1), Some(&*VALIDATOR_1_STAKE));
        assert_eq!(era_weights.get(&VALIDATOR_2), Some(&*VALIDATOR_2_STAKE));
        assert_eq!(era_weights.get(&VALIDATOR_3), Some(&*VALIDATOR_3_STAKE));
    }

    // Check weights after auction_delay eras
    {
        for _ in 0..=auction_delay {
            builder.run_auction();
        }

        era = builder.get_era();
        assert_eq!(builder.get_auction_delay(), auction_delay);

        let era_weights = builder
            .get_validator_weights(era + auction_delay)
            .expect("should get validator weights");

        assert_eq!(era_weights.get(&VALIDATOR_1), Some(&*VALIDATOR_1_STAKE));
        assert_eq!(era_weights.get(&VALIDATOR_2), Some(&*VALIDATOR_2_STAKE));
        assert_eq!(era_weights.get(&VALIDATOR_3), Some(&*VALIDATOR_3_STAKE));
    }

    // Check weights after Delegator 1 delegates to Validator 1 (and auction_delay)
    {
        let delegator_1_delegate_request = ExecuteRequestBuilder::standard(
            *DELEGATOR_1_ADDR,
            CONTRACT_DELEGATE,
            runtime_args! {
                ARG_AMOUNT => *DELEGATOR_1_STAKE,
                ARG_VALIDATOR => VALIDATOR_1,
                ARG_DELEGATOR => DELEGATOR_1,
            },
        )
        .build();

        builder
            .exec(delegator_1_delegate_request)
            .commit()
            .expect_success();

        for _ in 0..=auction_delay {
            builder.run_auction();
        }

        era = builder.get_era();
        assert_eq!(builder.get_auction_delay(), auction_delay);

        let era_weights = builder
            .get_validator_weights(era)
            .expect("should get validator weights");

        let validator_1_expected_stake = *VALIDATOR_1_STAKE + *DELEGATOR_1_STAKE;

        let validator_2_expected_stake = *VALIDATOR_2_STAKE;

        let validator_3_expected_stake = *VALIDATOR_3_STAKE;

        assert_eq!(
            era_weights.get(&VALIDATOR_1),
            Some(&validator_1_expected_stake)
        );
        assert_eq!(
            era_weights.get(&VALIDATOR_2),
            Some(&validator_2_expected_stake)
        );
        assert_eq!(
            era_weights.get(&VALIDATOR_3),
            Some(&validator_3_expected_stake)
        );
    }

    // Check weights after Delegator 2 delegates to Validator 1 (and auction_delay)
    {
        let delegator_2_delegate_request = ExecuteRequestBuilder::standard(
            *DELEGATOR_2_ADDR,
            CONTRACT_DELEGATE,
            runtime_args! {
                ARG_AMOUNT => *DELEGATOR_2_STAKE,
                ARG_VALIDATOR => VALIDATOR_1,
                ARG_DELEGATOR => DELEGATOR_2,
            },
        )
        .build();

        builder
            .exec(delegator_2_delegate_request)
            .commit()
            .expect_success();

        for _ in 0..=auction_delay {
            builder.run_auction();
        }

        era = builder.get_era();
        assert_eq!(builder.get_auction_delay(), auction_delay);

        let era_weights = builder
            .get_validator_weights(era)
            .expect("should get validator weights");

        let validator_1_expected_stake =
            *VALIDATOR_1_STAKE + *DELEGATOR_1_STAKE + *DELEGATOR_2_STAKE;

        let validator_2_expected_stake = *VALIDATOR_2_STAKE;

        let validator_3_expected_stake = *VALIDATOR_3_STAKE;

        assert_eq!(
            era_weights.get(&VALIDATOR_1),
            Some(&validator_1_expected_stake)
        );
        assert_eq!(
            era_weights.get(&VALIDATOR_2),
            Some(&validator_2_expected_stake)
        );
        assert_eq!(
            era_weights.get(&VALIDATOR_3),
            Some(&validator_3_expected_stake)
        );
    }

    // Check weights after Delegator 3 delegates to Validator 2 (and auction_delay)
    {
        let delegator_3_delegate_request = ExecuteRequestBuilder::standard(
            *DELEGATOR_3_ADDR,
            CONTRACT_DELEGATE,
            runtime_args! {
                ARG_AMOUNT => *DELEGATOR_3_STAKE,
                ARG_VALIDATOR => VALIDATOR_2,
                ARG_DELEGATOR => DELEGATOR_3,
            },
        )
        .build();

        builder
            .exec(delegator_3_delegate_request)
            .commit()
            .expect_success();

        for _ in 0..=auction_delay {
            builder.run_auction();
        }
        era = builder.get_era();
        assert_eq!(builder.get_auction_delay(), auction_delay);

        let era_weights = builder
            .get_validator_weights(era)
            .expect("should get validator weights");

        let validator_1_expected_stake =
            *VALIDATOR_1_STAKE + *DELEGATOR_1_STAKE + *DELEGATOR_2_STAKE;

        let validator_2_expected_stake = *VALIDATOR_2_STAKE + *DELEGATOR_3_STAKE;

        let validator_3_expected_stake = *VALIDATOR_3_STAKE;

        assert_eq!(
            era_weights.get(&VALIDATOR_1),
            Some(&validator_1_expected_stake)
        );
        assert_eq!(
            era_weights.get(&VALIDATOR_2),
            Some(&validator_2_expected_stake)
        );
        assert_eq!(
            era_weights.get(&VALIDATOR_3),
            Some(&validator_3_expected_stake)
        );
    }
}
