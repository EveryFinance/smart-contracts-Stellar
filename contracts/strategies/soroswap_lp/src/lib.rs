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

use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, Address, Env, IntoVal, String, Symbol,
};

use interfaces::{PairAdapter, RouterAdapter};

/// Fixed-point precision matching the vault and oracle (7 decimal places).
const PRICE_PRECISION: i128 = 10_000_000;

fn checked_mul_div(env: &Env, a: i128, b: i128, denominator: i128) -> i128 {
    if denominator <= 0 {
        panic_with_error!(env, SoroswapLpError::InvalidAmount);
    }
    a.checked_mul(b)
        .map(|v| v / denominator)
        .unwrap_or_else(|| panic_with_error!(env, SoroswapLpError::Overflow))
}

fn checked_add(env: &Env, a: i128, b: i128) -> i128 {
    a.checked_add(b)
        .unwrap_or_else(|| panic_with_error!(env, SoroswapLpError::Overflow))
}

use storage::{
    get_asset_a, get_asset_b, get_factory, get_lp_balance, get_lp_token, get_name, get_router,
    get_vault, is_initialized, set_asset_a, set_asset_b, set_factory, set_initialized,
    set_lp_balance, set_lp_token, set_name, set_router, set_vault, INSTANCE_BUMP_AMOUNT,
    INSTANCE_LIFETIME_THRESHOLD,
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
    /// * `name`     – Human-readable label.
    ///
    /// # Auth
    /// The vault's current on-chain manager must authorise this call.
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
        name: String,
    ) {
        if is_initialized(&env) {
            panic_with_error!(&env, SoroswapLpError::AlreadyInitialized);
        }

        // Derive the vault's manager from on-chain state and require their
        // signature to prevent front-running initialization.
        let vault_manager: Address =
            env.invoke_contract(&vault, &Symbol::new(&env, "get_manager"), ().into_val(&env));
        vault_manager.require_auth();
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        set_vault(&env, &vault);
        set_asset_a(&env, &asset_a);
        set_asset_b(&env, &asset_b);
        set_lp_token(&env, &lp_token);
        set_router(&env, &router);
        set_name(&env, &name);
        set_lp_balance(&env, 0);

        // Cache factory locally: get_total_value must not call back into vault
        // (re-entry), so we resolve vault → factory once during initialization.
        let factory_opt: Option<Address> =
            env.invoke_contract(&vault, &Symbol::new(&env, "get_factory"), ().into_val(&env));
        if let Some(ref factory) = factory_opt {
            set_factory(&env, factory);
        }
        set_initialized(&env);
    }

    // -----------------------------------------------------------------------
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

    /// Return the raw LP share balance held by this strategy.
    ///
    /// Used by the vault's `set_strategies` removal guard to detect active LP
    /// positions even when `get_value` returns 0 (e.g. pool reserves temporarily zero).
    pub fn get_share_balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_lp_balance(&env)
    }

    /// Return the base-asset value of this strategy's LP position.
    ///
    /// Uses reserve decomposition with prices from the vault's AssetHandler.
    /// Returns 0 if the vault has no AssetHandler configured.
    pub fn get_value(env: Env, vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Self::compute_lp_value(&env, &vault)
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

    // -----------------------------------------------------------------------
    // Multi-asset guard interface v2
    // -----------------------------------------------------------------------

    /// Return the total base-asset value of all LP positions for `vault`.
    ///
    /// Uses reserve decomposition with prices from the vault's AssetHandler.
    pub fn get_total_value(env: Env, vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        if vault != get_vault(&env) {
            return 0;
        }
        Self::compute_lp_value(&env, &vault)
    }

    /// Withdraw `numerator/denominator` fraction of LP position and send the
    /// underlying tokens (token_a + token_b) directly to `to`.
    ///
    /// Calls `remove_liquidity(…, to=to)` so tokens go directly to the user.
    ///
    /// # Errors
    /// * [`SoroswapLpError::NotVault`] if `vault` ≠ registered vault.
    pub fn withdraw_fraction(
        env: Env,
        vault: Address,
        numerator: i128,
        denominator: i128,
        to: Address,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        vault.require_auth();
        if vault != get_vault(&env) {
            panic_with_error!(&env, SoroswapLpError::NotVault);
        }
        if numerator <= 0 || denominator <= 0 || numerator > denominator {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }

        let tracked = get_lp_balance(&env);
        if tracked == 0 {
            return;
        }

        let lp_amount = checked_mul_div(&env, tracked, numerator, denominator);
        if lp_amount == 0 {
            return;
        }

        let asset_a = get_asset_a(&env);
        let asset_b = get_asset_b(&env);
        let lp_token = get_lp_token(&env);
        let router = get_router(&env);
        let strategy = env.current_contract_address();
        let deadline = env.ledger().timestamp() + 300;
        let expiry = env.ledger().sequence() + 100;

        token::Client::new(&env, &lp_token).approve(&strategy, &router, &lp_amount, &expiry);
        RouterAdapter::new(&env, &router)
            .remove_liquidity(asset_a, asset_b, lp_amount, 0, 0, to, deadline);
        let now = env.ledger().sequence();
        token::Client::new(&env, &lp_token).approve(&strategy, &router, &0i128, &now);

        let new_tracked = tracked
            .checked_sub(lp_amount)
            .unwrap_or_else(|| panic_with_error!(&env, SoroswapLpError::Overflow));
        set_lp_balance(&env, new_tracked);
    }

    /// Return `true` if this strategy has a non-zero LP position for `vault`
    /// that involves `asset` (either token_a or token_b).
    pub fn asset_in_use(env: Env, vault: Address, asset: Address) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if vault != get_vault(&env) {
            return false;
        }
        if get_lp_balance(&env) == 0 {
            return false;
        }
        asset == get_asset_a(&env) || asset == get_asset_b(&env)
    }

    // -----------------------------------------------------------------------
    // Trader-callable functions (dispatched via vault.execute_op)
    //
    // The vault injects its own address as the first argument.  Callers
    // cannot substitute a different source address, so funds only flow
    // from the registered vault.
    //
    // `add_liquidity` and `swap` use vault-scoped contract authorization
    // prepared by vault.execute_op, avoiding standing token approvals.
    // -----------------------------------------------------------------------

    /// Add liquidity to Soroswap on behalf of `vault`.
    ///
    /// Called via `vault.execute_op(caller, strategy, "add_liquidity",
    ///   [amount_a, amount_b, min_a, min_b])`.
    pub fn add_liquidity(
        env: Env,
        vault: Address,
        amount_a: i128,
        amount_b: i128,
        min_a: i128,
        min_b: i128,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount_a <= 0 || amount_b <= 0 || min_a < 0 || min_b < 0 {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, SoroswapLpError::NotVault);
        }

        let asset_a = get_asset_a(&env);
        let asset_b = get_asset_b(&env);
        let lp_token = get_lp_token(&env);
        let router = get_router(&env);
        let strategy = env.current_contract_address();
        let deadline = env.ledger().timestamp() + 300;
        let expiry = env.ledger().sequence() + 100;

        // Pull both tokens from vault into strategy using the vault's scoped
        // contract authorization prepared by vault.execute_op.
        token::Client::new(&env, &asset_a).transfer(&vault, &strategy, &amount_a);
        token::Client::new(&env, &asset_b).transfer(&vault, &strategy, &amount_b);

        // Approve both tokens to the router.
        token::Client::new(&env, &asset_a).approve(&strategy, &router, &amount_a, &expiry);
        token::Client::new(&env, &asset_b).approve(&strategy, &router, &amount_b, &expiry);

        let lp_before = token::Client::new(&env, &lp_token).balance(&strategy);

        let (a_used, b_used, _lp) = RouterAdapter::new(&env, &router).add_liquidity(
            asset_a.clone(),
            asset_b.clone(),
            amount_a,
            amount_b,
            min_a,
            min_b,
            strategy.clone(),
            deadline,
        );

        // Revoke residual allowances.
        let now = env.ledger().sequence();
        token::Client::new(&env, &asset_a).approve(&strategy, &router, &0i128, &now);
        token::Client::new(&env, &asset_b).approve(&strategy, &router, &0i128, &now);

        if a_used > amount_a || b_used > amount_b {
            panic_with_error!(&env, SoroswapLpError::Overflow);
        }

        // Return dust back to vault.
        let dust_a = amount_a - a_used;
        let dust_b = amount_b - b_used;
        if dust_a > 0 {
            token::Client::new(&env, &asset_a).transfer(&strategy, &vault, &dust_a);
        }
        if dust_b > 0 {
            token::Client::new(&env, &asset_b).transfer(&strategy, &vault, &dust_b);
        }

        let lp_after = token::Client::new(&env, &lp_token).balance(&strategy);
        if lp_after < lp_before {
            panic_with_error!(&env, SoroswapLpError::Overflow);
        }
        let lp_received = lp_after - lp_before;
        if lp_received <= 0 {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }

        let new_lp = get_lp_balance(&env)
            .checked_add(lp_received)
            .unwrap_or_else(|| panic_with_error!(&env, SoroswapLpError::Overflow));
        set_lp_balance(&env, new_lp);
    }

    /// Remove `lp_amount` of liquidity from Soroswap and send tokens to `vault`.
    ///
    /// Called via `vault.execute_op(caller, strategy, "remove_liquidity",
    ///   [lp_amount, min_a, min_b])`.
    pub fn remove_liquidity(env: Env, vault: Address, lp_amount: i128, min_a: i128, min_b: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if lp_amount <= 0 || min_a < 0 || min_b < 0 {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }
        if vault != get_vault(&env) {
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

        token::Client::new(&env, &lp_token).approve(&strategy, &router, &lp_amount, &expiry);

        // Underlying tokens go directly to vault.
        RouterAdapter::new(&env, &router)
            .remove_liquidity(asset_a, asset_b, lp_amount, min_a, min_b, vault, deadline);

        let now = env.ledger().sequence();
        token::Client::new(&env, &lp_token).approve(&strategy, &router, &0i128, &now);

        let new_tracked = tracked
            .checked_sub(lp_amount)
            .unwrap_or_else(|| panic_with_error!(&env, SoroswapLpError::Overflow));
        set_lp_balance(&env, new_tracked);
    }

    /// Swap `amount_in` of `from_asset` for `to_asset` via Soroswap router.
    ///
    /// Both `from_asset` and `to_asset` must be in the vault's PortfolioAssets.
    /// Output tokens go directly to `vault`.
    ///
    /// Called via `vault.execute_op(caller, strategy, "swap",
    ///   [from_asset, to_asset, amount_in, min_out])`.
    pub fn swap(
        env: Env,
        vault: Address,
        from_asset: Address,
        to_asset: Address,
        amount_in: i128,
        min_out: i128,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount_in <= 0 || min_out < 0 {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, SoroswapLpError::NotVault);
        }
        let asset_a = get_asset_a(&env);
        let asset_b = get_asset_b(&env);
        let valid_pair = (from_asset == asset_a && to_asset == asset_b)
            || (from_asset == asset_b && to_asset == asset_a);
        if !valid_pair {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }

        let router = get_router(&env);
        let strategy = env.current_contract_address();
        let expiry = env.ledger().sequence() + 100;
        let deadline = env.ledger().timestamp() + 300;

        // Pull from_asset from vault into strategy using the vault's scoped
        // contract authorization prepared by vault.execute_op.
        token::Client::new(&env, &from_asset).transfer(&vault, &strategy, &amount_in);

        // Approve from_asset to router.
        token::Client::new(&env, &from_asset).approve(&strategy, &router, &amount_in, &expiry);

        // Build path: [from_asset, to_asset].
        let path = soroban_sdk::vec![&env, from_asset.clone(), to_asset];

        // Swap; router sends to_asset directly to vault.
        RouterAdapter::new(&env, &router)
            .swap_exact_tokens_for_tokens(amount_in, min_out, path, vault, deadline);

        // Revoke residual allowance.
        let now = env.ledger().sequence();
        token::Client::new(&env, &from_asset).approve(&strategy, &router, &0i128, &now);
    }

    // -----------------------------------------------------------------------
    // Emergency controls
    // -----------------------------------------------------------------------

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn compute_lp_value(env: &Env, _vault: &Address) -> i128 {
        let lp_balance = get_lp_balance(env);
        if lp_balance == 0 {
            return 0;
        }

        // Get AssetHandler from factory (cached locally to avoid vault re-entry).
        let factory_opt = get_factory(env);
        let asset_handler_opt: Option<Address> = factory_opt.and_then(|factory| {
            env.invoke_contract(
                &factory,
                &Symbol::new(env, "get_asset_handler"),
                ().into_val(env),
            )
        });
        let asset_handler = asset_handler_opt
            .unwrap_or_else(|| panic_with_error!(env, SoroswapLpError::InvalidOraclePrice));

        let lp_token = get_lp_token(env);
        let asset_a = get_asset_a(env);
        let asset_b = get_asset_b(env);

        let pair = PairAdapter::new(env, &lp_token);
        let (reserve_0, reserve_1) = pair.get_reserves();
        let total_lp = pair.total_supply();

        if total_lp == 0 || (reserve_0 == 0 && reserve_1 == 0) {
            panic_with_error!(env, SoroswapLpError::InvalidOraclePrice);
        }

        // Map reserves to asset_a/asset_b using token0 ordering.
        let token0 = pair.token0();
        let (reserve_a, reserve_b) = if token0 == asset_a {
            (reserve_0, reserve_1)
        } else {
            (reserve_1, reserve_0)
        };

        let price_a: i128 = env.invoke_contract(
            &asset_handler,
            &Symbol::new(env, "get_price"),
            (asset_a,).into_val(env),
        );
        let price_b: i128 = env.invoke_contract(
            &asset_handler,
            &Symbol::new(env, "get_price"),
            (asset_b,).into_val(env),
        );
        if price_a <= 0 || price_b <= 0 {
            panic_with_error!(env, SoroswapLpError::InvalidOraclePrice);
        }

        let pool_value_a = checked_mul_div(env, reserve_a, price_a, PRICE_PRECISION);
        let pool_value_b = checked_mul_div(env, reserve_b, price_b, PRICE_PRECISION);
        let pool_value = checked_add(env, pool_value_a, pool_value_b);
        checked_mul_div(env, lp_balance, pool_value, total_lp)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
