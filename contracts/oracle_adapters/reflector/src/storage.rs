use soroban_sdk::{contracttype, panic_with_error, Address, Env};

use crate::error::ReflectorAdapterError;

pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;       // ~2.4 days
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280; // ~1.2 days
pub const PERSISTENT_BUMP_AMOUNT: u32 = 5_256_000;
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 2_628_000;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    PendingAdmin,
    Initialized,
    /// Address of the Reflector oracle contract.
    ReflectorContract,
    /// Decimal precision reported by the Reflector contract (typically 8).
    Decimals,
}

fn bump_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
}

// ---------------------------------------------------------------------------
// Init guard
// ---------------------------------------------------------------------------

pub fn is_initialized(env: &Env) -> bool {
    env.storage().persistent().has(&DataKey::Initialized)
}

pub fn set_initialized(env: &Env) {
    env.storage()
        .persistent()
        .set(&DataKey::Initialized, &true);
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
        .unwrap_or_else(|| panic_with_error!(env, ReflectorAdapterError::NotInitialized))
}

pub fn set_pending_admin(env: &Env, admin: &Address) {
    env.storage()
        .instance()
        .set(&DataKey::PendingAdmin, admin);
}

pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::PendingAdmin)
}

pub fn clear_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
}

// ---------------------------------------------------------------------------
// Reflector contract address + decimals
// ---------------------------------------------------------------------------

pub fn set_reflector_contract(env: &Env, addr: &Address) {
    env.storage()
        .persistent()
        .set(&DataKey::ReflectorContract, addr);
    bump_persistent(env, &DataKey::ReflectorContract);
}

pub fn get_reflector_contract(env: &Env) -> Address {
    bump_persistent(env, &DataKey::ReflectorContract);
    env.storage()
        .persistent()
        .get(&DataKey::ReflectorContract)
        .unwrap_or_else(|| panic_with_error!(env, ReflectorAdapterError::NotInitialized))
}

pub fn set_decimals(env: &Env, decimals: u32) {
    env.storage()
        .persistent()
        .set(&DataKey::Decimals, &decimals);
    bump_persistent(env, &DataKey::Decimals);
}

pub fn get_decimals(env: &Env) -> u32 {
    bump_persistent(env, &DataKey::Decimals);
    env.storage()
        .persistent()
        .get(&DataKey::Decimals)
        .unwrap_or(8) // Reflector standard
}
