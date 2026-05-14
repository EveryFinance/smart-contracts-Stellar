//! # ReflectorAdapter
//!
//! Adapter contract that wraps the [Reflector](https://reflector.network) decentralized
//! oracle and exposes the unified `get_price(asset) -> i128` interface expected by
//! `AssetHandler`.
//!
//! ## How it works
//!
//! ```text
//! AssetHandler::get_price(asset)
//!   └─ ReflectorAdapter::get_price(asset)
//!        └─ Reflector::lastprice(Asset::Stellar(asset)) -> Option<PriceData>
//!             ├─ Some(PriceData { price, .. }) → normalize to PRICE_PRECISION
//!             └─ None                           → return 0 (unavailable)
//! ```
//!
//! ## Price normalization
//!
//! Reflector encodes prices with `decimals` decimal places (typically 8).
//! This adapter converts to `PRICE_PRECISION` (7 decimal places):
//!
//! ```text
//! normalized = reflector_price * PRICE_PRECISION / 10^decimals
//! ```
//!
//! The `decimals` value is read from Reflector at initialization and stored.
//! It can be refreshed by the admin via `refresh_decimals()` if it ever changes.
//!
//! ## Reflector asset types
//!
//! - `Asset::Stellar(Address)` — Stellar Classic / Soroban token (used here)
//! - `Asset::Other(Symbol)`   — External currencies (EUR, BTC/USD, …)
//!
//! This adapter always queries using `Asset::Stellar(asset_address)`.

#![no_std]

mod error;
mod storage;

pub use error::ReflectorAdapterError;

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, Address, Env, IntoVal, Symbol, Val, Vec,
};

use storage::{
    clear_pending_admin, get_admin, get_decimals, get_max_age_secs, get_pending_admin,
    get_reflector_contract, is_initialized, set_admin, set_decimals,
    set_max_age_secs, set_pending_admin, set_reflector_contract, DataKey, INSTANCE_BUMP_AMOUNT,
    INSTANCE_LIFETIME_THRESHOLD,
};

/// Protocol price precision: 7 decimal places (Stellar native).
pub const PRICE_PRECISION: i128 = 10_000_000;
/// Default freshness window for upstream Reflector prices.
const DEFAULT_MAX_AGE_SECS: u64 = 3_600;

// ---------------------------------------------------------------------------
// Local mirrors of Reflector's on-chain types
//
// These must match the XDR encoding of the types in the Reflector contract.
// `#[contracttype]` structs are encoded as Soroban map values keyed by field
// name, so field names and types must be identical to the originals.
// ---------------------------------------------------------------------------

/// Mirror of Reflector's `Asset` enum.
#[contracttype]
#[derive(Clone)]
pub enum ReflectorAsset {
    /// A Stellar / Soroban token contract address.
    Stellar(Address),
    /// An external asset symbol (e.g. `Symbol::new(env, "BTC")`).
    Other(Symbol),
}

/// Mirror of Reflector's `PriceData` struct.
#[contracttype]
#[derive(Clone)]
pub struct PriceData {
    /// Price encoded with `decimals` decimal places.
    pub price: i128,
    /// Ledger timestamp of the last update.
    pub timestamp: u64,
}

#[contract]
pub struct ReflectorAdapter;

#[contractimpl]
impl ReflectorAdapter {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the adapter.
    ///
    /// `decimals` must match Reflector's reported precision (call `reflector.decimals()`
    /// off-chain to confirm — mainnet Reflector returns 14). Use `refresh_decimals()`
    /// after deployment if it ever needs updating.
    pub fn __constructor(env: Env, admin: Address, reflector: Address, decimals: u32) {
        if is_initialized(&env) {
            panic_with_error!(&env, ReflectorAdapterError::AlreadyInitialized);
        }
        admin.require_auth();

        // Write directly to avoid extend_ttl on brand-new persistent entries,
        // which fails in a constructor (entry not yet in ledger footprint).
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::ReflectorContract, &reflector);
        env.storage().persistent().set(&DataKey::Decimals, &decimals);
        env.storage().persistent().set(&DataKey::MaxAgeSecs, &DEFAULT_MAX_AGE_SECS);
        env.storage().persistent().set(&DataKey::Initialized, &true);
    }

    // -----------------------------------------------------------------------
    // Core interface — called by AssetHandler
    // -----------------------------------------------------------------------

    /// Return the `PRICE_PRECISION`-scaled price of `asset`.
    ///
    /// Queries Reflector's `lastprice(Asset::Stellar(asset))`.
    /// Returns `0` when Reflector has no data for the asset.
    pub fn get_price(env: Env, asset: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        let reflector = get_reflector_contract(&env);
        let reflector_asset = ReflectorAsset::Stellar(asset);
        let args: Vec<Val> = (reflector_asset,).into_val(&env);

        let result: Option<PriceData> =
            env.invoke_contract(&reflector, &Symbol::new(&env, "lastprice"), args);

        match result {
            Some(data) if data.price > 0 && Self::is_fresh(&env, data.timestamp) => {
                Self::normalize(data.price, get_decimals(&env))
            }
            _ => 0,
        }
    }

    // -----------------------------------------------------------------------
    // Admin — oracle management
    // -----------------------------------------------------------------------

    /// Update the Reflector contract address and re-query decimals. Admin only.
    pub fn set_reflector(env: Env, caller: Address, reflector: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, ReflectorAdapterError::NotAdmin);
        }
        let decimals: u32 =
            env.invoke_contract(&reflector, &Symbol::new(&env, "decimals"), Vec::new(&env));
        set_reflector_contract(&env, &reflector);
        set_decimals(&env, decimals);
    }

    /// Re-read `decimals()` from the Reflector contract and persist it. Admin only.
    ///
    /// Only needed if Reflector ever changes its decimal precision (very rare).
    pub fn refresh_decimals(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, ReflectorAdapterError::NotAdmin);
        }
        let reflector = get_reflector_contract(&env);
        let decimals: u32 =
            env.invoke_contract(&reflector, &Symbol::new(&env, "decimals"), Vec::new(&env));
        set_decimals(&env, decimals);
    }

    /// Set the maximum accepted upstream Reflector price age in seconds. Admin only.
    pub fn set_max_age_secs(env: Env, caller: Address, secs: u64) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, ReflectorAdapterError::NotAdmin);
        }
        if secs == 0 {
            panic_with_error!(&env, ReflectorAdapterError::InvalidMaxAge);
        }
        set_max_age_secs(&env, secs);
    }

    // -----------------------------------------------------------------------
    // Admin — two-step transfer
    // -----------------------------------------------------------------------

    pub fn set_pending_admin(env: Env, caller: Address, new_admin: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, ReflectorAdapterError::NotAdmin);
        }
        set_pending_admin(&env, &new_admin);
    }

    pub fn accept_admin(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        let pending = get_pending_admin(&env)
            .unwrap_or_else(|| panic_with_error!(&env, ReflectorAdapterError::NoPendingAdmin));
        if caller != pending {
            panic_with_error!(&env, ReflectorAdapterError::NotAdmin);
        }
        set_admin(&env, &pending);
        clear_pending_admin(&env);
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_admin(&env)
    }

    pub fn get_reflector(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_reflector_contract(&env)
    }

    pub fn get_decimals(env: Env) -> u32 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_decimals(&env)
    }

    pub fn get_max_age_secs(env: Env) -> u64 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_max_age_secs(&env)
    }

    // -----------------------------------------------------------------------
    // Internal
    // -----------------------------------------------------------------------

    /// Convert a Reflector price (with `oracle_decimals` precision) to
    /// `PRICE_PRECISION` (7 decimal places).
    ///
    /// Formula: `reflector_price * PRICE_PRECISION / 10^oracle_decimals`
    ///
    /// Uses checked arithmetic; returns 0 on overflow or unsupported decimals.
    fn normalize(reflector_price: i128, oracle_decimals: u32) -> i128 {
        let Some(divisor) = 10i128.checked_pow(oracle_decimals) else {
            return 0;
        };
        reflector_price
            .checked_mul(PRICE_PRECISION)
            .and_then(|p| p.checked_div(divisor))
            .unwrap_or(0)
    }

    fn is_fresh(env: &Env, timestamp: u64) -> bool {
        let now = env.ledger().timestamp();
        timestamp <= now && now.saturating_sub(timestamp) <= get_max_age_secs(env)
    }
}

#[cfg(test)]
mod test;
