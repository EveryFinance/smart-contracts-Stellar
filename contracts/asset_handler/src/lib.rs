//! # AssetHandler — Asset Registry with Dual On-Chain Oracle + Per-Asset Override
//!
//! ## Price resolution order
//!
//! ```text
//! get_price(asset)
//!   │
//!   ├─ 1. Per-asset oracle  (set via set_asset_oracle)
//!   │      Any contract implementing get_price(asset: Address) -> i128.
//!   │      Admin sets this for assets that need a custom feed
//!   │      (e.g. not listed on Reflector, or requiring a dedicated adapter).
//!   │      → try_invoke_contract: reverts are caught, 0 treated as unavailable.
//!   │
//!   ├─ 2. Primary global oracle  (e.g. Reflector adapter)
//!   │      Used when no per-asset oracle price is available.
//!   │      → try_invoke_contract: reverts are caught, 0 treated as unavailable.
//!   │
//!   ├─ 3. Fallback global oracle  (e.g. DIA adapter)
//!   │      Used when primary reverts or returns 0.
//!   │      → invoke_contract; if fallback also fails the tx reverts.
//!   │
//!   └─ 4. Panic PriceNotAvailable
//! ```
//!
//! ## Oracle adapter interface
//! Every oracle (per-asset, primary, or fallback) must expose:
//! ```text
//! get_price(asset: Address) -> i128   // PRICE_PRECISION-scaled; 0 = unavailable
//! ```

#![no_std]

mod error;
mod storage;

pub use error::AssetHandlerError;

use soroban_sdk::{
    contract, contractimpl, panic_with_error, Address, Env, IntoVal, Map, Symbol, Val, Vec,
};

use storage::{
    clear_pending_admin, get_admin, get_asset_oracle, get_fallback_oracle, get_pending_admin,
    get_primary_oracle, get_registered_assets, is_initialized, remove_asset_oracle, set_admin,
    set_asset_oracle, set_fallback_oracle, set_initialized, set_pending_admin, set_primary_oracle,
    set_registered_assets, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
};

/// Fixed-point precision: 7 decimal places matching Stellar native precision.
pub const PRICE_PRECISION: i128 = 10_000_000;

#[contract]
pub struct AssetHandler;

#[contractimpl]
impl AssetHandler {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    pub fn __constructor(env: Env, admin: Address) {
        if is_initialized(&env) {
            panic_with_error!(&env, AssetHandlerError::AlreadyInitialized);
        }
        admin.require_auth();
        set_admin(&env, &admin);
        set_initialized(&env);
    }

    // -----------------------------------------------------------------------
    // Asset management — admin only
    // -----------------------------------------------------------------------

    /// Register an asset. Pricing comes from the per-asset oracle (if set via
    /// `set_asset_oracle`) or the global primary/fallback oracles.
    ///
    /// # Errors
    /// * [`AssetHandlerError::NotAdmin`]
    /// * [`AssetHandlerError::AssetAlreadyRegistered`]
    pub fn add_asset(env: Env, caller: Address, asset: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, AssetHandlerError::NotAdmin);
        }
        let assets = get_registered_assets(&env);
        for a in assets.iter() {
            if a == asset {
                panic_with_error!(&env, AssetHandlerError::AssetAlreadyRegistered);
            }
        }
        let mut assets = get_registered_assets(&env);
        assets.push_back(asset);
        set_registered_assets(&env, &assets);
    }

    /// Remove an asset from the registry. Also clears its per-asset oracle if set.
    ///
    /// # Errors
    /// * [`AssetHandlerError::NotAdmin`]
    /// * [`AssetHandlerError::AssetNotRegistered`]
    pub fn remove_asset(env: Env, caller: Address, asset: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, AssetHandlerError::NotAdmin);
        }
        let assets = get_registered_assets(&env);
        let mut found = false;
        let mut updated: Vec<Address> = Vec::new(&env);
        for a in assets.iter() {
            if a == asset {
                found = true;
            } else {
                updated.push_back(a);
            }
        }
        if !found {
            panic_with_error!(&env, AssetHandlerError::AssetNotRegistered);
        }
        remove_asset_oracle(&env, &asset);
        set_registered_assets(&env, &updated);
    }

    // -----------------------------------------------------------------------
    // Per-asset oracle management — admin only
    // -----------------------------------------------------------------------

    /// Assign a dedicated oracle to `asset`.
    ///
    /// The oracle contract must implement `get_price(asset: Address) -> i128`.
    /// Use this for assets not covered by the global oracles or that need a
    /// specialised price feed.
    ///
    /// # Errors
    /// * [`AssetHandlerError::NotAdmin`]
    /// * [`AssetHandlerError::AssetNotRegistered`]
    pub fn set_asset_oracle(env: Env, caller: Address, asset: Address, oracle: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, AssetHandlerError::NotAdmin);
        }
        if !Self::asset_is_registered(&env, &asset) {
            panic_with_error!(&env, AssetHandlerError::AssetNotRegistered);
        }
        set_asset_oracle(&env, &asset, &oracle);
    }

    /// Remove the per-asset oracle for `asset` so it falls back to global oracles.
    ///
    /// # Errors
    /// * [`AssetHandlerError::NotAdmin`]
    pub fn remove_asset_oracle(env: Env, caller: Address, asset: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, AssetHandlerError::NotAdmin);
        }
        remove_asset_oracle(&env, &asset);
    }

    /// Return the per-asset oracle address, or `None` if not set.
    pub fn get_asset_oracle(env: Env, asset: Address) -> Option<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_asset_oracle(&env, &asset)
    }

    // -----------------------------------------------------------------------
    // Pricing — three-tier oracle with graceful fallback
    // -----------------------------------------------------------------------

    /// Return the `PRICE_PRECISION`-scaled price of `asset`.
    ///
    /// Resolution: per-asset oracle → primary (Reflector) → fallback (DIA).
    ///
    /// # Errors
    /// * [`AssetHandlerError::AssetNotRegistered`]
    /// * [`AssetHandlerError::NoPrimaryOracle`] — primary not configured and no per-asset oracle
    /// * [`AssetHandlerError::PriceNotAvailable`] — all oracles failed or returned 0
    pub fn get_price(env: Env, asset: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if !Self::asset_is_registered(&env, &asset) {
            panic_with_error!(&env, AssetHandlerError::AssetNotRegistered);
        }

        let get_price_sym = Symbol::new(&env, "get_price");
        let args: Vec<Val> = (asset.clone(),).into_val(&env);

        // --- Tier 1: per-asset oracle (if set) ---
        if let Some(oracle) = get_asset_oracle(&env, &asset) {
            if let Ok(Ok(price)) = env.try_invoke_contract::<i128, AssetHandlerError>(
                &oracle,
                &get_price_sym,
                args.clone(),
            ) {
                if price > 0 {
                    return price;
                }
            }
            // Per-asset oracle reverted or returned 0 — fall through to globals.
        }

        // --- Tier 2: primary global oracle (Reflector) ---
        let primary = get_primary_oracle(&env)
            .unwrap_or_else(|| panic_with_error!(&env, AssetHandlerError::NoPrimaryOracle));

        if let Ok(Ok(price)) = env.try_invoke_contract::<i128, AssetHandlerError>(
            &primary,
            &get_price_sym,
            args.clone(),
        ) {
            if price > 0 {
                return price;
            }
        }
        // Primary reverted or returned 0 — try fallback.

        // --- Tier 3: fallback global oracle (DIA) ---
        let fallback = get_fallback_oracle(&env)
            .unwrap_or_else(|| panic_with_error!(&env, AssetHandlerError::PriceNotAvailable));

        let price: i128 = env.invoke_contract(&fallback, &get_price_sym, args);
        if price <= 0 {
            panic_with_error!(&env, AssetHandlerError::PriceNotAvailable);
        }
        price
    }

    /// Batch price lookup — returns a `Map<asset, price>` for every requested asset.
    ///
    /// Each asset goes through the same three-tier resolution as `get_price`.
    /// Panics if any asset is unregistered or has no available price.
    pub fn get_prices(env: Env, assets: Vec<Address>) -> Map<Address, i128> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let mut result = Map::new(&env);
        for asset in assets.iter() {
            let price = Self::get_price(env.clone(), asset.clone());
            result.set(asset, price);
        }
        result
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    pub fn is_registered(env: Env, asset: Address) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Self::asset_is_registered(&env, &asset)
    }

    pub fn get_all_assets(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_registered_assets(&env)
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_admin(&env)
    }

    // -----------------------------------------------------------------------
    // Global oracle management — admin only
    // -----------------------------------------------------------------------

    /// Set the primary global oracle (e.g. a Reflector adapter).
    /// Must implement `get_price(asset: Address) -> i128`.
    pub fn set_primary_oracle(env: Env, caller: Address, oracle: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, AssetHandlerError::NotAdmin);
        }
        set_primary_oracle(&env, &oracle);
    }

    /// Set the fallback global oracle (e.g. a DIA adapter).
    /// Used when the primary oracle reverts or returns 0.
    pub fn set_fallback_oracle(env: Env, caller: Address, oracle: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, AssetHandlerError::NotAdmin);
        }
        set_fallback_oracle(&env, &oracle);
    }

    pub fn get_primary_oracle(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_primary_oracle(&env)
    }

    pub fn get_fallback_oracle(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_fallback_oracle(&env)
    }

    // -----------------------------------------------------------------------
    // Admin transfer (two-step)
    // -----------------------------------------------------------------------

    pub fn set_pending_admin(env: Env, caller: Address, new_admin: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, AssetHandlerError::NotAdmin);
        }
        set_pending_admin(&env, &new_admin);
    }

    pub fn accept_admin(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        let pending = get_pending_admin(&env)
            .unwrap_or_else(|| panic_with_error!(&env, AssetHandlerError::NoPendingAdmin));
        if caller != pending {
            panic_with_error!(&env, AssetHandlerError::NotAdmin);
        }
        set_admin(&env, &pending);
        clear_pending_admin(&env);
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn asset_is_registered(env: &Env, asset: &Address) -> bool {
        let assets = get_registered_assets(env);
        for a in assets.iter() {
            if &a == asset {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod test;
