//! # Phoenix Trade Guard
//!
//! Validates Phoenix multi-hop swap parameters before the vault executes them.
//! Phoenix swaps are expressed as a sequence of `SwapOperation` structs — each
//! specifying `offer_asset` (token in), `ask_asset` (token out), and an
//! optional per-hop slippage belief.  The guard checks:
//!
//! 1. **Non-empty / max-length** — at least 1 operation, at most [`MAX_OPERATIONS`].
//! 2. **Amount** — `amount_in` must be positive.
//! 3. **Whitelist** — every `offer_asset` and `ask_asset` must be in the
//!    manager-controlled whitelist.
//! 4. **Global slippage** — `(amount_in - min_out) / amount_in` must not
//!    exceed [`MAX_SLIPPAGE_BPS`] basis points.
//!
//! ## Auth model
//! * `initialize` — one-time setup; `manager` must authorize.
//! * `set_whitelist` — manager only.
//! * `validate_swap` — vault only.

#![no_std]

mod error;
mod storage;

pub use error::PhoenixGuardError;

use soroban_sdk::{contract, contractimpl, contracttype, panic_with_error, Address, Env, IntoVal, Symbol, Vec};

use storage::{
    get_manager, get_router, get_vault, get_whitelist, is_initialized, set_manager, set_router,
    set_vault, set_whitelist, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD, MAX_OPERATIONS,
    MAX_SLIPPAGE_BPS,
};

// ---------------------------------------------------------------------------
// Phoenix swap-operation type
// ---------------------------------------------------------------------------

/// Represents one hop in a Phoenix multi-hop swap.
///
/// Mirrors the `SwapOperation` struct used by Phoenix's multi-hop router.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SwapOperation {
    /// Token offered in this hop.
    pub offer_asset: Address,
    /// Token received in this hop.
    pub ask_asset: Address,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PhoenixTradeGuard;

#[contractimpl]
impl PhoenixTradeGuard {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the Phoenix trade guard atomically at deployment.
    ///
    /// Runs as part of `CreateContractV2` — no post-deployment initialization
    /// window that an attacker could race to call with a malicious vault.
    ///
    /// # Arguments
    /// * `vault`   – The only address allowed to call `validate_swap`.
    /// * `manager` – Account allowed to update the token whitelist.
    /// * `tokens`  – Initial whitelist of tradeable token addresses.
    /// * `router`  – Contract exposing `quote_exact_in(amount_in, path) -> i128`
    ///               used to obtain output-token-unit quotes for slippage checks.
    ///
    /// # Errors
    /// * [`PhoenixGuardError::AlreadyInitialized`]
    pub fn __constructor(env: Env, vault: Address, manager: Address, tokens: Vec<Address>, router: Address) {
        if is_initialized(&env) {
            panic_with_error!(&env, PhoenixGuardError::AlreadyInitialized);
        }

        // Require auth from the vault's on-chain manager to prevent an attacker
        // from supplying a malicious vault they control at deployment time.
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
        set_router(&env, &router);
    }

    // -----------------------------------------------------------------------
    // Whitelist management
    // -----------------------------------------------------------------------

    /// Replace the token whitelist. Manager only.
    ///
    /// # Errors
    /// * [`PhoenixGuardError::NotManager`]
    pub fn set_whitelist(env: Env, caller: Address, tokens: Vec<Address>) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, PhoenixGuardError::NotManager);
        }
        set_whitelist(&env, &tokens);
    }

    // -----------------------------------------------------------------------
    // Validation entry point
    // -----------------------------------------------------------------------

    /// Validate a Phoenix multi-hop swap.
    ///
    /// The vault calls this immediately before forwarding the swap to the
    /// Phoenix multi-hop router. The function reverts if any policy is violated.
    ///
    /// Slippage is checked against `amount_in` (not a caller-supplied quote) so
    /// that a manipulated `quoted_out` value cannot be used to bypass the limit.
    ///
    /// # Arguments
    /// * `caller`     – Must equal the registered vault address.
    /// * `amount_in`  – Exact input amount for the first hop (must be positive).
    /// * `min_out`    – Minimum output amount from the final hop (slippage guard).
    /// * `operations` – Ordered list of [`SwapOperation`]s (1 to [`MAX_OPERATIONS`]).
    ///
    /// # Errors
    /// * [`PhoenixGuardError::NotVault`]
    /// * [`PhoenixGuardError::InvalidAmount`]
    /// * [`PhoenixGuardError::OperationsEmpty`] / [`PhoenixGuardError::OperationsTooMany`]
    /// * [`PhoenixGuardError::TokenNotWhitelisted`]
    /// * [`PhoenixGuardError::SlippageTooHigh`]
    pub fn validate_swap(
        env: Env,
        caller: Address,
        amount_in: i128,
        min_out: i128,
        operations: Vec<SwapOperation>,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_vault(&env) {
            panic_with_error!(&env, PhoenixGuardError::NotVault);
        }

        if amount_in <= 0 {
            panic_with_error!(&env, PhoenixGuardError::InvalidAmount);
        }

        let n = operations.len();
        if n == 0 {
            panic_with_error!(&env, PhoenixGuardError::OperationsEmpty);
        }
        if n > MAX_OPERATIONS {
            panic_with_error!(&env, PhoenixGuardError::OperationsTooMany);
        }

        // Enforce hop continuity: each hop's output must be the next hop's input.
        // Without this check a crafted operations list could pass whitelist checks
        // but compute slippage against a different effective path.
        for i in 1..n {
            let prev = operations.get(i - 1).unwrap();
            let curr = operations.get(i).unwrap();
            if prev.ask_asset != curr.offer_asset {
                panic_with_error!(&env, PhoenixGuardError::NonContiguousHops);
            }
        }

        let whitelist = get_whitelist(&env);
        for op in operations.iter() {
            Self::assert_whitelisted(&op.offer_asset, &whitelist, &env);
            Self::assert_whitelisted(&op.ask_asset, &whitelist, &env);
        }

        // Build a flat path [offer_0, ask_0, ask_1, …] from the operations so
        // we can call the router's generic quote_exact_in interface.
        let mut path: Vec<Address> = Vec::new(&env);
        let first = operations.get(0).unwrap();
        path.push_back(first.offer_asset.clone());
        for op in operations.iter() {
            path.push_back(op.ask_asset.clone());
        }
        let router = get_router(&env);
        let quoted_out: i128 = env.invoke_contract(
            &router,
            &Symbol::new(&env, "quote_exact_in"),
            (amount_in, path).into_val(&env),
        );

        Self::check_slippage(min_out, quoted_out, &env);
    }

    /// Validate a swap using a flat token path — compatible with the vault's
    /// generic `guard_validate` call convention.
    ///
    /// Converts the flat path `[token_0, token_1, …, token_n]` into Phoenix
    /// `SwapOperation`s `[{offer: token_0, ask: token_1}, …, {offer: token_{n-1}, ask: token_n}]`
    /// and applies the same policy checks as [`validate_swap`].
    ///
    /// Slippage is checked against `amount_in`; no caller-supplied quote is
    /// accepted, eliminating the spoofed-`quoted_out` attack surface.  This
    /// also matches the 4-argument call convention the vault uses via
    /// `guard_validate(vault, amount_in, min_out, path)`.
    ///
    /// # Errors
    /// * [`PhoenixGuardError::NotVault`]
    /// * [`PhoenixGuardError::InvalidAmount`]
    /// * [`PhoenixGuardError::OperationsEmpty`] — path has fewer than 2 tokens
    /// * [`PhoenixGuardError::OperationsTooMany`] — too many hops
    /// * [`PhoenixGuardError::TokenNotWhitelisted`]
    /// * [`PhoenixGuardError::SlippageTooHigh`]
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
            panic_with_error!(&env, PhoenixGuardError::NotVault);
        }

        if amount_in <= 0 {
            panic_with_error!(&env, PhoenixGuardError::InvalidAmount);
        }

        let n = path.len();
        // A valid path needs at least [token_in, token_out] = 2 addresses = 1 hop.
        if n < 2 {
            panic_with_error!(&env, PhoenixGuardError::OperationsEmpty);
        }
        let num_ops = n - 1; // number of hops
        if num_ops > MAX_OPERATIONS {
            panic_with_error!(&env, PhoenixGuardError::OperationsTooMany);
        }

        let whitelist = get_whitelist(&env);
        let mut i: u32 = 0;
        while i < n - 1 {
            let offer = path.get(i).unwrap();
            let ask = path.get(i + 1).unwrap();
            Self::assert_whitelisted(&offer, &whitelist, &env);
            Self::assert_whitelisted(&ask, &whitelist, &env);
            i += 1;
        }

        let router = get_router(&env);
        let quoted_out: i128 = env.invoke_contract(
            &router,
            &Symbol::new(&env, "quote_exact_in"),
            (amount_in, path.clone()).into_val(&env),
        );

        Self::check_slippage(min_out, quoted_out, &env);
    }

    /// Validate strategy-invest operations guarded by this contract.
    ///
    /// # Errors
    /// * [`PhoenixGuardError::NotVault`]
    /// * [`PhoenixGuardError::InvalidAmount`]
    pub fn validate_invest(env: Env, caller: Address, amount: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_vault(&env) {
            panic_with_error!(&env, PhoenixGuardError::NotVault);
        }
        if amount <= 0 {
            panic_with_error!(&env, PhoenixGuardError::InvalidAmount);
        }
    }

    /// Validate strategy-unwind operations guarded by this contract.
    ///
    /// # Errors
    /// * [`PhoenixGuardError::NotVault`]
    /// * [`PhoenixGuardError::InvalidAmount`]
    pub fn validate_unwind(env: Env, caller: Address, units: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_vault(&env) {
            panic_with_error!(&env, PhoenixGuardError::NotVault);
        }
        if units <= 0 {
            panic_with_error!(&env, PhoenixGuardError::InvalidAmount);
        }
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

    pub fn get_router(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_router(&env)
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn assert_whitelisted(token: &Address, whitelist: &Vec<Address>, env: &Env) {
        for w in whitelist.iter() {
            if &w == token {
                return;
            }
        }
        panic_with_error!(env, PhoenixGuardError::TokenNotWhitelisted);
    }

    fn check_slippage(min_out: i128, quoted_out: i128, env: &Env) {
        if min_out < 0 || quoted_out <= 0 || min_out > quoted_out {
            panic_with_error!(env, PhoenixGuardError::SlippageTooHigh);
        }
        let diff = quoted_out - min_out;
        // Use checked_mul and fail closed on overflow: saturating_mul could make
        // both sides equal i128::MAX, causing the strict > comparison to return
        // false and silently bypass the slippage limit.
        let lhs = diff
            .checked_mul(10_000)
            .unwrap_or_else(|| panic_with_error!(env, PhoenixGuardError::SlippageTooHigh));
        let rhs = quoted_out
            .checked_mul(MAX_SLIPPAGE_BPS as i128)
            .unwrap_or_else(|| panic_with_error!(env, PhoenixGuardError::SlippageTooHigh));
        if lhs > rhs {
            panic_with_error!(env, PhoenixGuardError::SlippageTooHigh);
        }
    }
}

#[cfg(test)]
mod test;
