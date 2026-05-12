#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env,
};

use crate::{ReflectorAdapter, ReflectorAdapterClient, PRICE_PRECISION};

// ---------------------------------------------------------------------------
// Mock Reflector contract
// ---------------------------------------------------------------------------

mod mock_reflector {
    use soroban_sdk::{contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol};

    #[contracttype]
    #[derive(Clone)]
    pub enum ReflectorAsset {
        Stellar(Address),
        Other(Symbol),
    }

    #[contracttype]
    #[derive(Clone)]
    pub struct PriceData {
        pub price: i128,
        pub timestamp: u64,
    }

    #[contract]
    pub struct MockReflector;

    #[contractimpl]
    impl MockReflector {
        pub fn __constructor(env: Env, price: i128, decimals: u32) {
            env.storage()
                .instance()
                .set(&symbol_short!("price"), &price);
            env.storage()
                .instance()
                .set(&symbol_short!("dec"), &decimals);
            env.storage()
                .instance()
                .set(&symbol_short!("ts"), &env.ledger().timestamp());
        }

        pub fn decimals(env: Env) -> u32 {
            env.storage()
                .instance()
                .get(&symbol_short!("dec"))
                .unwrap_or(8u32)
        }

        pub fn lastprice(env: Env, _asset: ReflectorAsset) -> Option<PriceData> {
            let price: i128 = env
                .storage()
                .instance()
                .get(&symbol_short!("price"))
                .unwrap_or(0);
            if price == 0 {
                None
            } else {
                let timestamp: u64 = env
                    .storage()
                    .instance()
                    .get(&symbol_short!("ts"))
                    .unwrap_or(0);
                Some(PriceData { price, timestamp })
            }
        }
    }
}

mod mock_reflector_none {
    use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

    #[contracttype]
    #[derive(Clone)]
    pub enum ReflectorAsset {
        Stellar(Address),
        Other(Symbol),
    }

    #[contracttype]
    #[derive(Clone)]
    pub struct PriceData {
        pub price: i128,
        pub timestamp: u64,
    }

    #[contract]
    pub struct MockReflectorNone;

    #[contractimpl]
    impl MockReflectorNone {
        pub fn decimals(_env: Env) -> u32 {
            8
        }
        pub fn lastprice(_env: Env, _asset: ReflectorAsset) -> Option<PriceData> {
            None
        }
    }
}

use mock_reflector::MockReflector;
use mock_reflector_none::MockReflectorNone;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn setup(reflector_price: i128, decimals: u32) -> (Env, ReflectorAdapterClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);

    let reflector_id = env.register(MockReflector, (reflector_price, decimals));

    let adapter_id = env.register(ReflectorAdapter, (admin.clone(), reflector_id));
    let client: ReflectorAdapterClient<'static> =
        unsafe { core::mem::transmute(ReflectorAdapterClient::new(&env, &adapter_id)) };

    (env, client, admin)
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

#[test]
fn test_constructor_stores_decimals() {
    let (_, client, _) = setup(100_000_000, 8);
    assert_eq!(client.get_decimals(), 8);
}

// Note: double-init protection is enforced by `is_initialized()` in `__constructor`.
// The constructor is called once at deploy time via `env.register()`; there is no
// way to call it a second time through the public client API, so no explicit test
// for the AlreadyInitialized path is needed here.

// ---------------------------------------------------------------------------
// get_price — normal cases
// ---------------------------------------------------------------------------

#[test]
fn test_get_price_8_decimals() {
    // Reflector price: 65_000 * 10^8 = 6_500_000_000_000 (BTC at $65,000 with 8 decimals)
    // Expected output: 65_000 * PRICE_PRECISION = 65_000 * 10^7 = 650_000_000_000
    let reflector_price = 65_000i128 * 100_000_000; // 6_500_000_000_000
    let (env, client, _) = setup(reflector_price, 8);
    let asset = Address::generate(&env);
    let price = client.get_price(&asset);
    assert_eq!(price, 65_000 * PRICE_PRECISION);
}

#[test]
fn test_get_price_exact_1_usd_8_decimals() {
    // 1.0 USD with 8 decimals → 100_000_000
    // Expected: PRICE_PRECISION = 10_000_000
    let (env, client, _) = setup(100_000_000, 8);
    let asset = Address::generate(&env);
    assert_eq!(client.get_price(&asset), PRICE_PRECISION);
}

#[test]
fn test_get_price_7_decimals_no_conversion_needed() {
    // If Reflector used 7 decimals, price should pass through unchanged.
    let price_7dec = 42 * PRICE_PRECISION; // 420_000_000
    let (env, client, _) = setup(price_7dec, 7);
    let asset = Address::generate(&env);
    assert_eq!(client.get_price(&asset), 42 * PRICE_PRECISION);
}

#[test]
fn test_get_price_unsupported_decimals_returns_zero() {
    let (env, client, _) = setup(100_000_000, 100);
    let asset = Address::generate(&env);
    assert_eq!(client.get_price(&asset), 0);
}

#[test]
fn test_get_price_stale_reflector_returns_zero() {
    let (env, client, _) = setup(100_000_000, 8);
    let asset = Address::generate(&env);
    assert_eq!(client.get_price(&asset), PRICE_PRECISION);

    env.ledger().with_mut(|li| li.timestamp = 3_601);
    assert_eq!(client.get_price(&asset), 0);
}

#[test]
fn test_set_max_age_secs_extends_freshness_window() {
    let (env, client, admin) = setup(100_000_000, 8);
    let asset = Address::generate(&env);
    client.set_max_age_secs(&admin, &7_200u64);

    env.ledger().with_mut(|li| li.timestamp = 3_601);
    assert_eq!(client.get_price(&asset), PRICE_PRECISION);
    assert_eq!(client.get_max_age_secs(), 7_200);
}

#[test]
#[should_panic(expected = "Error(Contract, #5)")]
fn test_set_max_age_zero_panics() {
    let (_, client, admin) = setup(100_000_000, 8);
    client.set_max_age_secs(&admin, &0u64);
}

// ---------------------------------------------------------------------------
// get_price — no data (None)
// ---------------------------------------------------------------------------

#[test]
fn test_get_price_returns_zero_when_reflector_returns_none() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    let admin = Address::generate(&env);
    let reflector_id = env.register(MockReflectorNone, ());
    let adapter_id = env.register(ReflectorAdapter, (admin.clone(), reflector_id));
    let client: ReflectorAdapterClient<'static> =
        unsafe { core::mem::transmute(ReflectorAdapterClient::new(&env, &adapter_id)) };

    let asset = Address::generate(&env);
    assert_eq!(client.get_price(&asset), 0);
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

#[test]
fn test_get_admin() {
    let (_, client, admin) = setup(100_000_000, 8);
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_two_step_admin_transfer() {
    let (env, client, admin) = setup(100_000_000, 8);
    let new_admin = Address::generate(&env);
    client.set_pending_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
#[should_panic(expected = "Error(Contract, #4)")]
fn test_accept_admin_no_pending_panics() {
    let (env, client, _) = setup(100_000_000, 8);
    let stranger = Address::generate(&env);
    client.accept_admin(&stranger);
}

#[test]
#[should_panic]
fn test_set_reflector_not_admin_panics() {
    let (env, client, _) = setup(100_000_000, 8);
    let rogue = Address::generate(&env);
    let new_reflector = env.register(MockReflector, (200_000_000i128, 8u32));
    client.set_reflector(&rogue, &new_reflector);
}

#[test]
fn test_set_reflector_updates_address_and_decimals() {
    let (env, client, admin) = setup(100_000_000, 8);
    // New reflector with 6 decimals.
    let new_reflector = env.register(MockReflector, (1_000_000i128, 6u32));
    client.set_reflector(&admin, &new_reflector);
    assert_eq!(client.get_decimals(), 6);
    assert_eq!(client.get_reflector(), new_reflector);
    // get_price: 1_000_000 (6 dec) → 1.0 → PRICE_PRECISION
    let asset = Address::generate(&env);
    assert_eq!(client.get_price(&asset), PRICE_PRECISION);
}

// ---------------------------------------------------------------------------
// Price normalization edge cases
// ---------------------------------------------------------------------------

#[test]
fn test_normalize_fractional_price() {
    // $0.50 with 8 decimals = 50_000_000
    // Expected: 0.5 * PRICE_PRECISION = 5_000_000
    let (env, client, _) = setup(50_000_000, 8);
    let asset = Address::generate(&env);
    assert_eq!(client.get_price(&asset), 5_000_000);
}

#[test]
fn test_normalize_large_price() {
    // $100,000 (e.g., BTC) with 8 decimals = 100_000 * 10^8 = 10_000_000_000_000
    // Expected: 100_000 * PRICE_PRECISION = 1_000_000_000_000
    let (env, client, _) = setup(10_000_000_000_000i128, 8);
    let asset = Address::generate(&env);
    assert_eq!(client.get_price(&asset), 100_000 * PRICE_PRECISION);
}
