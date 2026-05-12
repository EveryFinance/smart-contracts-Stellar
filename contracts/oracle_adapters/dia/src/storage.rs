use soroban_sdk::{contracttype, panic_with_error, Address, Env, String};

use crate::error::DiaAdapterError;

pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;
pub const PERSISTENT_BUMP_AMOUNT: u32 = 5_256_000;
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 2_628_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    PendingAdmin,
    Initialized,
    /// Address of the DIA oracle contract.
    DiaContract,
    /// Maximum accepted upstream oracle age in seconds.
    MaxAgeSecs,
    /// DIA query key for a given asset address (e.g. "BTC/USD").
    AssetKey(Address),
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
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::Initialized, u32::MAX / 2, u32::MAX);
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().persistent().set(&DataKey::Admin, admin);
    bump_persistent(env, &DataKey::Admin);
}

pub fn get_admin(env: &Env) -> Address {
    bump_persistent(env, &DataKey::Admin);
    env.storage()
        .persistent()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, DiaAdapterError::NotInitialized))
}

pub fn set_pending_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::PendingAdmin, admin);
}

pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::PendingAdmin)
}

pub fn clear_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
}

// ---------------------------------------------------------------------------
// DIA contract address
// ---------------------------------------------------------------------------

pub fn set_dia_contract(env: &Env, addr: &Address) {
    env.storage().persistent().set(&DataKey::DiaContract, addr);
    bump_persistent(env, &DataKey::DiaContract);
}

pub fn get_dia_contract(env: &Env) -> Address {
    bump_persistent(env, &DataKey::DiaContract);
    env.storage()
        .persistent()
        .get(&DataKey::DiaContract)
        .unwrap_or_else(|| panic_with_error!(env, DiaAdapterError::NotInitialized))
}

pub fn set_max_age_secs(env: &Env, secs: u64) {
    env.storage().persistent().set(&DataKey::MaxAgeSecs, &secs);
    bump_persistent(env, &DataKey::MaxAgeSecs);
}

pub fn get_max_age_secs(env: &Env) -> u64 {
    bump_persistent(env, &DataKey::MaxAgeSecs);
    env.storage()
        .persistent()
        .get(&DataKey::MaxAgeSecs)
        .unwrap_or_else(|| panic_with_error!(env, DiaAdapterError::NotInitialized))
}

// ---------------------------------------------------------------------------
// Asset → DIA key mapping
// ---------------------------------------------------------------------------

pub fn set_asset_key(env: &Env, asset: &Address, key: &String) {
    let data_key = DataKey::AssetKey(asset.clone());
    env.storage().persistent().set(&data_key, key);
    bump_persistent(env, &data_key);
}

pub fn get_asset_key(env: &Env, asset: &Address) -> Option<String> {
    let data_key = DataKey::AssetKey(asset.clone());
    if env.storage().persistent().has(&data_key) {
        bump_persistent(env, &data_key);
        env.storage().persistent().get(&data_key)
    } else {
        None
    }
}

pub fn remove_asset_key(env: &Env, asset: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::AssetKey(asset.clone()));
}
