//! # Oracle — Mock Price Feed for the Asset-Management Protocol
//!
//! This contract provides asset prices used by the vault to calculate the
//! total portfolio value and per-share price.
//!
//! ## Precision
//! All prices are expressed as `i128` values scaled by
//! [`PRICE_PRECISION`] (`10_000_000`, i.e. 7 decimal places matching
//! Stellar's native precision).
//!
//! * `get_price(XLM) == 10_000_000`  → 1 XLM = 1.0 base unit
//! * `get_price(USDC) == 10_000_000` → 1 USDC = 1.0 base unit
//! * `get_price(BTC)  == 650_000_000_000` → 1 BTC = 65 000 base units
//!
//! ## Production upgrade path
//! Replace the admin-controlled `set_price` with calls to the
//! [Reflector oracle network](https://reflector.network/) or any other
//! on-chain price aggregator while keeping the same `get_price` interface.

#![no_std]

mod error;
mod storage;

pub use error::OracleError;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Map, Vec};

use storage::{
    get_admin, get_max_age_ledgers, get_price_data, has_admin, set_admin, set_max_age_ledgers,
    set_price, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
};

/// Prices are `PRICE_PRECISION`-scaled `i128` values.
/// One unit of the asset costs `price / PRICE_PRECISION` base-currency units.
pub const PRICE_PRECISION: i128 = 10_000_000;
/// Default maximum accepted oracle price age (~1.2 days at ~6s/ledger).
pub const DEFAULT_MAX_AGE_LEDGERS: u32 = 17_280;

// ---------------------------------------------------------------------------
// Contract struct
// ---------------------------------------------------------------------------

/// The Oracle contract.  All state lives in Soroban host storage.
#[contract]
pub struct OracleContract;

// ---------------------------------------------------------------------------
// Implementation
// ---------------------------------------------------------------------------

#[contractimpl]
impl OracleContract {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Constructor — runs atomically with `CreateContract`, cannot be front-run.
    ///
    /// The deployer supplies the initial `admin` address. Because this executes
    /// in the same transaction as contract creation, no attacker can race ahead
    /// and claim admin rights.
    ///
    /// # Arguments
    /// * `admin` – The address that will be permitted to call [`set_price`].
    pub fn __constructor(env: Env, admin: Address) {
        if has_admin(&env) {
            panic_with_error!(&env, OracleError::AlreadyInitialized);
        }
        set_admin(&env, &admin);
        set_max_age_ledgers(&env, DEFAULT_MAX_AGE_LEDGERS);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // -----------------------------------------------------------------------
    // Admin
    // -----------------------------------------------------------------------

    /// Transfer admin rights to `new_admin`.
    ///
    /// # Auth
    /// Current admin must authorize this call.
    ///
    /// # Errors
    /// * [`OracleError::NotInitialized`] if the contract has not been set up.
    pub fn set_admin(env: Env, new_admin: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let admin = get_admin(&env);
        admin.require_auth();
        set_admin(&env, &new_admin);
    }

    /// Set the maximum allowed age of stored prices in ledgers.
    ///
    /// `max_age_ledgers` must be ≥ 1; zero would permanently disable the
    /// staleness guard, accepting arbitrarily old prices.
    ///
    /// # Errors
    /// * [`OracleError::InvalidMaxAge`] if `max_age_ledgers == 0`.
    pub fn set_max_age_ledgers(env: Env, max_age_ledgers: u32) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        if max_age_ledgers == 0 {
            panic_with_error!(&env, OracleError::InvalidMaxAge);
        }
        let admin = get_admin(&env);
        admin.require_auth();
        set_max_age_ledgers(&env, max_age_ledgers);
    }

    /// Return the configured maximum allowed price age in ledgers.
    pub fn get_max_age_ledgers(env: Env) -> u32 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_max_age_ledgers(&env)
    }

    /// Return the current admin address.
    ///
    /// # Errors
    /// * [`OracleError::NotInitialized`] if the contract has not been set up.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_admin(&env)
    }

    // -----------------------------------------------------------------------
    // Price management
    // -----------------------------------------------------------------------

    /// Set (or update) the price of `asset`.
    ///
    /// The price must be expressed in [`PRICE_PRECISION`]-scaled units.
    /// To set "1 USDC = 1.0 base unit" pass `price = 10_000_000`.
    ///
    /// # Arguments
    /// * `asset` – The SEP-41 token address whose price is being set.
    /// * `price` – PRECISION-scaled price. Must be > 0.
    ///
    /// # Auth
    /// Admin must authorize this call.
    ///
    /// # Errors
    /// * [`OracleError::NotInitialized`]   if the contract has not been set up.
    /// * [`OracleError::NotAuthorized`]    if caller is not the admin.
    /// * [`OracleError::NonPositivePrice`] if `price <= 0`.
    pub fn set_price(env: Env, asset: Address, price: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if price <= 0 {
            panic_with_error!(&env, OracleError::NonPositivePrice);
        }

        let admin = get_admin(&env);
        admin.require_auth();

        set_price(&env, &asset, price, env.ledger().sequence());
    }

    /// Return the [`PRICE_PRECISION`]-scaled price of `asset`.
    ///
    /// # Arguments
    /// * `asset` – The SEP-41 token address to query.
    ///
    /// # Errors
    /// * [`OracleError::NotInitialized`] if the contract has not been set up.
    /// * [`OracleError::PriceNotFound`]  if no price has been set for `asset`.
    pub fn get_price(env: Env, asset: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Self::load_fresh_price(&env, &asset)
    }

    /// Return a map of prices for all requested assets in a single call.
    ///
    /// This is a convenience batch-read that avoids multiple round-trips.
    ///
    /// # Arguments
    /// * `assets` – List of SEP-41 token addresses to query.
    ///
    /// # Errors
    /// * [`OracleError::PriceNotFound`] if any asset in the list has no price.
    pub fn get_prices(env: Env, assets: Vec<Address>) -> Map<Address, i128> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        let mut result: Map<Address, i128> = Map::new(&env);
        for asset in assets.iter() {
            let price = Self::load_fresh_price(&env, &asset);
            result.set(asset, price);
        }
        result
    }

    fn load_fresh_price(env: &Env, asset: &Address) -> i128 {
        let data = get_price_data(env, asset)
            .unwrap_or_else(|| panic_with_error!(env, OracleError::PriceNotFound));
        let max_age = get_max_age_ledgers(env);
        if max_age > 0 {
            let now = env.ledger().sequence();
            let age = now.saturating_sub(data.updated_ledger);
            if age > max_age {
                panic_with_error!(env, OracleError::StalePrice);
            }
        }
        data.price
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
