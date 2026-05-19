//! # DIAAdapter
//!
//! Adapter contract that wraps the [DIA](https://diadata.org) decentralized
//! oracle and exposes the unified `get_price(asset) -> i128` interface expected
//! by `AssetHandler`.
//!
//! ## How it works
//!
//! ```text
//! AssetHandler::get_price(asset)
//!   └─ DIAAdapter::get_price(asset)
//!        ├─ lookup asset → DIA key (e.g. "BTC/USD")
//!        │    └─ None → return 0 (no mapping configured)
//!        └─ DIA::read_oracle_value(key) -> OracleValue
//!             ├─ Ok(OracleValue { price, .. }) → normalize to PRICE_PRECISION
//!             └─ Err / revert               → return 0 (unavailable)
//! ```
//!
//! ## Asset key mapping
//!
//! DIA identifies assets by string pair key (e.g. `"BTC/USD"`, `"XLM/USD"`).
//! The admin must register each asset address → DIA key before `get_price` can
//! return a non-zero value for that asset:
//!
//! ```text
//! dia_adapter.set_asset_key(admin, usdc_addr, "USDC/USD")
//! dia_adapter.set_asset_key(admin, xlm_addr,  "XLM/USD")
//! dia_adapter.set_asset_key(admin, btc_addr,  "BTC/USD")
//! ```
//!
//! ## Price normalization
//!
//! DIA encodes prices with 8 fixed decimal places.
//! This adapter converts to `PRICE_PRECISION` (7 decimal places):
//!
//! ```text
//! normalized = dia_price * PRICE_PRECISION / 10^8
//!            = dia_price * 10_000_000 / 100_000_000
//!            = dia_price / 10
//! ```

#![no_std]

mod error;
mod storage;

pub use error::DiaAdapterError;

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, Address, Env, IntoVal, String, Symbol,
    Val, Vec,
};

use storage::{
    clear_pending_admin, get_admin, get_asset_key, get_dia_contract, get_max_age_secs,
    get_pending_admin, is_initialized, remove_asset_key, set_admin, set_asset_key,
    set_dia_contract, set_max_age_secs, set_pending_admin, DataKey,
    INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
};

/// Protocol price precision: 7 decimal places (Stellar native).
pub const PRICE_PRECISION: i128 = 10_000_000;

/// DIA oracle always uses 8 fixed decimal places.
const DIA_DECIMALS: i128 = 100_000_000; // 10^8
/// Default freshness window for upstream DIA prices.
const DEFAULT_MAX_AGE_SECS: u64 = 3_600;

// ---------------------------------------------------------------------------
// Local mirror of DIA's OracleValue struct
// ---------------------------------------------------------------------------

/// Mirror of DIA's `OracleValue` return type from `read_oracle_value`.
#[contracttype]
#[derive(Clone)]
pub struct OracleValue {
    /// Price with 8 decimal places (e.g. 6_500_000_000_000 = $65,000).
    pub price: i128,
    /// Timestamp of the last price update.
    pub timestamp: u64,
}

#[contract]
pub struct DiaAdapter;

#[contractimpl]
impl DiaAdapter {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    pub fn __constructor(env: Env, admin: Address, dia_contract: Address) {
        if is_initialized(&env) {
            panic_with_error!(&env, DiaAdapterError::AlreadyInitialized);
        }
        admin.require_auth();

        // Write directly to avoid extend_ttl on brand-new persistent entries,
        // which fails in a constructor (entry not yet in ledger footprint).
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::DiaContract, &dia_contract);
        env.storage().persistent().set(&DataKey::MaxAgeSecs, &DEFAULT_MAX_AGE_SECS);
        env.storage().persistent().set(&DataKey::Initialized, &true);
    }

    // -----------------------------------------------------------------------
    // Core interface — called by AssetHandler
    // -----------------------------------------------------------------------

    /// Return the `PRICE_PRECISION`-scaled price of `asset`.
    ///
    /// Returns `0` when:
    /// - no DIA key is registered for `asset`
    /// - the DIA contract reverts or returns a zero/negative price
    pub fn get_price(env: Env, asset: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        // Step 1: resolve asset address → DIA query key.
        let key = match get_asset_key(&env, &asset) {
            Some(k) => k,
            None => return 0,
        };

        // Step 2: call DIA.
        let dia = get_dia_contract(&env);
        let args: Vec<Val> = (key,).into_val(&env);

        let result = env.try_invoke_contract::<OracleValue, DiaAdapterError>(
            &dia,
            &Symbol::new(&env, "read_oracle_value"),
            args,
        );

        // Step 3: normalize.
        match result {
            Ok(Ok(val)) if val.price > 0 && Self::is_fresh(&env, val.timestamp) => val
                .price
                .checked_mul(PRICE_PRECISION)
                .and_then(|p| p.checked_div(DIA_DECIMALS))
                .unwrap_or(0),
            _ => 0,
        }
    }

    // -----------------------------------------------------------------------
    // Admin — asset key management
    // -----------------------------------------------------------------------

    /// Register a DIA query key for `asset`. Admin only.
    ///
    /// `key` must match DIA's pair notation, e.g. `"BTC/USD"`, `"XLM/USD"`.
    pub fn set_asset_key(env: Env, caller: Address, asset: Address, key: String) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, DiaAdapterError::NotAdmin);
        }
        set_asset_key(&env, &asset, &key);
    }

    /// Remove the DIA key mapping for `asset`. Admin only.
    pub fn remove_asset_key(env: Env, caller: Address, asset: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, DiaAdapterError::NotAdmin);
        }
        remove_asset_key(&env, &asset);
    }

    /// Return the DIA key registered for `asset`, or `None` if not set.
    pub fn get_asset_key(env: Env, asset: Address) -> Option<String> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_asset_key(&env, &asset)
    }

    // -----------------------------------------------------------------------
    // Admin — DIA contract management
    // -----------------------------------------------------------------------

    /// Update the DIA contract address. Admin only.
    pub fn set_dia_contract(env: Env, caller: Address, dia: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, DiaAdapterError::NotAdmin);
        }
        set_dia_contract(&env, &dia);
    }

    /// Set the maximum accepted upstream DIA price age in seconds. Admin only.
    pub fn set_max_age_secs(env: Env, caller: Address, secs: u64) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, DiaAdapterError::NotAdmin);
        }
        if secs == 0 {
            panic_with_error!(&env, DiaAdapterError::InvalidMaxAge);
        }
        set_max_age_secs(&env, secs);
    }

    pub fn get_max_age_secs(env: Env) -> u64 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_max_age_secs(&env)
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
            panic_with_error!(&env, DiaAdapterError::NotAdmin);
        }
        set_pending_admin(&env, &new_admin);
    }

    pub fn accept_admin(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        let pending = get_pending_admin(&env)
            .unwrap_or_else(|| panic_with_error!(&env, DiaAdapterError::NoPendingAdmin));
        if caller != pending {
            panic_with_error!(&env, DiaAdapterError::NotAdmin);
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

    pub fn get_dia_contract(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_dia_contract(&env)
    }

    fn is_fresh(env: &Env, timestamp: u64) -> bool {
        let now = env.ledger().timestamp();
        timestamp <= now && now.saturating_sub(timestamp) <= get_max_age_secs(env)
    }
}

#[cfg(test)]
mod test;
