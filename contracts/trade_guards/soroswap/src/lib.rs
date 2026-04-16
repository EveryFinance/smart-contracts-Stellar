//! # Soroswap Trade Guard
//!
//! A stateful on-chain policy contract that validates Soroswap swap parameters
//! before a vault executes them. The vault calls one of the two `validate_*`
//! functions; this contract reverts if any policy rule is violated.
//!
//! ## Rules enforced
//! 1. Every token in the swap path must be in the **token whitelist**.
//! 2. The swap path length must be between 2 and [`MAX_PATH_LEN`] (inclusive).
//! 3. The amount-in must be positive.
//! 4. For exact-in swaps, implied slippage `(quoted_out - min_out) / quoted_out`
//!    must not exceed [`MAX_SLIPPAGE_BPS`] basis points.
//! 5. For exact-out swaps, input headroom `(max_in - quoted_in) / quoted_in`
//!    must not exceed [`MAX_SLIPPAGE_BPS`] basis points.
//!
//! ## Auth model
//! * `initialize` — one-time setup; `manager` must authorize.
//! * `set_whitelist` — manager only.
//! * `validate_*` — vault only (read-only except for auth check).

#![no_std]

mod error;
mod storage;

pub use error::SoroswapGuardError;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, Vec};

use storage::{
    get_manager, get_vault, get_whitelist, is_initialized, set_manager, set_vault, set_whitelist,
    INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD, MAX_PATH_LEN, MAX_SLIPPAGE_BPS,
};

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct SoroswapTradeGuard;

#[contractimpl]
impl SoroswapTradeGuard {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the trade guard.
    ///
    /// # Arguments
    /// * `vault`   – The only address allowed to call `validate_*`.
    /// * `manager` – Account allowed to update the token whitelist.
    /// * `tokens`  – Initial whitelist of tradeable token addresses.
    ///
    /// # Errors
    /// * [`SoroswapGuardError::AlreadyInitialized`]
    pub fn initialize(env: Env, vault: Address, manager: Address, tokens: Vec<Address>) {
        if is_initialized(&env) {
            panic_with_error!(&env, SoroswapGuardError::AlreadyInitialized);
        }
        manager.require_auth();
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        set_vault(&env, &vault);
        set_manager(&env, &manager);
        set_whitelist(&env, &tokens);
    }

    // -----------------------------------------------------------------------
    // Whitelist management
    // -----------------------------------------------------------------------

    /// Replace the token whitelist. Manager only.
    ///
    /// # Errors
    /// * [`SoroswapGuardError::NotManager`]
    pub fn set_whitelist(env: Env, caller: Address, tokens: Vec<Address>) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, SoroswapGuardError::NotManager);
        }
        set_whitelist(&env, &tokens);
    }

    // -----------------------------------------------------------------------
    // Validation entry points
    // -----------------------------------------------------------------------

    /// Validate a `swap_exact_tokens_for_tokens` call.
    ///
    /// Called by the vault before forwarding the swap to the Soroswap router.
    ///
    /// # Arguments
    /// * `caller`     – Must equal the registered vault address.
    /// * `amount_in`  – Exact input token amount (must be positive).
    /// * `min_out`    – Minimum output accepted (slippage guard).
    /// * `quoted_out` – Router quote for `amount_in` and `path`.
    /// * `path`       – Ordered list of token addresses [token_in, …, token_out].
    ///
    /// # Errors
    /// * [`SoroswapGuardError::NotVault`]
    /// * [`SoroswapGuardError::InvalidAmount`]
    /// * [`SoroswapGuardError::PathTooShort`] / [`SoroswapGuardError::PathTooLong`]
    /// * [`SoroswapGuardError::TokenNotWhitelisted`]
    /// * [`SoroswapGuardError::SlippageTooHigh`]
    pub fn validate_swap_exact_in(
        env: Env,
        caller: Address,
        amount_in: i128,
        min_out: i128,
        path: Vec<Address>,
        quoted_out: i128,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_vault(&env) {
            panic_with_error!(&env, SoroswapGuardError::NotVault);
        }

        Self::check_amount(amount_in, &env);
        Self::check_path(&path, &env);
        Self::check_whitelist(&path, &env);
        Self::check_slippage(min_out, quoted_out, &env);
    }

    /// Validate a `swap_tokens_for_exact_tokens` call.
    ///
    /// Called by the vault before forwarding the swap to the Soroswap router.
    ///
    /// # Arguments
    /// * `caller`      – Must equal the registered vault address.
    /// * `amount_out`  – Exact output token amount desired (must be positive).
    /// * `max_in`      – Maximum input token amount willing to spend.
    /// * `path`        – Ordered list of token addresses [token_in, …, token_out].
    /// * `quoted_in`   – Router quote for required input to get `amount_out`.
    ///
    /// # Errors
    /// Same set as [`validate_swap_exact_in`].
    pub fn validate_swap_exact_out(
        env: Env,
        caller: Address,
        amount_out: i128,
        max_in: i128,
        path: Vec<Address>,
        quoted_in: i128,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_vault(&env) {
            panic_with_error!(&env, SoroswapGuardError::NotVault);
        }

        Self::check_amount(amount_out, &env);
        Self::check_amount(max_in, &env);
        Self::check_path(&path, &env);
        Self::check_whitelist(&path, &env);
        Self::check_exact_out_slippage(max_in, quoted_in, &env);
    }

    /// Validate strategy-invest operations guarded by this contract.
    ///
    /// # Errors
    /// * [`SoroswapGuardError::NotVault`]
    /// * [`SoroswapGuardError::InvalidAmount`]
    pub fn validate_invest(env: Env, caller: Address, amount: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_vault(&env) {
            panic_with_error!(&env, SoroswapGuardError::NotVault);
        }
        Self::check_amount(amount, &env);
    }

    /// Validate strategy-unwind operations guarded by this contract.
    ///
    /// # Errors
    /// * [`SoroswapGuardError::NotVault`]
    /// * [`SoroswapGuardError::InvalidAmount`]
    pub fn validate_unwind(env: Env, caller: Address, units: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_vault(&env) {
            panic_with_error!(&env, SoroswapGuardError::NotVault);
        }
        Self::check_amount(units, &env);
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// Return the current token whitelist.
    pub fn get_whitelist(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_whitelist(&env)
    }

    pub fn get_vault(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_vault(&env)
    }

    pub fn get_manager(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_manager(&env)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn check_amount(amount: i128, env: &Env) {
        if amount <= 0 {
            panic_with_error!(env, SoroswapGuardError::InvalidAmount);
        }
    }

    fn check_path(path: &Vec<Address>, env: &Env) {
        let len = path.len();
        if len < 2 {
            panic_with_error!(env, SoroswapGuardError::PathTooShort);
        }
        if len > MAX_PATH_LEN {
            panic_with_error!(env, SoroswapGuardError::PathTooLong);
        }
    }

    fn check_whitelist(path: &Vec<Address>, env: &Env) {
        let whitelist = get_whitelist(env);
        for token in path.iter() {
            let mut found = false;
            for w in whitelist.iter() {
                if w == token {
                    found = true;
                    break;
                }
            }
            if !found {
                panic_with_error!(env, SoroswapGuardError::TokenNotWhitelisted);
            }
        }
    }

    /// Reject if `(quoted_out - min_out) / quoted_out > MAX_SLIPPAGE_BPS / 10_000`.
    ///
    /// Rearranged to avoid floating point:
    /// `(quoted_out - min_out) * 10_000 > quoted_out * MAX_SLIPPAGE_BPS`
    fn check_slippage(min_out: i128, quoted_out: i128, env: &Env) {
        if min_out < 0 || quoted_out <= 0 || min_out > quoted_out {
            panic_with_error!(env, SoroswapGuardError::SlippageTooHigh);
        }
        let diff = quoted_out - min_out;
        // diff * 10_000 > quoted_out * MAX_SLIPPAGE_BPS  →  reject
        if diff.saturating_mul(10_000) > quoted_out.saturating_mul(MAX_SLIPPAGE_BPS as i128) {
            panic_with_error!(env, SoroswapGuardError::SlippageTooHigh);
        }
    }

    /// Reject if `(max_in - quoted_in) / quoted_in > MAX_SLIPPAGE_BPS / 10_000`.
    ///
    /// If `max_in <= quoted_in`, this is stricter than quote and is accepted.
    fn check_exact_out_slippage(max_in: i128, quoted_in: i128, env: &Env) {
        if quoted_in <= 0 {
            panic_with_error!(env, SoroswapGuardError::SlippageTooHigh);
        }
        if max_in <= quoted_in {
            return;
        }
        let diff = max_in - quoted_in;
        if diff.saturating_mul(10_000) > quoted_in.saturating_mul(MAX_SLIPPAGE_BPS as i128) {
            panic_with_error!(env, SoroswapGuardError::SlippageTooHigh);
        }
    }
}

#[cfg(test)]
mod test;
