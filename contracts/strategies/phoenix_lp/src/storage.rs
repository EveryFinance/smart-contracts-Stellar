//! PhoenixLpStrategy storage — multi-position AMM liquidity manager.
//!
//! One strategy instance manages **all** Phoenix LP positions for the owning
//! vault, keyed by pool address.  Each pool has two assets and a share token.
//!
//! | Key                   | Type             | Description                            |
//! |-----------------------|------------------|----------------------------------------|
//! | `Vault`               | `Address`        | Owning vault contract                  |
//! | `Name`                | `String`         | Human-readable strategy name           |
//! | `Factory`             | `Address?`       | Factory (cached from vault at init)    |
//! | `AssetHandler`        | `Address?`       | AssetHandler (lazy-cached)             |
//! | `Initialized`         | `bool` (persist) | One-time init guard                    |
//! | `Position(pool)`      | `PhoenixPosition`| Per-pool position data + share balance |
//! | `ActivePositions`     | `Vec<Address>`   | Pools with total_shares > 0            |

use crate::error::PhoenixLpError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, String, Symbol, Vec};

// ---------------------------------------------------------------------------
// TTL constants (ledgers; ~6 s/ledger)
// ---------------------------------------------------------------------------

pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;

// ---------------------------------------------------------------------------
// Per-pool position record
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug)]
pub struct PhoenixPosition {
    pub asset_a: Address,
    pub asset_b: Address,
    /// Phoenix LP share token for this pool (auto-queried on first add_liquidity).
    pub share_token: Address,
    /// Locally tracked share balance (updated on every add/remove).
    pub total_shares: i128,
}

// ---------------------------------------------------------------------------
// Storage key enum
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Vault,
    Name,
    Factory,
    AssetHandler,
    Initialized,
    Position(Address), // pool_address → PhoenixPosition
    ActivePositions,   // Vec<Address> of pools with total_shares > 0
}

// ---------------------------------------------------------------------------
// TTL helper
// ---------------------------------------------------------------------------

fn bump(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

// ---------------------------------------------------------------------------
// Initialization guard
// ---------------------------------------------------------------------------

pub fn set_initialized(env: &Env) {
    env.storage().persistent().set(&DataKey::Initialized, &true);
}

pub fn is_initialized(env: &Env) -> bool {
    env.storage().persistent().has(&DataKey::Initialized)
}

// ---------------------------------------------------------------------------
// Simple accessors
// ---------------------------------------------------------------------------

pub fn set_vault(env: &Env, v: &Address) {
    env.storage().instance().set(&DataKey::Vault, v);
}

pub fn get_vault(env: &Env) -> Address {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Vault)
        .unwrap_or_else(|| panic_with_error!(env, PhoenixLpError::NotInitialized))
}

pub fn set_name(env: &Env, v: &String) {
    env.storage().instance().set(&DataKey::Name, v);
}

pub fn get_name(env: &Env) -> String {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Name)
        .unwrap_or_else(|| panic_with_error!(env, PhoenixLpError::NotInitialized))
}

pub fn set_factory(env: &Env, v: &Address) {
    env.storage().instance().set(&DataKey::Factory, v);
}

pub fn get_factory(env: &Env) -> Option<Address> {
    bump(env);
    env.storage().instance().get(&DataKey::Factory)
}

pub fn set_asset_handler(env: &Env, v: &Address) {
    env.storage().instance().set(&DataKey::AssetHandler, v);
}

pub fn get_asset_handler(env: &Env) -> Option<Address> {
    bump(env);
    env.storage().instance().get(&DataKey::AssetHandler)
}

/// Lazy-resolve and cache the AssetHandler address.
pub fn get_or_cache_asset_handler(env: &Env) -> Option<Address> {
    if let Some(ah) = get_asset_handler(env) {
        return Some(ah);
    }
    if let Some(factory) = get_factory(env) {
        let ah_opt: Option<Address> = env.invoke_contract(
            &factory,
            &Symbol::new(env, "get_asset_handler"),
            soroban_sdk::vec![env].into(),
        );
        if let Some(ref ah) = ah_opt {
            set_asset_handler(env, ah);
            return Some(ah.clone());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Per-pool position accessors
// ---------------------------------------------------------------------------

pub fn get_position(env: &Env, pool: &Address) -> Option<PhoenixPosition> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Position(pool.clone()))
}

pub fn set_position(env: &Env, pool: &Address, position: &PhoenixPosition) {
    env.storage()
        .instance()
        .set(&DataKey::Position(pool.clone()), position);
}

// ---------------------------------------------------------------------------
// Active-positions index
// ---------------------------------------------------------------------------

pub fn get_active_positions(env: &Env) -> Vec<Address> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::ActivePositions)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_active_positions(env: &Env, positions: &Vec<Address>) {
    env.storage()
        .instance()
        .set(&DataKey::ActivePositions, positions);
}

pub fn add_to_active_positions(env: &Env, pool: &Address) {
    let mut active = get_active_positions(env);
    if !active.contains(pool.clone()) {
        active.push_back(pool.clone());
        set_active_positions(env, &active);
    }
}

pub fn remove_from_active_positions(env: &Env, pool: &Address) {
    let active = get_active_positions(env);
    let mut updated = Vec::new(env);
    for item in active.iter() {
        if item != *pool {
            updated.push_back(item);
        }
    }
    set_active_positions(env, &updated);
}
