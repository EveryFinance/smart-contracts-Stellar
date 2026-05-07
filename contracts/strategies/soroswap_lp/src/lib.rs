//! # Soroswap LP Strategy — Automated Market-Maker Liquidity Position
//!
//! This strategy deposits two assets (`asset_a`, `asset_b`) into a Soroswap
//! AMM pair and holds the resulting LP tokens.  When the vault needs to
//! liquidate the position (during user withdrawal), the strategy calls
//! `router.remove_liquidity(…, to=user)` so that the underlying tokens go
//! **directly** from the DEX to the user without transiting the vault.
//!
//! ## Soroswap router interface used
//! ```text
//! add_liquidity(token_a, token_b, amount_a_desired, amount_b_desired,
//!               amount_a_min, amount_b_min, to, deadline) → (i128, i128, i128)
//! remove_liquidity(token_a, token_b, liquidity, amount_a_min, amount_b_min,
//!                  to, deadline) → (i128, i128)
//! ```
//!
//! ## Auth flow for deposit_liquidity
//! 1. Vault pre-authorises two `token.transfer(vault → strategy, amount)` calls
//!    via `authorize_as_current_contract`.
//! 2. Vault calls `strategy.deposit_liquidity(…, from=vault)`.
//! 3. Strategy pulls both tokens from vault.
//! 4. Strategy approves both tokens to the router.
//! 5. Strategy calls `router.add_liquidity(…, to=self)` — LP tokens arrive at
//!    strategy.
//!
//! ## Auth flow for withdraw
//! 1. Vault calls `strategy.withdraw(lp_amount, …, from=vault, to=user)`.
//! 2. Strategy approves LP token to router.
//! 3. Strategy calls `router.remove_liquidity(…, to=user)` — underlying
//!    tokens arrive directly at user.

#![no_std]
#![allow(clippy::too_many_arguments)]

mod error;
mod interfaces;
mod storage;

pub use error::SoroswapLpError;

use soroban_sdk::{contract, contractimpl, panic_with_error, token, Address, Env, IntoVal, String, Symbol};

use interfaces::{OracleAdapter, PairAdapter, RouterAdapter};

/// Fixed-point precision matching the vault and oracle (7 decimal places).
const PRICE_PRECISION: i128 = 10_000_000;

use storage::{
    get_asset_a, get_asset_b, get_lp_balance, get_lp_token, get_manager, get_name, get_oracle,
    get_paused, get_router, get_vault, is_initialized, set_asset_a, set_asset_b, set_lp_balance,
    set_lp_token, set_manager, set_name, set_oracle, set_paused, set_router, set_vault,
    INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
};

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct SoroswapLpStrategy;

#[allow(clippy::too_many_arguments)]
#[contractimpl]
impl SoroswapLpStrategy {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the strategy.
    ///
    /// # Arguments
    /// * `vault`    – Vault contract that owns this strategy.
    /// * `asset_a`  – First token of the pair (e.g. USDC).
    /// * `asset_b`  – Second token of the pair (e.g. XLM).
    /// * `lp_token` – The Soroswap pair LP token address.
    /// * `router`   – Soroswap router address.
    /// * `manager`  – Address allowed to pause / unpause.
    /// * `name`     – Human-readable label.
    ///
    /// # Auth
    /// Both the vault's current on-chain manager and the designated strategy
    /// `manager` must authorise this call (see Blend strategy doc for the
    /// front-running rationale).
    ///
    /// # Errors
    /// * [`SoroswapLpError::AlreadyInitialized`]
    #[allow(clippy::too_many_arguments)]
    pub fn initialize(
        env: Env,
        vault: Address,
        asset_a: Address,
        asset_b: Address,
        lp_token: Address,
        router: Address,
        manager: Address,
        name: String,
    ) {
        if is_initialized(&env) {
            panic_with_error!(&env, SoroswapLpError::AlreadyInitialized);
        }

        // Derive the vault's manager from on-chain state and require their
        // signature to prevent front-running initialization.
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
        set_asset_a(&env, &asset_a);
        set_asset_b(&env, &asset_b);
        set_lp_token(&env, &lp_token);
        set_router(&env, &router);
        set_manager(&env, &manager);
        set_name(&env, &name);
        set_paused(&env, false);
        set_lp_balance(&env, 0);
    }

    // -----------------------------------------------------------------------
    // Core operations
    // -----------------------------------------------------------------------

    /// Provide liquidity to the Soroswap pool.
    ///
    /// The vault must pre-authorise both token transfers via
    /// `authorize_as_current_contract` before calling this function.
    ///
    /// # Arguments
    /// * `amount_a`  – Desired amount of `asset_a`.
    /// * `amount_b`  – Desired amount of `asset_b`.
    /// * `min_a`     – Minimum acceptable `asset_a` deposited (slippage guard).
    /// * `min_b`     – Minimum acceptable `asset_b` deposited.
    /// * `from`      – Must equal the registered vault address.
    ///
    /// # Returns
    /// LP tokens minted to this strategy.
    ///
    /// # Errors
    /// * [`SoroswapLpError::NotVault`]
    /// * [`SoroswapLpError::Paused`]
    /// * [`SoroswapLpError::InvalidAmount`]
    pub fn deposit_liquidity(
        env: Env,
        amount_a: i128,
        amount_b: i128,
        min_a: i128,
        min_b: i128,
        from: Address,
    ) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount_a <= 0 || amount_b <= 0 {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }
        if get_paused(&env) {
            panic_with_error!(&env, SoroswapLpError::Paused);
        }

        from.require_auth();
        if from != get_vault(&env) {
            panic_with_error!(&env, SoroswapLpError::NotVault);
        }

        let asset_a = get_asset_a(&env);
        let asset_b = get_asset_b(&env);
        let lp_token = get_lp_token(&env);
        let router = get_router(&env);
        let strategy = env.current_contract_address();
        let deadline = env.ledger().timestamp() + 300;

        // Pull both tokens from vault into strategy.
        token::Client::new(&env, &asset_a).transfer(&from, &strategy, &amount_a);
        token::Client::new(&env, &asset_b).transfer(&from, &strategy, &amount_b);

        // Approve both tokens to the router.
        let expiry = env.ledger().sequence() + 100;
        token::Client::new(&env, &asset_a).approve(&strategy, &router, &amount_a, &expiry);
        token::Client::new(&env, &asset_b).approve(&strategy, &router, &amount_b, &expiry);

        // Balance before to compute LP minted.
        let lp_before = token::Client::new(&env, &lp_token).balance(&strategy);

        // Call Soroswap router.
        let (_a_used, _b_used, _lp_minted) = RouterAdapter::new(&env, &router).add_liquidity(
            asset_a.clone(),
            asset_b.clone(),
            amount_a,
            amount_b,
            min_a,
            min_b,
            strategy.clone(),
            deadline,
        );

        // Revoke any unspent allowance so a later router compromise cannot
        // drain tokens that were not consumed on this call.
        let zero = 0i128;
        let now = env.ledger().sequence();
        token::Client::new(&env, &asset_a).approve(&strategy, &router, &zero, &now);
        token::Client::new(&env, &asset_b).approve(&strategy, &router, &zero, &now);

        // Compute LP received.
        let lp_after = token::Client::new(&env, &lp_token).balance(&strategy);

        // Guard against a misbehaving router that burns or redirects LP tokens.
        if lp_after < lp_before {
            panic_with_error!(&env, SoroswapLpError::Overflow);
        }
        let lp_received = lp_after - lp_before;
        if lp_received <= 0 {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }

        // Track locally with checked arithmetic.
        let prev_lp = get_lp_balance(&env);
        let new_lp = prev_lp
            .checked_add(lp_received)
            .unwrap_or_else(|| panic_with_error!(&env, SoroswapLpError::Overflow));
        set_lp_balance(&env, new_lp);

        lp_received
    }

    /// Remove liquidity from the Soroswap pool.
    ///
    /// Underlying tokens go **directly from the DEX to `to`** (the user).
    ///
    /// # Arguments
    /// * `lp_amount` – LP tokens to redeem.
    /// * `min_a`     – Minimum `asset_a` to receive.
    /// * `min_b`     – Minimum `asset_b` to receive.
    /// * `from`      – Must equal the registered vault.
    /// * `to`        – Recipient of the underlying tokens (typically the user).
    ///
    /// # Returns
    /// `(amount_a, amount_b)` delivered to `to`.
    ///
    /// # Errors
    /// * [`SoroswapLpError::NotVault`]
    /// * [`SoroswapLpError::Paused`]
    /// * [`SoroswapLpError::InvalidAmount`]
    /// * [`SoroswapLpError::InsufficientLpBalance`]
    pub fn withdraw(
        env: Env,
        lp_amount: i128,
        min_a: i128,
        min_b: i128,
        from: Address,
        to: Address,
    ) -> (i128, i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if lp_amount <= 0 {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }
        if get_paused(&env) {
            panic_with_error!(&env, SoroswapLpError::Paused);
        }

        from.require_auth();
        if from != get_vault(&env) {
            panic_with_error!(&env, SoroswapLpError::NotVault);
        }

        let tracked = get_lp_balance(&env);
        if lp_amount > tracked {
            panic_with_error!(&env, SoroswapLpError::InsufficientLpBalance);
        }

        let asset_a = get_asset_a(&env);
        let asset_b = get_asset_b(&env);
        let lp_token = get_lp_token(&env);
        let router = get_router(&env);
        let strategy = env.current_contract_address();
        let deadline = env.ledger().timestamp() + 300;
        let expiry = env.ledger().sequence() + 100;

        // Approve LP token to router.
        token::Client::new(&env, &lp_token).approve(&strategy, &router, &lp_amount, &expiry);

        // Remove liquidity; router sends token_a + token_b directly to `to`.
        let (amount_a, amount_b) = RouterAdapter::new(&env, &router)
            .remove_liquidity(asset_a, asset_b, lp_amount, min_a, min_b, to, deadline);

        // Revoke any residual LP-token allowance after the router call.
        let zero = 0i128;
        let now = env.ledger().sequence();
        token::Client::new(&env, &lp_token).approve(&strategy, &router, &zero, &now);

        let new_tracked = tracked
            .checked_sub(lp_amount)
            .unwrap_or_else(|| panic_with_error!(&env, SoroswapLpError::Overflow));
        set_lp_balance(&env, new_tracked);

        (amount_a, amount_b)
    }

    // -----------------------------------------------------------------------
    // Valuation
    // -----------------------------------------------------------------------

    /// Return the LP token balance held by this strategy.
    pub fn get_lp_balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_lp_balance(&env)
    }

    /// Return the base-asset value of this strategy's LP position.
    ///
    /// ## With oracle configured (production path — dHedge V2 §3)
    /// Uses reserve decomposition for an accurate NAV contribution:
    /// ```text
    /// share        = lp_balance / lp_total_supply
    /// value_a      = reserve_a × oracle.get_price(asset_a) / PRICE_PRECISION
    /// value_b      = reserve_b × oracle.get_price(asset_b) / PRICE_PRECISION
    /// get_value()  = share × (value_a + value_b)
    /// ```
    /// This correctly accounts for impermanent loss and does **not** require an
    /// oracle for the LP token itself — only for the underlying pair assets.
    ///
    /// ## Without oracle (fallback)
    /// Returns raw LP token balance.  Vault NAV will be approximate; wire the
    /// oracle via `set_oracle(manager, oracle_address)` for a production build.
    pub fn get_value(env: Env, _vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        let lp_balance = get_lp_balance(&env);
        if lp_balance == 0 {
            return 0;
        }

        let oracle = match get_oracle(&env) {
            Some(o) => o,
            // No oracle configured — fall back to raw LP units.
            None => return lp_balance,
        };

        let lp_token = get_lp_token(&env);
        let asset_a = get_asset_a(&env);
        let asset_b = get_asset_b(&env);

        // Pool reserves and total LP supply from the Soroswap pair contract.
        let pair = PairAdapter::new(&env, &lp_token);
        let (reserve_a, reserve_b) = pair.get_reserves();
        let total_lp = pair.total_supply();

        if total_lp == 0 || (reserve_a == 0 && reserve_b == 0) {
            return lp_balance;
        }

        // Oracle prices (PRICE_PRECISION-scaled) for each underlying asset.
        let oracle_client = OracleAdapter::new(&env, &oracle);
        let price_a = oracle_client.get_price(&asset_a);
        let price_b = oracle_client.get_price(&asset_b);

        // Reserve decomposition:
        //   pool_value = reserveA × priceA/PREC + reserveB × priceB/PREC
        //   position   = lp_balance / total_lp × pool_value
        let pool_value_a = reserve_a.saturating_mul(price_a) / PRICE_PRECISION;
        let pool_value_b = reserve_b.saturating_mul(price_b) / PRICE_PRECISION;
        let pool_value = pool_value_a.saturating_add(pool_value_b);

        // Use multiply-before-divide to preserve precision.
        pool_value.saturating_mul(lp_balance) / total_lp
    }

    // -----------------------------------------------------------------------
    // Oracle configuration
    // -----------------------------------------------------------------------

    /// Set the oracle used for reserve-decomposition NAV. Manager only.
    ///
    /// Once set, `get_value()` will call `oracle.get_price(asset_a)` and
    /// `oracle.get_price(asset_b)` and compute NAV from pool reserves rather
    /// than returning raw LP token units.
    pub fn set_oracle(env: Env, caller: Address, oracle: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, SoroswapLpError::NotManager);
        }
        set_oracle(&env, &oracle);
    }

    // -----------------------------------------------------------------------
    // Metadata
    // -----------------------------------------------------------------------

    /// Return the first token in the pair.
    pub fn asset_a(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_asset_a(&env)
    }

    /// Return the second token in the pair.
    pub fn asset_b(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_asset_b(&env)
    }

    /// Return the LP token address.
    pub fn lp_token(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_lp_token(&env)
    }

    /// Return the Soroswap router address.
    pub fn get_router(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_router(&env)
    }

    /// Return the strategy name.
    pub fn get_name(env: Env) -> String {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_name(&env)
    }

    /// Return `true` if the strategy is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_paused(&env)
    }

    /// Return `true` when an internal oracle is configured.
    pub fn has_oracle(env: Env) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_oracle(&env).is_some()
    }

    // -----------------------------------------------------------------------
    // Emergency controls
    // -----------------------------------------------------------------------

    /// Pause the strategy (blocks deposits and withdrawals).
    ///
    /// # Auth
    /// Manager must authorize.
    pub fn pause(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, SoroswapLpError::NotManager);
        }
        set_paused(&env, true);
    }

    /// Unpause the strategy.
    ///
    /// # Auth
    /// Manager must authorize.
    pub fn unpause(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, SoroswapLpError::NotManager);
        }
        set_paused(&env, false);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
