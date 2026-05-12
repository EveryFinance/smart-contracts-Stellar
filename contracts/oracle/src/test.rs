#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    vec, Address, Env,
};

use crate::{OracleContract, OracleContractClient, DEFAULT_MAX_AGE_LEDGERS, PRICE_PRECISION};

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

fn setup(env: &Env) -> (OracleContractClient<'_>, Address) {
    let admin = Address::generate(env);
    let id = env.register(OracleContract, (admin.clone(),));
    let client = OracleContractClient::new(env, &id);
    (client, admin)
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

#[test]
fn test_initialize() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);
    assert_eq!(client.get_admin(), admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #2)")]
fn test_constructor_rejects_reinitialization() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin) = setup(&env);

    env.as_contract(&client.address, || {
        OracleContract::__constructor(env.clone(), admin);
    });
}

// ---------------------------------------------------------------------------
// set_price / get_price
// ---------------------------------------------------------------------------

#[test]
fn test_set_and_get_price() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let usdc = Address::generate(&env);
    client.set_price(&usdc, &PRICE_PRECISION); // 1 USDC = 1.0

    assert_eq!(client.get_price(&usdc), PRICE_PRECISION);
}

#[test]
fn test_update_price_overwrites() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let xlm = Address::generate(&env);
    client.set_price(&xlm, &5_000_000i128); // 0.5
    client.set_price(&xlm, &12_000_000i128); // 1.2 (update)

    assert_eq!(client.get_price(&xlm), 12_000_000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_price_not_found_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let unknown = Address::generate(&env);
    client.get_price(&unknown);
}

#[test]
#[should_panic]
fn test_set_zero_price_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    let asset = Address::generate(&env);
    client.set_price(&asset, &0i128);
}

#[test]
#[should_panic]
fn test_set_negative_price_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    let asset = Address::generate(&env);
    client.set_price(&asset, &-1i128);
}

// ---------------------------------------------------------------------------
// get_prices (batch)
// ---------------------------------------------------------------------------

#[test]
fn test_get_prices_multiple_assets() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let usdc = Address::generate(&env);
    let xlm = Address::generate(&env);
    let btc = Address::generate(&env);

    client.set_price(&usdc, &10_000_000i128);
    client.set_price(&xlm, &5_000_000i128);
    client.set_price(&btc, &650_000_000_000i128);

    let prices = client.get_prices(&vec![&env, usdc.clone(), xlm.clone(), btc.clone()]);

    assert_eq!(prices.get(usdc).unwrap(), 10_000_000i128);
    assert_eq!(prices.get(xlm).unwrap(), 5_000_000i128);
    assert_eq!(prices.get(btc).unwrap(), 650_000_000_000i128);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_get_prices_missing_one_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    let usdc = Address::generate(&env);
    let missing = Address::generate(&env);

    client.set_price(&usdc, &PRICE_PRECISION);
    // `missing` has no price → must panic
    client.get_prices(&vec![&env, usdc, missing]);
}

// ---------------------------------------------------------------------------
// set_admin
// ---------------------------------------------------------------------------

#[test]
fn test_set_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _old) = setup(&env);
    let new_admin = Address::generate(&env);

    client.set_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);

    // New admin can set prices.
    let asset = Address::generate(&env);
    client.set_price(&asset, &PRICE_PRECISION);
    assert_eq!(client.get_price(&asset), PRICE_PRECISION);
}

// ---------------------------------------------------------------------------
// Unauthorized access — real auth enforcement (no mock_all_auths)
// ---------------------------------------------------------------------------

/// `set_price` requires the stored admin's auth — calling without it panics.
#[test]
#[should_panic]
fn test_set_price_unauthorized_no_mock() {
    let env = Env::default();
    // No mock_all_auths — Soroban enforces auth.
    let admin = Address::generate(&env);
    // Constructor runs atomically; no separate initialize call needed.
    let id = env.register(OracleContract, (admin.clone(),));
    let client = OracleContractClient::new(&env, &id);
    let asset = Address::generate(&env);
    // set_price requires admin.require_auth() — panics without signatures.
    client.set_price(&asset, &PRICE_PRECISION);
}

/// `set_admin` requires the stored admin's auth — calling without it panics.
#[test]
#[should_panic]
fn test_set_admin_unauthorized_no_mock() {
    let env = Env::default();
    // No mock_all_auths — Soroban enforces auth.
    let admin = Address::generate(&env);
    let id = env.register(OracleContract, (admin.clone(),));
    let client = OracleContractClient::new(&env, &id);
    let new_admin = Address::generate(&env);
    // set_admin requires admin.require_auth() — panics without signatures.
    client.set_admin(&new_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_stale_price_panics() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
    });

    let (client, _) = setup(&env);
    let asset = Address::generate(&env);
    client.set_price(&asset, &PRICE_PRECISION);
    client.set_max_age_ledgers(&5u32);

    env.ledger().with_mut(|li| {
        li.sequence_number = 106;
    });
    client.get_price(&asset);
}

#[test]
#[should_panic]
fn test_max_age_zero_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    // max_age_ledgers == 0 would permanently disable staleness checks; must be rejected.
    client.set_max_age_ledgers(&0u32);
}

#[test]
fn test_get_max_age_ledgers_default_and_updated() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);

    assert_eq!(client.get_max_age_ledgers(), DEFAULT_MAX_AGE_LEDGERS);

    client.set_max_age_ledgers(&17u32);
    assert_eq!(client.get_max_age_ledgers(), 17u32);
}

// ---------------------------------------------------------------------------
// Price precision and large values
// ---------------------------------------------------------------------------

#[test]
fn test_price_precision_constant() {
    assert_eq!(PRICE_PRECISION, 10_000_000i128);
}

#[test]
fn test_very_large_price() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    let btc = Address::generate(&env);
    // BTC at $65,000 with 7-decimal precision
    let price = 650_000_000_000i128; // 65000 * 10^7
    client.set_price(&btc, &price);
    assert_eq!(client.get_price(&btc), price);
}

#[test]
fn test_get_prices_empty_returns_empty_map() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    let prices = client.get_prices(&vec![&env]);
    assert_eq!(prices.len(), 0);
}

#[test]
fn test_price_overwrite_reflects_immediately() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _) = setup(&env);
    let asset = Address::generate(&env);

    for i in 1i128..=5 {
        client.set_price(&asset, &(i * PRICE_PRECISION));
        assert_eq!(client.get_price(&asset), i * PRICE_PRECISION);
    }
}
