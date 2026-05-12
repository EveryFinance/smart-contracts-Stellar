//! SoroswapLpStrategy storage layout — multi-position design.
//!
//! One strategy instance manages **all** Soroswap LP positions for a vault,
//! keyed by LP-token address (each Soroswap pair has a unique LP token).
//!
//! | Key                    | Type            | Description                              |
//! |------------------------|-----------------|------------------------------------------|
//! | `Vault`                | `Address`       | Owning vault contract                    |
//! | `Router`               | `Address`       | Soroswap router contract                 |
//! | `Name`                 | `String`        | Human-readable strategy name             |
//! | `Factory`              | `Address?`      | Factory address (cached from vault)      |
//! | `AssetHandler`         | `Address?`      | AssetHandler (lazy-cached from factory)  |
//! | `Initialized`          | `bool` (persist)| One-time init guard                      |
//! | `Position(lp_token)`   | `LpPosition`    | Per-pair position data + LP balance      |
//! | `ActivePositions`      | `Vec<Address>`  | LP tokens with lp_balance > 0            |

use crate::error::SoroswapLpError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, String, Symbol, Vec};

// ---------------------------------------------------------------------------
// TTL constants (ledgers; ~6 s/ledger on Stellar mainnet)
// ---------------------------------------------------------------------------

pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;

// ---------------------------------------------------------------------------
// LP position record stored per pair
// ---------------------------------------------------------------------------

/// State for one Soroswap LP position (one pair).
#[contracttype]
#[derive(Clone, Debug)]
pub struct LpPosition {
    /// First token of the pair as passed at first add_liquidity.
    pub asset_a: Address,
    /// Second token of the pair as passed at first add_liquidity.
    pub asset_b: Address,
    /// Cumulative LP tokens held by this strategy for this pair.
    pub lp_balance: i128,
    /// `true` when the pair's internal token0 == asset_a.
    /// Cached at first deposit so `pair.token0()` is called only once per pair.
    pub token0_is_asset_a: bool,
}

// ---------------------------------------------------------------------------
// Storage key enum
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Owning vault.
    Vault,
    /// Soroswap router.
    Router,
    /// Human-readable name.
    Name,
    /// Factory (cached from vault at init to avoid re-entry).
    Factory,
    /// AssetHandler (lazy-cached from factory on first valuation).
    AssetHandler,
    /// Persistent init guard (survives instance TTL expiry).
    Initialized,
    /// Per-pair LP position, keyed by LP token address.
    Position(Address),
    /// Ordered list of LP token addresses that have lp_balance > 0.
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

macro_rules! simple_set {
    ($fn_name:ident, $key:ident, $t:ty) => {
        pub fn $fn_name(env: &Env, val: &$t) {
            env.storage().instance().set(&DataKey::$key, val);
        }
    };
}

macro_rules! simple_get {
    ($fn_name:ident, $key:ident, $t:ty) => {
        pub fn $fn_name(env: &Env) -> $t {
            bump(env);
            env.storage()
                .instance()
                .get(&DataKey::$key)
                .unwrap_or_else(|| panic_with_error!(env, SoroswapLpError::NotInitialized))
        }
    };
}

macro_rules! simple_get_opt {
    ($fn_name:ident, $key:ident, $t:ty) => {
        pub fn $fn_name(env: &Env) -> Option<$t> {
            bump(env);
            env.storage().instance().get(&DataKey::$key)
        }
    };
}

simple_set!(set_vault, Vault, Address);
simple_get!(get_vault, Vault, Address);
simple_set!(set_router, Router, Address);
simple_get!(get_router, Router, Address);
simple_set!(set_name, Name, String);
simple_get!(get_name, Name, String);
simple_set!(set_factory, Factory, Address);
simple_get_opt!(get_factory, Factory, Address);

pub fn set_asset_handler(env: &Env, v: &Address) {
    env.storage().instance().set(&DataKey::AssetHandler, v);
}

pub fn get_asset_handler(env: &Env) -> Option<Address> {
    bump(env);
    env.storage().instance().get(&DataKey::AssetHandler)
}

/// Lazy-resolve and cache the AssetHandler address.
/// On first call (cache empty): queries factory.get_asset_handler() and stores it.
/// Subsequent calls: reads directly from instance storage (no cross-contract call).
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
// Per-pair position accessors
// ---------------------------------------------------------------------------

/// Return the position for `lp_token`, or `None` if not yet created.
pub fn get_position(env: &Env, lp_token: &Address) -> Option<LpPosition> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Position(lp_token.clone()))
}

/// Persist (create or update) the position for `lp_token`.
pub fn set_position(env: &Env, lp_token: &Address, position: &LpPosition) {
    env.storage()
        .instance()
        .set(&DataKey::Position(lp_token.clone()), position);
}

// ---------------------------------------------------------------------------
// Active-positions index
// ---------------------------------------------------------------------------

/// Return the list of LP token addresses that have a non-zero balance.
pub fn get_active_positions(env: &Env) -> Vec<Address> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::ActivePositions)
        .unwrap_or_else(|| Vec::new(env))
}

/// Overwrite the active-positions list.
pub fn set_active_positions(env: &Env, positions: &Vec<Address>) {
    env.storage()
        .instance()
        .set(&DataKey::ActivePositions, positions);
}

/// Add `lp_token` to the active list if not already present.
pub fn add_to_active_positions(env: &Env, lp_token: &Address) {
    let mut active = get_active_positions(env);
    if !active.contains(lp_token.clone()) {
        active.push_back(lp_token.clone());
        set_active_positions(env, &active);
    }
}

/// Remove `lp_token` from the active list (called when balance reaches zero).
pub fn remove_from_active_positions(env: &Env, lp_token: &Address) {
    let active = get_active_positions(env);
    let mut updated = Vec::new(env);
    for item in active.iter() {
        if item != *lp_token {
            updated.push_back(item);
        }
    }
    set_active_positions(env, &updated);
}
