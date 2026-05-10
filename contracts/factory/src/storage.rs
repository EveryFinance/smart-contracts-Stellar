//! Factory storage layout.
//!
//! ## Key layout
//!
//! | Key                      | Storage    | Type      | Description                        |
//! |--------------------------|------------|-----------|------------------------------------|
//! | `Admin`                  | instance   | `Address` | Registry write-permission account  |
//! | `VaultCount`             | instance   | `u32`     | Number of registered vaults        |
//! | `VaultByIndex(u32)`      | persistent | `Address` | Vault address at position *i*      |
//! | `VaultPosition(Address)` | persistent | `u32`     | Position of a vault in the index   |
//! | `IsRegistered(Address)`  | persistent | `bool`    | O(1) membership flag               |
//!
//! Keeping `Admin` and `VaultCount` in instance storage keeps those two reads
//! free (one ledger-entry load shared with the contract instance).  The per-vault
//! keys live in **persistent** storage so the registry can grow to an arbitrary
//! number of entries without ever hitting the ~64 KiB instance-entry size limit.
//!
//! Removal uses **swap-and-pop**: the vault being removed is replaced with the
//! last entry, so both insert and remove are O(1) storage writes.

use crate::error::FactoryError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env};

// ---------------------------------------------------------------------------
// TTL constants (ledgers; ~6 s/ledger on Stellar mainnet)
// ---------------------------------------------------------------------------

/// Ledgers added to the instance TTL on every entry-point call.
/// 34 560 ledgers ≈ 2.4 days.
pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;

/// Trigger an instance bump when TTL drops below this threshold.
/// 17 280 ledgers ≈ 1.2 days.
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;

/// Ledgers added to each persistent vault entry on access.
/// 5 256 000 ledgers ≈ 1 year — registry entries must survive without
/// intervention for at least a year to prevent silent expiry corruption.
pub const PERSISTENT_BUMP_AMOUNT: u32 = 5_256_000;
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 2_628_000; // ≈ 6 months

/// Bump amount for the persistent factory-initialized flag.
/// u32::MAX ≈ 248 000 years — effectively permanent.
pub const FACTORY_INIT_PERSISTENT_BUMP_AMOUNT: u32 = u32::MAX;

/// Threshold for the persistent factory-initialized flag bump.
pub const FACTORY_INIT_PERSISTENT_LIFETIME_THRESHOLD: u32 = u32::MAX / 2;

// ---------------------------------------------------------------------------
// Storage key enum
// ---------------------------------------------------------------------------

/// All storage keys used by the Factory contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Factory admin address.
    Admin,
    /// Pending admin address awaiting acceptance (two-step transfer).
    PendingAdmin,
    /// Total number of registered vaults.
    VaultCount,
    /// Vault address at index *i* (0-based).
    VaultByIndex(u32),
    /// The index in the vault list at which `address` is stored.
    VaultPosition(Address),
    /// Membership flag for quick duplicate detection.
    IsRegistered(Address),
    /// Persistent initialization flag.
    ///
    /// Stored in **persistent** storage so it survives instance TTL expiry.
    /// If only the instance-storage `Admin` key were used as the guard,
    /// an attacker could wait for the instance to expire and re-call
    /// `__constructor` with their own admin address, effectively taking over
    /// the factory registry.
    Initialized,
}

// ---------------------------------------------------------------------------
// Internal TTL helpers
// ---------------------------------------------------------------------------

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

fn bump_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
}

// ---------------------------------------------------------------------------
// Factory initialization guard (persistent storage)
// ---------------------------------------------------------------------------

/// Persist the factory-initialized flag in **persistent** storage.
///
/// Persistent storage survives instance-entry TTL expiry, preventing an
/// attacker from calling `__constructor` again after the instance expires.
pub fn set_factory_initialized(env: &Env) {
    env.storage()
        .persistent()
        .set(&DataKey::Initialized, &true);
    env.storage().persistent().extend_ttl(
        &DataKey::Initialized,
        FACTORY_INIT_PERSISTENT_LIFETIME_THRESHOLD,
        FACTORY_INIT_PERSISTENT_BUMP_AMOUNT,
    );
}

/// Return `true` when the factory has been initialized.
///
/// Checks the **persistent** `Initialized` flag so that expiry of the
/// instance entry cannot be exploited to re-run `__constructor`.
pub fn is_factory_initialized(env: &Env) -> bool {
    env.storage()
        .persistent()
        .has(&DataKey::Initialized)
}

// ---------------------------------------------------------------------------
// Admin helpers  (instance storage)
// ---------------------------------------------------------------------------

pub fn set_admin(env: &Env, v: &Address) {
    bump_instance(env);
    env.storage().instance().set(&DataKey::Admin, v);
}

pub fn get_admin(env: &Env) -> Address {
    bump_instance(env);
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, FactoryError::NotInitialized))
}

pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

pub fn set_pending_admin(env: &Env, v: &Address) {
    bump_instance(env);
    env.storage().instance().set(&DataKey::PendingAdmin, v);
}

pub fn get_pending_admin(env: &Env) -> Option<Address> {
    bump_instance(env);
    env.storage().instance().get(&DataKey::PendingAdmin)
}

pub fn clear_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
}

// ---------------------------------------------------------------------------
// Count helpers  (instance storage)
// ---------------------------------------------------------------------------

pub fn get_vault_count(env: &Env) -> u32 {
    bump_instance(env);
    env.storage()
        .instance()
        .get(&DataKey::VaultCount)
        .unwrap_or(0)
}

pub fn set_vault_count(env: &Env, count: u32) {
    bump_instance(env);
    env.storage().instance().set(&DataKey::VaultCount, &count);
}

// ---------------------------------------------------------------------------
// Per-index vault helpers  (persistent storage)
// ---------------------------------------------------------------------------

pub fn get_vault_by_index(env: &Env, idx: u32) -> Address {
    let key = DataKey::VaultByIndex(idx);
    if !env.storage().persistent().has(&key) {
        panic_with_error!(env, FactoryError::VaultNotFound);
    }
    bump_persistent(env, &key);
    env.storage().persistent().get(&key).unwrap()
}

pub fn set_vault_by_index(env: &Env, idx: u32, vault: &Address) {
    let key = DataKey::VaultByIndex(idx);
    env.storage().persistent().set(&key, vault);
    bump_persistent(env, &key);
}

pub fn remove_vault_by_index(env: &Env, idx: u32) {
    env.storage()
        .persistent()
        .remove(&DataKey::VaultByIndex(idx));
}

// ---------------------------------------------------------------------------
// Reverse-index helpers  (persistent storage)
// ---------------------------------------------------------------------------

pub fn get_vault_position(env: &Env, vault: &Address) -> u32 {
    let key = DataKey::VaultPosition(vault.clone());
    if !env.storage().persistent().has(&key) {
        panic_with_error!(env, FactoryError::VaultNotFound);
    }
    bump_persistent(env, &key);
    env.storage().persistent().get(&key).unwrap()
}

pub fn set_vault_position(env: &Env, vault: &Address, idx: u32) {
    let key = DataKey::VaultPosition(vault.clone());
    env.storage().persistent().set(&key, &idx);
    bump_persistent(env, &key);
}

pub fn remove_vault_position(env: &Env, vault: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::VaultPosition(vault.clone()));
}

// ---------------------------------------------------------------------------
// Membership helpers  (persistent storage)
// ---------------------------------------------------------------------------

pub fn get_is_registered(env: &Env, vault: &Address) -> bool {
    let key = DataKey::IsRegistered(vault.clone());
    if env.storage().persistent().has(&key) {
        bump_persistent(env, &key);
        env.storage().persistent().get(&key).unwrap_or(false)
    } else {
        false
    }
}

pub fn set_registered(env: &Env, vault: &Address) {
    let key = DataKey::IsRegistered(vault.clone());
    env.storage().persistent().set(&key, &true);
    bump_persistent(env, &key);
}

pub fn remove_registered(env: &Env, vault: &Address) {
    env.storage()
        .persistent()
        .remove(&DataKey::IsRegistered(vault.clone()));
}
