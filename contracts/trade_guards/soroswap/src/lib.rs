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

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, IntoVal, Symbol, Vec};

use storage::{
    get_manager, get_strategy, get_vault, get_whitelist, is_initialized, set_manager, set_strategy,
    set_vault, set_whitelist, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD, MAX_PATH_LEN,
    MAX_SLIPPAGE_BPS,
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
    /// * `vault`    – The only address allowed to call `validate_*`.
    /// * `manager`  – Account allowed to update the token whitelist.
    /// * `tokens`   – Initial whitelist of tradeable token addresses.
    /// * `strategy` – Strategy contract whose `quote_exact_in` is called
    ///                on-chain to obtain a trusted price quote for slippage
    ///                validation (instead of accepting caller-supplied values).
    ///
    /// # Errors
    /// * [`SoroswapGuardError::AlreadyInitialized`]
    pub fn initialize(
        env: Env,
        vault: Address,
        manager: Address,
        tokens: Vec<Address>,
        strategy: Address,
    ) {
        if is_initialized(&env) {
            panic_with_error!(&env, SoroswapGuardError::AlreadyInitialized);
        }

        // Derive the vault's authoritative manager from on-chain state so an
        // attacker cannot front-run initialization by supplying their own vault.
        let vault_manager: Address = env.invoke_contract(
            &vault,
            &Symbol::new(&env, "get_manager"),
            ().into_val(&env),
        );
        vault_manager.require_auth();
        manager.require_auth();
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        set_vault(&env, &vault);
        set_manager(&env, &manager);
        set_whitelist(&env, &tokens);
        set_strategy(&env, &strategy);
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
    /// The slippage check uses an on-chain quote fetched directly from the
    /// stored strategy contract rather than accepting a caller-supplied value,
    /// preventing quote-spoofing attacks.
    ///
    /// # Arguments
    /// * `caller`    – Must equal the registered vault address.
    /// * `amount_in` – Exact input token amount (must be positive).
    /// * `min_out`   – Minimum output accepted (slippage guard).
    /// * `path`      – Ordered list of token addresses [token_in, …, token_out].
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

        // Fetch quote from the trusted strategy contract instead of trusting
        // the caller-supplied value.  This closes the quote-spoofing window.
        let strategy = get_strategy(&env);
        let quoted_out: i128 = env.invoke_contract(
            &strategy,
            &Symbol::new(&env, "quote_exact_in"),
            (amount_in, path.clone()).into_val(&env),
        );

        Self::check_slippage(min_out, quoted_out, &env);
    }

    /// Validate a `swap_tokens_for_exact_tokens` call.
    ///
    /// Called by the vault before forwarding the swap to the Soroswap router.
    /// Slippage is enforced by fetching the expected input cost (`quoted_in`)
    /// from the trusted strategy contract and checking:
    ///   `(max_in - quoted_in) / quoted_in <= MAX_SLIPPAGE_BPS / 10_000`
    ///
    /// # Arguments
    /// * `caller`     – Must equal the registered vault address.
    /// * `amount_out` – Exact output token amount desired (must be positive).
    /// * `max_in`     – Maximum input token amount willing to spend (must be
    ///                  positive).
    /// * `path`       – Ordered list of token addresses [token_in, …, token_out].
    ///
    /// # Errors
    /// * [`SoroswapGuardError::NotVault`]
    /// * [`SoroswapGuardError::InvalidAmount`]
    /// * [`SoroswapGuardError::PathTooShort`] / [`SoroswapGuardError::PathTooLong`]
    /// * [`SoroswapGuardError::TokenNotWhitelisted`]
    /// * [`SoroswapGuardError::SlippageTooHigh`]
    pub fn validate_swap_exact_out(
        env: Env,
        caller: Address,
        amount_out: i128,
        max_in: i128,
        path: Vec<Address>,
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

        // Fetch the expected input cost from the trusted strategy contract.
        // Using a caller-supplied quote would let an attacker bypass the limit
        // by passing quoted_in == max_in (0% apparent headroom).
        let strategy = get_strategy(&env);
        let quoted_in: i128 = env.invoke_contract(
            &strategy,
            &Symbol::new(&env, "quote_exact_out"),
            (amount_out, path.clone()).into_val(&env),
        );

        Self::check_slippage_out(max_in, quoted_in, &env);
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

    pub fn get_strategy(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_strategy(&env)
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
        // Reject if (quoted_out - min_out) * 10_000 > quoted_out * MAX_SLIPPAGE_BPS.
        // Use checked_mul and fail closed on overflow: an overflowing product means
        // the values are pathologically large; treat that as a policy violation.
        let lhs = diff
            .checked_mul(10_000)
            .unwrap_or_else(|| panic_with_error!(env, SoroswapGuardError::SlippageTooHigh));
        let rhs = quoted_out
            .checked_mul(MAX_SLIPPAGE_BPS as i128)
            .unwrap_or_else(|| panic_with_error!(env, SoroswapGuardError::SlippageTooHigh));
        if lhs > rhs {
            panic_with_error!(env, SoroswapGuardError::SlippageTooHigh);
        }
    }

    /// For exact-out swaps: reject if `(max_in - quoted_in) / quoted_in > MAX_SLIPPAGE_BPS / 10_000`.
    ///
    /// Rearranged: `(max_in - quoted_in) * 10_000 > quoted_in * MAX_SLIPPAGE_BPS`
    fn check_slippage_out(max_in: i128, quoted_in: i128, env: &Env) {
        if max_in <= 0 || quoted_in <= 0 || max_in < quoted_in {
            panic_with_error!(env, SoroswapGuardError::SlippageTooHigh);
        }
        let diff = max_in - quoted_in;
        let lhs = diff
            .checked_mul(10_000)
            .unwrap_or_else(|| panic_with_error!(env, SoroswapGuardError::SlippageTooHigh));
        let rhs = quoted_in
            .checked_mul(MAX_SLIPPAGE_BPS as i128)
            .unwrap_or_else(|| panic_with_error!(env, SoroswapGuardError::SlippageTooHigh));
        if lhs > rhs {
            panic_with_error!(env, SoroswapGuardError::SlippageTooHigh);
        }
    }

}

#[cfg(test)]
mod test;
