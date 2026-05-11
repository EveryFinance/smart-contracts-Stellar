#![cfg(test)]

use soroban_sdk::{
    testutils::Address as _,
    Address, Env,
};

use crate::{AssetHandler, AssetHandlerClient, PRICE_PRECISION};

// ---------------------------------------------------------------------------
// Mock contracts — each in its own module to avoid symbol conflicts.
// ---------------------------------------------------------------------------

mod mock_oracle {
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    pub struct MockOracle;

    #[contractimpl]
    impl MockOracle {
        pub fn __constructor(env: Env, price: i128) {
            env.storage()
                .instance()
                .set(&soroban_sdk::symbol_short!("price"), &price);
        }
        pub fn get_price(env: Env, _asset: Address) -> i128 {
            env.storage()
                .instance()
                .get(&soroban_sdk::symbol_short!("price"))
                .unwrap_or(0i128)
        }
    }
}

mod mock_oracle_zero {
    use soroban_sdk::{contract, contractimpl, Address, Env};

    #[contract]
    pub struct MockOracleZero;

    #[contractimpl]
    impl MockOracleZero {
        pub fn get_price(_env: Env, _asset: Address) -> i128 {
            0
        }
    }
}

mod mock_oracle_panic {
    use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env};

    #[contract]
    pub struct MockOraclePanic;

    #[contractimpl]
    impl MockOraclePanic {
        pub fn get_price(env: Env, _asset: Address) -> i128 {
            panic_with_error!(&env, crate::AssetHandlerError::PriceNotAvailable)
        }
    }
}

use mock_oracle::MockOracle;
use mock_oracle_zero::MockOracleZero;
use mock_oracle_panic::MockOraclePanic;

// ---------------------------------------------------------------------------
// Test setup helpers
// ---------------------------------------------------------------------------

fn setup() -> (Env, AssetHandlerClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let id = env.register(AssetHandler, (admin.clone(),));
    let client: AssetHandlerClient<'static> =
        unsafe { core::mem::transmute(AssetHandlerClient::new(&env, &id)) };
    (env, client, admin)
}

fn make_oracle(env: &Env, price: i128) -> Address {
    env.register(MockOracle, (price,))
}

fn make_zero_oracle(env: &Env) -> Address {
    env.register(MockOracleZero, ())
}

fn make_panic_oracle(env: &Env) -> Address {
    env.register(MockOraclePanic, ())
}

// ---------------------------------------------------------------------------
// add_asset
// ---------------------------------------------------------------------------

#[test]
fn test_add_asset_registered() {
    let (env, handler, admin) = setup();
    let asset = Address::generate(&env);
    handler.add_asset(&admin, &asset);
    assert!(handler.is_registered(&asset));
    assert_eq!(handler.get_all_assets().len(), 1);
}

#[test]
#[should_panic]
fn test_add_asset_not_admin_panics() {
    let (env, handler, _admin) = setup();
    let rogue = Address::generate(&env);
    let asset = Address::generate(&env);
    handler.add_asset(&rogue, &asset);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_add_asset_duplicate_panics() {
    let (env, handler, admin) = setup();
    let asset = Address::generate(&env);
    handler.add_asset(&admin, &asset);
    handler.add_asset(&admin, &asset);
}

// ---------------------------------------------------------------------------
// remove_asset
// ---------------------------------------------------------------------------

#[test]
fn test_remove_asset() {
    let (env, handler, admin) = setup();
    let asset = Address::generate(&env);
    handler.add_asset(&admin, &asset);
    handler.remove_asset(&admin, &asset);
    assert!(!handler.is_registered(&asset));
    assert_eq!(handler.get_all_assets().len(), 0);
}

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_remove_asset_not_registered_panics() {
    let (env, handler, admin) = setup();
    let asset = Address::generate(&env);
    handler.remove_asset(&admin, &asset);
}

// ---------------------------------------------------------------------------
// get_price — no primary oracle configured
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #7)")]
fn test_get_price_no_primary_oracle_panics() {
    let (env, handler, admin) = setup();
    let asset = Address::generate(&env);
    handler.add_asset(&admin, &asset);
    handler.get_price(&asset);
}

// ---------------------------------------------------------------------------
// get_price — unregistered asset
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #6)")]
fn test_get_price_unregistered_panics() {
    let (env, handler, admin) = setup();
    let primary = make_oracle(&env, PRICE_PRECISION);
    handler.set_primary_oracle(&admin, &primary);
    let asset = Address::generate(&env);
    handler.get_price(&asset);
}

// ---------------------------------------------------------------------------
// get_price — primary succeeds
// ---------------------------------------------------------------------------

#[test]
fn test_get_price_from_primary() {
    let (env, handler, admin) = setup();
    let asset = Address::generate(&env);
    let primary = make_oracle(&env, 65_000 * PRICE_PRECISION);
    handler.add_asset(&admin, &asset);
    handler.set_primary_oracle(&admin, &primary);
    assert_eq!(handler.get_price(&asset), 65_000 * PRICE_PRECISION);
}

// ---------------------------------------------------------------------------
// get_price — primary returns 0, fallback succeeds
// ---------------------------------------------------------------------------

#[test]
fn test_get_price_falls_back_when_primary_returns_zero() {
    let (env, handler, admin) = setup();
    let asset    = Address::generate(&env);
    let primary  = make_zero_oracle(&env);
    let fallback = make_oracle(&env, 3_000 * PRICE_PRECISION);
    handler.add_asset(&admin, &asset);
    handler.set_primary_oracle(&admin, &primary);
    handler.set_fallback_oracle(&admin, &fallback);
    assert_eq!(handler.get_price(&asset), 3_000 * PRICE_PRECISION);
}

// ---------------------------------------------------------------------------
// get_price — primary reverts, fallback succeeds
// ---------------------------------------------------------------------------

#[test]
fn test_get_price_falls_back_when_primary_reverts() {
    let (env, handler, admin) = setup();
    let asset    = Address::generate(&env);
    let primary  = make_panic_oracle(&env);
    let fallback = make_oracle(&env, PRICE_PRECISION);
    handler.add_asset(&admin, &asset);
    handler.set_primary_oracle(&admin, &primary);
    handler.set_fallback_oracle(&admin, &fallback);
    assert_eq!(handler.get_price(&asset), PRICE_PRECISION);
}

// ---------------------------------------------------------------------------
// get_price — both oracles fail → PriceNotAvailable
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_get_price_both_oracles_fail_panics() {
    let (env, handler, admin) = setup();
    let asset    = Address::generate(&env);
    let primary  = make_zero_oracle(&env);
    let fallback = make_zero_oracle(&env);
    handler.add_asset(&admin, &asset);
    handler.set_primary_oracle(&admin, &primary);
    handler.set_fallback_oracle(&admin, &fallback);
    handler.get_price(&asset);
}

// ---------------------------------------------------------------------------
// get_price — primary returns 0, no fallback → PriceNotAvailable
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "Error(Contract, #8)")]
fn test_get_price_primary_zero_no_fallback_panics() {
    let (env, handler, admin) = setup();
    let asset   = Address::generate(&env);
    let primary = make_zero_oracle(&env);
    handler.add_asset(&admin, &asset);
    handler.set_primary_oracle(&admin, &primary);
    handler.get_price(&asset);
}

// ---------------------------------------------------------------------------
// Multiple assets — one primary oracle covers all
// ---------------------------------------------------------------------------

#[test]
fn test_multiple_assets_single_primary_oracle() {
    let (env, handler, admin) = setup();
    let btc  = Address::generate(&env);
    let usdc = Address::generate(&env);
    let primary = make_oracle(&env, 65_000 * PRICE_PRECISION);
    handler.set_primary_oracle(&admin, &primary);
    handler.add_asset(&admin, &btc);
    handler.add_asset(&admin, &usdc);
    assert_eq!(handler.get_all_assets().len(), 2);
    assert_eq!(handler.get_price(&btc),  65_000 * PRICE_PRECISION);
    assert_eq!(handler.get_price(&usdc), 65_000 * PRICE_PRECISION);
}

// ---------------------------------------------------------------------------
// Oracle management
// ---------------------------------------------------------------------------

#[test]
fn test_set_get_primary_oracle() {
    let (env, handler, admin) = setup();
    let oracle = make_oracle(&env, PRICE_PRECISION);
    assert!(handler.get_primary_oracle().is_none());
    handler.set_primary_oracle(&admin, &oracle);
    assert_eq!(handler.get_primary_oracle(), Some(oracle));
}

#[test]
fn test_set_get_fallback_oracle() {
    let (env, handler, admin) = setup();
    let oracle = make_oracle(&env, PRICE_PRECISION);
    assert!(handler.get_fallback_oracle().is_none());
    handler.set_fallback_oracle(&admin, &oracle);
    assert_eq!(handler.get_fallback_oracle(), Some(oracle));
}

#[test]
#[should_panic]
fn test_set_primary_oracle_not_admin_panics() {
    let (env, handler, _admin) = setup();
    let rogue  = Address::generate(&env);
    let oracle = make_oracle(&env, PRICE_PRECISION);
    handler.set_primary_oracle(&rogue, &oracle);
}

// ---------------------------------------------------------------------------
// Admin transfer
// ---------------------------------------------------------------------------

#[test]
fn test_two_step_admin_transfer() {
    let (env, handler, admin) = setup();
    let new_admin = Address::generate(&env);
    handler.set_pending_admin(&admin, &new_admin);
    handler.accept_admin(&new_admin);
    assert_eq!(handler.get_admin(), new_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_accept_admin_no_pending_panics() {
    let (env, handler, _admin) = setup();
    let stranger = Address::generate(&env);
    handler.accept_admin(&stranger);
}
