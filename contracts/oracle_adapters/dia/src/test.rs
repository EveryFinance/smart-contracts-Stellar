#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

use crate::{DiaAdapter, DiaAdapterClient, PRICE_PRECISION};

// ---------------------------------------------------------------------------
// Mock DIA contract
// ---------------------------------------------------------------------------

mod mock_dia {
    use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Env, String};

    #[contracttype]
    #[derive(Clone)]
    pub struct OracleValue {
        pub price: i128,
        pub timestamp: u64,
    }

    #[contract]
    pub struct MockDia;

    #[contractimpl]
    impl MockDia {
        pub fn __constructor(env: Env, price: i128) {
            env.storage()
                .instance()
                .set(&symbol_short!("price"), &price);
            env.storage()
                .instance()
                .set(&symbol_short!("ts"), &env.ledger().timestamp());
        }

        pub fn read_oracle_value(env: Env, _key: String) -> OracleValue {
            let price: i128 = env
                .storage()
                .instance()
                .get(&symbol_short!("price"))
                .unwrap_or(0);
            let timestamp: u64 = env
                .storage()
                .instance()
                .get(&symbol_short!("ts"))
                .unwrap_or(0);
            OracleValue { price, timestamp }
        }
    }
}

mod mock_dia_panic {
    use soroban_sdk::{contract, contractimpl, panic_with_error, Env, String};

    #[contract]
    pub struct MockDiaPanic;

    #[contractimpl]
    impl MockDiaPanic {
        pub fn read_oracle_value(env: Env, _key: String) -> i128 {
            panic_with_error!(&env, crate::DiaAdapterError::NotInitialized)
        }
    }
}

use mock_dia::MockDia;
use mock_dia_panic::MockDiaPanic;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup(dia_price: i128) -> (Env, DiaAdapterClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);

    let dia_id = env.register(MockDia, (dia_price,));
    let adapter_id = env.register(DiaAdapter, (admin.clone(), dia_id));
    let client: DiaAdapterClient<'static> =
        unsafe { core::mem::transmute(DiaAdapterClient::new(&env, &adapter_id)) };

    (env, client, admin)
}

fn xlm_key(env: &Env) -> String {
    String::from_str(env, "XLM/USD")
}

fn btc_key(env: &Env) -> String {
    String::from_str(env, "BTC/USD")
}

// ---------------------------------------------------------------------------
// get_price — no mapping
// ---------------------------------------------------------------------------

#[test]
fn test_get_price_no_key_returns_zero() {
    let (env, client, _) = setup(100_000_000);
    let asset = Address::generate(&env);
    assert_eq!(client.get_price(&asset), 0);
}

// ---------------------------------------------------------------------------
// get_price — with mapping, normal prices
// ---------------------------------------------------------------------------

#[test]
fn test_get_price_1_usd() {
    // DIA price for $1.00 with 8 decimals = 100_000_000
    // Expected: PRICE_PRECISION = 10_000_000
    let (env, client, admin) = setup(100_000_000);
    let asset = Address::generate(&env);
    client.set_asset_key(&admin, &asset, &xlm_key(&env));
    assert_eq!(client.get_price(&asset), PRICE_PRECISION);
}

#[test]
fn test_get_price_65000_usd() {
    // BTC at $65,000 with 8 decimals = 6_500_000_000_000
    // Expected: 65_000 * PRICE_PRECISION = 650_000_000_000
    let (env, client, admin) = setup(6_500_000_000_000i128);
    let asset = Address::generate(&env);
    client.set_asset_key(&admin, &asset, &btc_key(&env));
    assert_eq!(client.get_price(&asset), 65_000 * PRICE_PRECISION);
}

#[test]
fn test_get_price_fractional() {
    // $0.10 with 8 decimals = 10_000_000
    // Expected: 0.1 * PRICE_PRECISION = 1_000_000
    let (env, client, admin) = setup(10_000_000);
    let asset = Address::generate(&env);
    client.set_asset_key(&admin, &asset, &xlm_key(&env));
    assert_eq!(client.get_price(&asset), 1_000_000);
}

#[test]
fn test_get_price_stale_dia_returns_zero() {
    let (env, client, admin) = setup(100_000_000);
    let asset = Address::generate(&env);
    client.set_asset_key(&admin, &asset, &xlm_key(&env));
    assert_eq!(client.get_price(&asset), PRICE_PRECISION);

    env.ledger().with_mut(|li| li.timestamp = 3_601);
    assert_eq!(client.get_price(&asset), 0);
}

#[test]
fn test_set_max_age_secs_extends_freshness_window() {
    let (env, client, admin) = setup(100_000_000);
    let asset = Address::generate(&env);
    client.set_asset_key(&admin, &asset, &xlm_key(&env));
    client.set_max_age_secs(&admin, &7_200u64);

    env.ledger().with_mut(|li| li.timestamp = 3_601);
    assert_eq!(client.get_price(&asset), PRICE_PRECISION);
    assert_eq!(client.get_max_age_secs(), 7_200);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_set_max_age_zero_panics() {
    let (_, client, admin) = setup(100_000_000);
    client.set_max_age_secs(&admin, &0u64);
}

// ---------------------------------------------------------------------------
// get_price — DIA reverts (graceful)
// ---------------------------------------------------------------------------

#[test]
fn test_get_price_returns_zero_when_dia_reverts() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let dia_id = env.register(MockDiaPanic, ());
    let adapter_id = env.register(DiaAdapter, (admin.clone(), dia_id));
    let client: DiaAdapterClient<'static> =
        unsafe { core::mem::transmute(DiaAdapterClient::new(&env, &adapter_id)) };

    let asset = Address::generate(&env);
    client.set_asset_key(&admin, &asset, &xlm_key(&env));
    // DIA panics → adapter catches it and returns 0.
    assert_eq!(client.get_price(&asset), 0);
}

// ---------------------------------------------------------------------------
// Key management
// ---------------------------------------------------------------------------

#[test]
fn test_set_get_asset_key() {
    let (env, client, admin) = setup(100_000_000);
    let asset = Address::generate(&env);
    assert!(client.get_asset_key(&asset).is_none());
    client.set_asset_key(&admin, &asset, &xlm_key(&env));
    assert_eq!(client.get_asset_key(&asset), Some(xlm_key(&env)));
}

#[test]
fn test_remove_asset_key() {
    let (env, client, admin) = setup(100_000_000);
    let asset = Address::generate(&env);
    client.set_asset_key(&admin, &asset, &xlm_key(&env));
    client.remove_asset_key(&admin, &asset);
    assert!(client.get_asset_key(&asset).is_none());
    // After removal, get_price returns 0.
    assert_eq!(client.get_price(&asset), 0);
}

#[test]
#[should_panic]
fn test_set_asset_key_not_admin_panics() {
    let (env, client, _) = setup(100_000_000);
    let rogue = Address::generate(&env);
    let asset = Address::generate(&env);
    client.set_asset_key(&rogue, &asset, &xlm_key(&env));
}

#[test]
fn test_multiple_assets_independent_keys() {
    let (env, client, admin) = setup(100_000_000);
    let xlm = Address::generate(&env);
    let btc = Address::generate(&env);
    client.set_asset_key(&admin, &xlm, &xlm_key(&env));
    client.set_asset_key(&admin, &btc, &btc_key(&env));
    assert_eq!(client.get_asset_key(&xlm), Some(xlm_key(&env)));
    assert_eq!(client.get_asset_key(&btc), Some(btc_key(&env)));
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

#[test]
fn test_get_admin() {
    let (_, client, admin) = setup(100_000_000);
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_two_step_admin_transfer() {
    let (env, client, admin) = setup(100_000_000);
    let new_admin = Address::generate(&env);
    client.set_pending_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_accept_admin_no_pending_panics() {
    let (env, client, _) = setup(100_000_000);
    let stranger = Address::generate(&env);
    client.accept_admin(&stranger);
}

#[test]
fn test_set_dia_contract_updates_address() {
    let (env, client, admin) = setup(100_000_000);
    let new_dia = env.register(MockDia, (200_000_000i128,));
    client.set_dia_contract(&admin, &new_dia);
    assert_eq!(client.get_dia_contract(), new_dia);
}
