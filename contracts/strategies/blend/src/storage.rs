//! Blend strategy storage — multi-position lending manager.
//!
//! One strategy instance manages **all** Blend lending positions for the
//! owning vault, keyed by pool address.  Each pool holds one supplied asset.
//!
//! | Key                   | Type            | Description                             |
//! |-----------------------|-----------------|-----------------------------------------|
//! | `Vault`               | `Address`       | Owning vault contract                   |
//! | `Name`                | `String`        | Human-readable strategy name            |
//! | `Factory`             | `Address?`      | Factory (cached from vault at init)     |
//! | `AssetHandler`        | `Address?`      | AssetHandler (lazy-cached from factory) |
//! | `Initialized`         | `bool` (persist)| One-time init guard                     |
//! | `Position(pool)`      | `LendingPosition`| Per-pool asset record                  |
//! | `ActivePositions`     | `Vec<Address>`  | Pools with potentially non-zero balance |

use crate::error::BlendStrategyError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, String, Symbol, Vec};

// ---------------------------------------------------------------------------
// TTL constants (ledgers; ~6 s/ledger on Stellar mainnet)
// ---------------------------------------------------------------------------

pub const INSTANCE_BUMP_AMOUNT: u32 = 518_400;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 259_200;

// ---------------------------------------------------------------------------
// Per-pool position record
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug)]
pub struct LendingPosition {
    /// The token supplied to this Blend pool.
    pub asset: Address,
}

// ---------------------------------------------------------------------------
// Storage key enum
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Vault,
    Name,
    /// Factory address (cached from vault at init).
    Factory,
    /// AssetHandler (lazy-cached from factory on first valuation).
    AssetHandler,
    /// Persistent init guard (survives instance TTL expiry).
    Initialized,
    /// Per-pool lending position, keyed by pool address.
    Position(Address),
    /// Pool addresses where supply has been called (may have non-zero balance).
    ActivePositions,
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

pub fn set_vault(env: &Env, vault: &Address) {
    env.storage().instance().set(&DataKey::Vault, vault);
}

pub fn get_vault(env: &Env) -> Address {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Vault)
        .unwrap_or_else(|| panic_with_error!(env, BlendStrategyError::NotInitialized))
}

pub fn set_name(env: &Env, name: &String) {
    env.storage().instance().set(&DataKey::Name, name);
}

pub fn get_name(env: &Env) -> String {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Name)
        .unwrap_or_else(|| panic_with_error!(env, BlendStrategyError::NotInitialized))
}

pub fn set_factory(env: &Env, factory: &Address) {
    env.storage().instance().set(&DataKey::Factory, factory);
}

pub fn get_factory(env: &Env) -> Option<Address> {
    bump(env);
    env.storage().instance().get(&DataKey::Factory)
}

pub fn set_asset_handler(env: &Env, ah: &Address) {
    env.storage().instance().set(&DataKey::AssetHandler, ah);
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

pub fn get_position(env: &Env, pool: &Address) -> Option<LendingPosition> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Position(pool.clone()))
}

pub fn set_position(env: &Env, pool: &Address, position: &LendingPosition) {
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
