use soroban_sdk::{contracttype, panic_with_error, Address, Env, Vec};

use crate::error::AssetHandlerError;

// ---------------------------------------------------------------------------
// TTL constants
// ---------------------------------------------------------------------------

pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;
pub const PERSISTENT_BUMP_AMOUNT: u32 = 5_256_000;
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 2_628_000;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Protocol admin (persistent — survives instance TTL expiry).
    Admin,
    /// Pending admin for two-step transfer.
    PendingAdmin,
    /// Persistent initialization flag.
    Initialized,
    /// Ordered list of all registered asset addresses.
    RegisteredAssets,
    /// Optional per-asset oracle. When set, takes priority over the global
    /// primary/fallback oracles. Must implement `get_price(asset) -> i128`.
    AssetOracle(Address),
    /// Primary global oracle (e.g. Reflector adapter).
    /// Used for all assets that have no per-asset oracle.
    PrimaryOracle,
    /// Fallback global oracle (e.g. DIA adapter).
    /// Used when the primary oracle reverts or returns 0.
    FallbackOracle,
}

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

fn bump_persistent(env: &Env, key: &DataKey) {
    env.storage().persistent().extend_ttl(
        key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

// ---------------------------------------------------------------------------
// Init guard
// ---------------------------------------------------------------------------

pub fn is_initialized(env: &Env) -> bool {
    env.storage().persistent().has(&DataKey::Initialized)
}

pub fn set_initialized(env: &Env) {
    env.storage().persistent().set(&DataKey::Initialized, &true);
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().persistent().set(&DataKey::Admin, admin);
}

pub fn get_admin(env: &Env) -> Address {
    bump_persistent(env, &DataKey::Admin);
    env.storage()
        .persistent()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, AssetHandlerError::NotInitialized))
}

pub fn set_pending_admin(env: &Env, admin: &Address) {
    bump_instance(env);
    env.storage().instance().set(&DataKey::PendingAdmin, admin);
}

pub fn get_pending_admin(env: &Env) -> Option<Address> {
    bump_instance(env);
    env.storage().instance().get(&DataKey::PendingAdmin)
}

pub fn clear_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
}

// ---------------------------------------------------------------------------
// Registered assets list
// ---------------------------------------------------------------------------

pub fn get_registered_assets(env: &Env) -> Vec<Address> {
    let key = DataKey::RegisteredAssets;
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
        env.storage().persistent().get(&key).unwrap()
    } else {
        Vec::new(env)
    }
}

pub fn set_registered_assets(env: &Env, assets: &Vec<Address>) {
    let key = DataKey::RegisteredAssets;
    env.storage().persistent().set(&key, assets);
    bump_persistent(env, &key);
}

// ---------------------------------------------------------------------------
// Per-asset oracle (optional override)
// ---------------------------------------------------------------------------

pub fn set_asset_oracle(env: &Env, asset: &Address, oracle: &Address) {
    let key = DataKey::AssetOracle(asset.clone());
    env.storage().persistent().set(&key, oracle);
    bump_persistent(env, &key);
}

pub fn get_asset_oracle(env: &Env, asset: &Address) -> Option<Address> {
    let key = DataKey::AssetOracle(asset.clone());
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
        env.storage().persistent().get(&key)
    } else {
        None
    }
}

pub fn remove_asset_oracle(env: &Env, asset: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::AssetOracle(asset.clone()));
}

// ---------------------------------------------------------------------------
// Primary oracle (global — e.g. Reflector)
// ---------------------------------------------------------------------------

pub fn set_primary_oracle(env: &Env, oracle: &Address) {
    env.storage()
        .persistent()
        .set(&DataKey::PrimaryOracle, oracle);
    bump_persistent(env, &DataKey::PrimaryOracle);
}

pub fn get_primary_oracle(env: &Env) -> Option<Address> {
    let key = DataKey::PrimaryOracle;
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
        env.storage().persistent().get(&key)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Fallback oracle (global — e.g. DIA)
// ---------------------------------------------------------------------------

pub fn set_fallback_oracle(env: &Env, oracle: &Address) {
    env.storage()
        .persistent()
        .set(&DataKey::FallbackOracle, oracle);
    bump_persistent(env, &DataKey::FallbackOracle);
}

pub fn get_fallback_oracle(env: &Env) -> Option<Address> {
    let key = DataKey::FallbackOracle;
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
        env.storage().persistent().get(&key)
    } else {
        None
    }
}
