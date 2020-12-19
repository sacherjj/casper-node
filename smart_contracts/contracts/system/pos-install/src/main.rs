#![no_std]
#![no_main]

extern crate alloc;

use alloc::{boxed::Box, string::ToString, vec};

use casper_contract::{
    contract_api::{runtime, storage},
    unwrap_or_revert::UnwrapOrRevert,
};
use casper_types::{
    contracts::{
        EntryPoint, EntryPointAccess, EntryPointType, EntryPoints, NamedKeys, Parameter,
        CONTRACT_INITIAL_VERSION,
    },
    proof_of_stake::{
        ACCESS_KEY, ARG_ACCOUNT, ARG_AMOUNT, ARG_PURSE, HASH_KEY, METHOD_FINALIZE_PAYMENT,
        METHOD_GET_PAYMENT_PURSE, METHOD_GET_REFUND_PURSE, METHOD_SET_REFUND_PURSE,
        POS_PAYMENT_PURSE,
    },
    runtime_args,
    system_contract_errors::mint,
    CLType, CLValue, ContractPackageHash, RuntimeArgs, URef, U512,
};

const ARG_MINT_PACKAGE_HASH: &str = "mint_contract_package_hash";
const ENTRY_POINT_MINT: &str = "mint";

#[no_mangle]
pub extern "C" fn get_payment_purse() {
    pos::get_payment_purse();
}

#[no_mangle]
pub extern "C" fn set_refund_purse() {
    pos::set_refund_purse();
}

#[no_mangle]
pub extern "C" fn get_refund_purse() {
    pos::get_refund_purse();
}

#[no_mangle]
pub extern "C" fn finalize_payment() {
    pos::finalize_payment();
}

#[no_mangle]
pub extern "C" fn install() {
    let mint_package_hash: ContractPackageHash = runtime::get_named_arg(ARG_MINT_PACKAGE_HASH);

    // Add genesis validators to PoS contract object.
    // For now, we are storing validators in `named_keys` map of the PoS contract
    // in the form: key: "v_{validator_pk}_{validator_stake}", value: doesn't
    // matter.
    let payment_purse = mint_purse(mint_package_hash, U512::zero());

    // Include PoS purses in its named_keys
    let mut named_keys = NamedKeys::new();
    named_keys.insert(POS_PAYMENT_PURSE.to_string(), payment_purse.into());

    let entry_points = {
        let mut entry_points = EntryPoints::new();

        let get_payment_purse = EntryPoint::new(
            METHOD_GET_PAYMENT_PURSE,
            vec![],
            CLType::URef,
            EntryPointAccess::Public,
            EntryPointType::Contract,
        );
        entry_points.add_entry_point(get_payment_purse);

        let set_refund_purse = EntryPoint::new(
            METHOD_SET_REFUND_PURSE,
            vec![Parameter::new(ARG_PURSE, CLType::URef)],
            CLType::Unit,
            EntryPointAccess::Public,
            EntryPointType::Contract,
        );
        entry_points.add_entry_point(set_refund_purse);

        let get_refund_purse = EntryPoint::new(
            METHOD_GET_REFUND_PURSE,
            vec![],
            CLType::Option(Box::new(CLType::URef)),
            EntryPointAccess::Public,
            EntryPointType::Contract,
        );
        entry_points.add_entry_point(get_refund_purse);

        let finalize_payment = EntryPoint::new(
            METHOD_FINALIZE_PAYMENT,
            vec![
                Parameter::new(ARG_AMOUNT, CLType::U512),
                Parameter::new(ARG_ACCOUNT, CLType::ByteArray(32)),
            ],
            CLType::Unit,
            EntryPointAccess::Public,
            EntryPointType::Contract,
        );
        entry_points.add_entry_point(finalize_payment);

        entry_points
    };

    let (contract_package_hash, access_uref) = storage::create_contract_package_at_hash();
    runtime::put_key(HASH_KEY, contract_package_hash.into());
    runtime::put_key(ACCESS_KEY, access_uref.into());

    let (contract_key, _contract_version) =
        storage::add_contract_version(contract_package_hash, entry_points, named_keys);

    let return_value = CLValue::from_t((contract_package_hash, contract_key)).unwrap_or_revert();
    runtime::ret(return_value);
}

fn mint_purse(contract_package_hash: ContractPackageHash, amount: U512) -> URef {
    let args = runtime_args! {
        ARG_AMOUNT => amount,
    };

    let result: Result<URef, mint::Error> = runtime::call_versioned_contract(
        contract_package_hash,
        Some(CONTRACT_INITIAL_VERSION),
        ENTRY_POINT_MINT,
        args,
    );

    result.unwrap_or_revert()
}
