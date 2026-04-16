//! Factory storage layout.
//!
//! All factory state lives in **instance storage** so it shares the same TTL
//! as the contract instance itself.
//!
//! | Key      | Type           | Description                             |
//! |----------|----------------|-----------------------------------------|
//! | `Admin`  | `Address`      | Address with registry write permissions |
//! | `Vaults` | `Vec<Address>` | Append-only list of registered vaults   |

use crate::error::FactoryError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Vec};

// ---------------------------------------------------------------------------
// TTL constants (ledgers; ~6 s/ledger on Stellar mainnet)
// ---------------------------------------------------------------------------

/// Ledgers added to the instance TTL on every entry-point call.
/// 34 560 ledgers ≈ 2.4 days.
pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;

/// Trigger a bump when the remaining instance TTL drops below this threshold.
/// 17 280 ledgers ≈ 1.2 days.
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;

// ---------------------------------------------------------------------------
// Storage key enum
// ---------------------------------------------------------------------------

/// All keys stored in instance storage by the Factory contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Factory admin address — the only account allowed to register or remove
    /// vaults, and to transfer the admin role.
    Admin,

    /// Ordered list of all vault contract addresses registered with this
    /// factory.  New vaults are appended; removed vaults are compacted out.
    Vaults,
}

// ---------------------------------------------------------------------------
// Internal TTL helper
// ---------------------------------------------------------------------------

/// Extend the instance TTL on every call so data is never archived.
fn bump(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

// ---------------------------------------------------------------------------
// Initialization guard
// ---------------------------------------------------------------------------

/// Return `true` when the factory has been initialized (i.e. `Admin` key exists).
pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

// ---------------------------------------------------------------------------
// Admin helpers
// ---------------------------------------------------------------------------

/// Persist the admin address in instance storage.
pub fn set_admin(env: &Env, v: &Address) {
    bump(env);
    env.storage().instance().set(&DataKey::Admin, v);
}

/// Read the admin address from instance storage.
///
/// # Panics
/// Panics with [`FactoryError::NotInitialized`] when the factory has not been
/// initialized.
pub fn get_admin(env: &Env) -> Address {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, FactoryError::NotInitialized))
}

// ---------------------------------------------------------------------------
// Vault list helpers
// ---------------------------------------------------------------------------

/// Read the complete vault list from instance storage.
///
/// Returns an empty `Vec` before any vaults have been registered.
pub fn get_vaults(env: &Env) -> Vec<Address> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Vaults)
        .unwrap_or_else(|| Vec::new(env))
}

/// Persist the complete vault list to instance storage.
pub fn set_vaults(env: &Env, v: &Vec<Address>) {
    bump(env);
    env.storage().instance().set(&DataKey::Vaults, v);
}
