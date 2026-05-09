use soroban_sdk::{contracttype, panic_with_error, Address, Env};

use crate::error::OracleError;

// ---------------------------------------------------------------------------
// TTL constants
// ---------------------------------------------------------------------------

pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;
pub const PERSISTENT_BUMP_AMOUNT: u32 = 518_400;
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 259_200;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Admin address — instance storage.
    Admin,
    /// Maximum acceptable age for price data in ledgers.
    MaxAgeLedgers,
    /// Price of an asset, PRICE_PRECISION-scaled — persistent storage.
    Price(Address),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PriceData {
    pub price: i128,
    pub updated_ledger: u32,
}

// ---------------------------------------------------------------------------
// Admin helpers
// ---------------------------------------------------------------------------

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().persistent().set(&DataKey::Admin, admin);
    env.storage().persistent().extend_ttl(
        &DataKey::Admin,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

pub fn get_admin(env: &Env) -> Address {
    env.storage().persistent().extend_ttl(
        &DataKey::Admin,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
    env.storage()
        .persistent()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, OracleError::NotInitialized))
}

pub fn has_admin(env: &Env) -> bool {
    env.storage().persistent().has(&DataKey::Admin)
}

// ---------------------------------------------------------------------------
// Price helpers
// ---------------------------------------------------------------------------

pub fn set_price(env: &Env, asset: &Address, price: i128, updated_ledger: u32) {
    let key = DataKey::Price(asset.clone());
    let value = PriceData {
        price,
        updated_ledger,
    };
    env.storage().persistent().set(&key, &value);
    env.storage().persistent().extend_ttl(
        &key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

/// Returns the stored price record, or `None` if not set.
pub fn get_price_data(env: &Env, asset: &Address) -> Option<PriceData> {
    let key = DataKey::Price(asset.clone());
    if let Some(value) = env.storage().persistent().get::<DataKey, PriceData>(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        Some(value)
    } else {
        None
    }
}

pub fn set_max_age_ledgers(env: &Env, max_age: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::MaxAgeLedgers, &max_age);
    env.storage().persistent().extend_ttl(
        &DataKey::MaxAgeLedgers,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

pub fn get_max_age_ledgers(env: &Env) -> u32 {
    env.storage().persistent().extend_ttl(
        &DataKey::MaxAgeLedgers,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
    env.storage()
        .persistent()
        .get(&DataKey::MaxAgeLedgers)
        .unwrap_or_else(|| panic_with_error!(env, OracleError::NotInitialized))
}
