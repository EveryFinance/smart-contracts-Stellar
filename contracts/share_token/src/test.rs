//! Full test suite for the ShareToken contract.
//!
//! Coverage:
//! * Initialization (happy path + double-init guard)
//! * Mint (admin-only, amount validation)
//! * Transfer (happy path, insufficient balance, zero/negative)
//! * Approve + transfer_from (happy path, insufficient allowance)
//! * Burn and burn_from
//! * set_admin
//! * total_supply accounting across multiple operations

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

use crate::ShareTokenContract;
use crate::ShareTokenContractClient;

// ---------------------------------------------------------------------------
// Helper: deploy + initialize a fresh token contract
// ---------------------------------------------------------------------------

fn setup(env: &Env) -> (ShareTokenContractClient<'_>, Address) {
    let contract_id = env.register(ShareTokenContract, ());
    let client = ShareTokenContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.initialize(
        &admin,
        &String::from_str(env, "Vault Share Token"),
        &String::from_str(env, "VST"),
        &7u32,
    );
    (client, admin)
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);

    assert_eq!(client.name(), String::from_str(&env, "Vault Share Token"));
    assert_eq!(client.symbol(), String::from_str(&env, "VST"));
    assert_eq!(client.decimals(), 7u32);
    assert_eq!(client.total_supply(), 0i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_double_initialize_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    // Second init must fail.
    client.initialize(
        &admin,
        &String::from_str(&env, "Other"),
        &String::from_str(&env, "OTH"),
        &7u32,
    );
}

// ---------------------------------------------------------------------------
// Mint
// ---------------------------------------------------------------------------

#[test]
fn test_mint_increases_balance_and_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);

    client.mint(&user, &1_000_000_000i128);

    assert_eq!(client.balance(&user), 1_000_000_000i128);
    assert_eq!(client.total_supply(), 1_000_000_000i128);
}

#[test]
fn test_mint_multiple_users_supply_accumulates() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let user_a = Address::generate(&env);
    let user_b = Address::generate(&env);

    client.mint(&user_a, &500_000_000i128);
    client.mint(&user_b, &300_000_000i128);

    assert_eq!(client.total_supply(), 800_000_000i128);
    assert_eq!(client.balance(&user_a), 500_000_000i128);
    assert_eq!(client.balance(&user_b), 300_000_000i128);
}

#[test]
#[should_panic]
fn test_mint_zero_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);
    client.mint(&user, &0i128);
}

#[test]
#[should_panic]
fn test_mint_negative_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);
    client.mint(&user, &-1i128);
}

// ---------------------------------------------------------------------------
// Transfer
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint(&alice, &1_000_0000000i128);
    client.transfer(&alice, &bob, &400_0000000i128);

    assert_eq!(client.balance(&alice), 600_0000000i128);
    assert_eq!(client.balance(&bob), 400_0000000i128);
    // Supply unchanged by transfer.
    assert_eq!(client.total_supply(), 1_000_0000000i128);
}

#[test]
#[should_panic]
fn test_transfer_insufficient_balance_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint(&alice, &100i128);
    client.transfer(&alice, &bob, &200i128); // more than balance
}

#[test]
#[should_panic]
fn test_transfer_zero_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint(&alice, &1_000i128);
    client.transfer(&alice, &bob, &0i128);
}

#[test]
#[should_panic]
fn test_transfer_negative_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint(&alice, &1_000i128);
    client.transfer(&alice, &bob, &-1i128);
}

// ---------------------------------------------------------------------------
// Approve + transfer_from
// ---------------------------------------------------------------------------

#[test]
fn test_approve_and_transfer_from() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let spender = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint(&alice, &1_000_0000000i128);
    client.approve(&alice, &spender, &500_0000000i128, &999u32);

    assert_eq!(client.allowance(&alice, &spender), 500_0000000i128);

    client.transfer_from(&spender, &alice, &bob, &200_0000000i128);

    assert_eq!(client.balance(&alice), 800_0000000i128);
    assert_eq!(client.balance(&bob), 200_0000000i128);
    // Allowance reduced.
    assert_eq!(client.allowance(&alice, &spender), 300_0000000i128);
}

#[test]
#[should_panic]
fn test_transfer_from_insufficient_allowance_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let spender = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint(&alice, &1_000i128);
    client.approve(&alice, &spender, &50i128, &999u32);
    client.transfer_from(&spender, &alice, &bob, &100i128); // over allowance
}

#[test]
fn test_approve_zero_revokes() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let spender = Address::generate(&env);

    client.mint(&alice, &1_000i128);
    client.approve(&alice, &spender, &500i128, &999u32);
    assert_eq!(client.allowance(&alice, &spender), 500i128);
    client.approve(&alice, &spender, &0i128, &999u32);
    assert_eq!(client.allowance(&alice, &spender), 0i128);
}

// ---------------------------------------------------------------------------
// Burn
// ---------------------------------------------------------------------------

#[test]
fn test_burn_reduces_balance_and_supply() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);

    client.mint(&alice, &1_000_0000000i128);
    client.burn(&alice, &300_0000000i128);

    assert_eq!(client.balance(&alice), 700_0000000i128);
    assert_eq!(client.total_supply(), 700_0000000i128);
}

#[test]
#[should_panic]
fn test_burn_insufficient_balance_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);

    client.mint(&alice, &100i128);
    client.burn(&alice, &200i128);
}

#[test]
fn test_burn_from_with_allowance() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let burner = Address::generate(&env);

    client.mint(&alice, &1_000i128);
    client.approve(&alice, &burner, &500i128, &999u32);
    client.burn_from(&burner, &alice, &400i128);

    assert_eq!(client.balance(&alice), 600i128);
    assert_eq!(client.total_supply(), 600i128);
    assert_eq!(client.allowance(&alice, &burner), 100i128);
}

#[test]
#[should_panic]
fn test_burn_from_insufficient_allowance_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let burner = Address::generate(&env);

    client.mint(&alice, &1_000i128);
    client.approve(&alice, &burner, &100i128, &999u32);
    client.burn_from(&burner, &alice, &200i128);
}

// ---------------------------------------------------------------------------
// set_admin
// ---------------------------------------------------------------------------

#[test]
fn test_set_admin_transfers_mint_rights() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _old_admin) = setup(&env);
    let new_admin = Address::generate(&env);
    let user = Address::generate(&env);

    client.set_admin(&new_admin);
    // New admin can mint.
    client.mint(&user, &1_000i128);
    assert_eq!(client.balance(&user), 1_000i128);
}

// ---------------------------------------------------------------------------
// Total supply consistency
// ---------------------------------------------------------------------------

#[test]
fn test_total_supply_consistency_after_mixed_ops() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    client.mint(&alice, &1_000i128);
    client.mint(&bob, &500i128);
    client.transfer(&alice, &bob, &100i128);
    client.burn(&bob, &200i128);

    // supply = 1000 + 500 - 200 = 1300
    assert_eq!(client.total_supply(), 1_300i128);
    // alice: 1000 - 100 = 900
    assert_eq!(client.balance(&alice), 900i128);
    // bob:   500 + 100 - 200 = 400
    assert_eq!(client.balance(&bob), 400i128);
}

#[test]
fn test_balance_unknown_address_returns_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let unknown = Address::generate(&env);
    assert_eq!(client.balance(&unknown), 0i128);
}

#[test]
fn test_allowance_unknown_pair_returns_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    assert_eq!(client.allowance(&a, &b), 0i128);
}

// ---------------------------------------------------------------------------
// NotInitialized — calling any metadata function before initialize panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_name_not_initialized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(ShareTokenContract, ());
    let client = ShareTokenContractClient::new(&env, &id);
    client.name(); // no initialize() called
}

#[test]
#[should_panic]
fn test_symbol_not_initialized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(ShareTokenContract, ());
    let client = ShareTokenContractClient::new(&env, &id);
    client.symbol();
}

#[test]
#[should_panic]
fn test_decimals_not_initialized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(ShareTokenContract, ());
    let client = ShareTokenContractClient::new(&env, &id);
    client.decimals();
}

#[test]
#[should_panic]
fn test_mint_not_initialized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(ShareTokenContract, ());
    let client = ShareTokenContractClient::new(&env, &id);
    let user = Address::generate(&env);
    client.mint(&user, &100i128);
}

#[test]
#[should_panic]
fn test_set_admin_not_initialized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let id = env.register(ShareTokenContract, ());
    let client = ShareTokenContractClient::new(&env, &id);
    let new_admin = Address::generate(&env);
    client.set_admin(&new_admin);
}

// ---------------------------------------------------------------------------
// Overflow — arithmetic overflow with i128::MAX values
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_mint_overflow_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let user = Address::generate(&env);

    // Mint i128::MAX to user — fills their balance.
    client.mint(&user, &i128::MAX);
    // Minting any more would overflow the per-user balance.
    client.mint(&user, &1i128);
}

#[test]
#[should_panic]
fn test_transfer_to_overflow_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    // Give bob i128::MAX, give alice 1.
    client.mint(&bob, &i128::MAX);
    client.mint(&alice, &1i128);
    // Transferring 1 from alice to bob overflows bob's balance.
    client.transfer(&alice, &bob, &1i128);
}

// ---------------------------------------------------------------------------
// Burn zero / negative
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_burn_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    client.mint(&alice, &1_000i128);
    client.burn(&alice, &0i128);
}

#[test]
#[should_panic]
fn test_burn_negative_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    client.mint(&alice, &1_000i128);
    client.burn(&alice, &-1i128);
}

// ---------------------------------------------------------------------------
// transfer_from — zero / negative amount
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_transfer_from_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let spender = Address::generate(&env);
    let bob = Address::generate(&env);
    client.mint(&alice, &1_000i128);
    client.approve(&alice, &spender, &500i128, &999u32);
    client.transfer_from(&spender, &alice, &bob, &0i128);
}

// ---------------------------------------------------------------------------
// approve — negative amount panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_approve_negative_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let spender = Address::generate(&env);
    client.mint(&alice, &1_000i128);
    client.approve(&alice, &spender, &-1i128, &999u32);
}

// ---------------------------------------------------------------------------
// Transfer from self to self — balance unchanged
// ---------------------------------------------------------------------------

#[test]
fn test_transfer_to_self() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    client.mint(&alice, &1_000i128);
    client.transfer(&alice, &alice, &400i128);
    assert_eq!(client.balance(&alice), 1_000i128);
    assert_eq!(client.total_supply(), 1_000i128);
}

// ---------------------------------------------------------------------------
// burn_from — zero amount panics
// ---------------------------------------------------------------------------

#[test]
#[should_panic]
fn test_burn_from_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin) = setup(&env);
    let alice = Address::generate(&env);
    let burner = Address::generate(&env);
    client.mint(&alice, &1_000i128);
    client.approve(&alice, &burner, &500i128, &999u32);
    client.burn_from(&burner, &alice, &0i128);
}

// ---------------------------------------------------------------------------
// Allowance expiration enforcement
// ---------------------------------------------------------------------------

#[test]
fn test_allowance_returns_zero_after_expiration() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });

    let (client, _admin) = setup(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);

    client.mint(&owner, &1_000i128);
    client.approve(&owner, &spender, &500i128, &101u32);
    assert_eq!(client.allowance(&owner, &spender), 500i128);

    env.ledger().with_mut(|li| {
        li.sequence_number = 102;
    });
    assert_eq!(client.allowance(&owner, &spender), 0i128);
}

#[test]
#[should_panic]
fn test_transfer_from_expired_allowance_panics() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 200;
    });

    let (client, _admin) = setup(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);
    let recipient = Address::generate(&env);

    client.mint(&owner, &1_000i128);
    client.approve(&owner, &spender, &500i128, &200u32);

    env.ledger().with_mut(|li| {
        li.sequence_number = 201;
    });

    client.transfer_from(&spender, &owner, &recipient, &1i128);
}

#[test]
#[should_panic]
fn test_burn_from_expired_allowance_panics() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 300;
    });

    let (client, _admin) = setup(&env);
    let owner = Address::generate(&env);
    let spender = Address::generate(&env);

    client.mint(&owner, &1_000i128);
    client.approve(&owner, &spender, &500i128, &300u32);

    env.ledger().with_mut(|li| {
        li.sequence_number = 301;
    });

    client.burn_from(&spender, &owner, &1i128);
}
