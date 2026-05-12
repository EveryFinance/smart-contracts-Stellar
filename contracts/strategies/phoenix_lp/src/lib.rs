//! # Phoenix LP Strategy — Multi-Position AMM Liquidity Manager
//!
//! One deployed instance manages **all** Phoenix LP positions for the owning
//! vault across any number of Phoenix pools.  Positions are keyed by pool
//! address; each pool stores two assets and a share token.
//!
//! ## Budget optimisations
//! * `share_token` and `token0_ordering` are cached per pool on first
//!   `add_liquidity` call (`query_share_token_address` called once per pool).
//! * `AssetHandler` is lazy-cached on first valuation.
//! * `get_total_value` calls `get_prices` once for all unique assets.
//! * Same-ledger approvals; no zero-revoke needed.
//!
//! ## Arg conventions for vault.execute_op dispatch
//! ```text
//! add_liquidity    args: [pool, asset_a, asset_b, amount_a, amount_b, min_a, min_b]
//! remove_liquidity args: [pool, share_amount, min_a, min_b]
//! swap             args: [pool, sell_a, amount_in, min_out]
//! ```

#![no_std]
#![allow(clippy::too_many_arguments)]

mod error;
mod interfaces;
mod storage;

pub use error::PhoenixLpError;

use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, Address, Env, IntoVal, Map, String, Symbol,
    Vec,
};

use interfaces::{PhoenixPoolAdapter, Sep41TokenAdapter};

use storage::{
    add_to_active_positions, get_active_positions, get_name, get_or_cache_asset_handler,
    get_position, get_vault, is_initialized, remove_from_active_positions, set_asset_handler,
    set_factory, set_initialized, set_name, set_position, set_vault, PhoenixPosition,
    INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
};

const PRICE_PRECISION: i128 = 10_000_000;

fn checked_mul_div(env: &Env, a: i128, b: i128, denominator: i128) -> i128 {
    if denominator <= 0 {
        panic_with_error!(env, PhoenixLpError::InvalidAmount);
    }
    a.checked_mul(b)
        .map(|v| v / denominator)
        .unwrap_or_else(|| panic_with_error!(env, PhoenixLpError::Overflow))
}

fn checked_add(env: &Env, a: i128, b: i128) -> i128 {
    a.checked_add(b)
        .unwrap_or_else(|| panic_with_error!(env, PhoenixLpError::Overflow))
}

#[contract]
pub struct PhoenixLpStrategy;

#[contractimpl]
impl PhoenixLpStrategy {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the strategy for a vault.
    ///
    /// No pool-specific arguments — pools are registered dynamically on first
    /// `add_liquidity` call.
    pub fn initialize(env: Env, vault: Address, name: String) {
        if is_initialized(&env) {
            panic_with_error!(&env, PhoenixLpError::AlreadyInitialized);
        }

        let vault_manager: Address =
            env.invoke_contract(&vault, &Symbol::new(&env, "get_manager"), ().into_val(&env));
        vault_manager.require_auth();

        set_vault(&env, &vault);
        set_name(&env, &name);

        let factory_opt: Option<Address> =
            env.invoke_contract(&vault, &Symbol::new(&env, "get_factory"), ().into_val(&env));
        if let Some(ref factory) = factory_opt {
            set_factory(&env, factory);
            let ah_opt: Option<Address> = env.invoke_contract(
                factory,
                &Symbol::new(&env, "get_asset_handler"),
                ().into_val(&env),
            );
            if let Some(ref ah) = ah_opt {
                set_asset_handler(&env, ah);
            }
        }

        set_initialized(&env);
    }

    // -----------------------------------------------------------------------
    // Valuation
    // -----------------------------------------------------------------------

    /// Return the total base-asset value of all LP positions for `vault`.
    ///
    /// Budget: 1 `get_reserves` + 1 `total_supply` per active pool,
    ///         plus ONE `get_prices` batch call for all unique assets.
    pub fn get_total_value(env: Env, vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if vault != get_vault(&env) {
            return 0;
        }
        Self::compute_lp_value(&env)
    }

    /// Alias for `get_total_value` (backwards-compatible single-vault call).
    pub fn get_value(env: Env, vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        if vault != get_vault(&env) {
            return 0;
        }
        Self::compute_lp_value(&env)
    }

    /// Return the number of active LP positions (consistent with other strategies).
    pub fn get_share_balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_active_positions(&env).len() as i128
    }

    /// Return all pool addresses with a non-zero share balance.
    pub fn get_active_positions(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_active_positions(&env)
    }

    // -----------------------------------------------------------------------
    // Metadata
    // -----------------------------------------------------------------------

    pub fn get_name(env: Env) -> String {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_name(&env)
    }

    // -----------------------------------------------------------------------
    // Guard interface
    // -----------------------------------------------------------------------

    /// Proportionally withdraw `numerator/denominator` of every active pool
    /// and send underlying tokens to `to`.
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
            panic_with_error!(&env, PhoenixLpError::NotVault);
        }
        if numerator <= 0 || denominator <= 0 || numerator > denominator {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }

        let active = get_active_positions(&env);
        if active.is_empty() {
            return;
        }

        let strategy = env.current_contract_address();
        let deadline = env.ledger().timestamp() + 300;
        let mut exhausted: Vec<Address> = Vec::new(&env);

        for pool in active.iter() {
            let mut pos = match get_position(&env, &pool) {
                Some(p) if p.total_shares > 0 => p,
                _ => continue,
            };

            let share_amount = checked_mul_div(&env, pos.total_shares, numerator, denominator);
            if share_amount == 0 {
                continue;
            }

            let expiry = env.ledger().sequence(); // same-ledger
            token::Client::new(&env, &pos.share_token).approve(
                &strategy,
                &pool,
                &share_amount,
                &expiry,
            );

            PhoenixPoolAdapter::new(&env, &pool).withdraw_liquidity(
                to.clone(),
                share_amount,
                0,
                0,
                Some(deadline),
            );

            pos.total_shares = pos
                .total_shares
                .checked_sub(share_amount)
                .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::Overflow));
            set_position(&env, &pool, &pos);

            if pos.total_shares == 0 {
                exhausted.push_back(pool.clone());
            }
        }

        for pool in exhausted.iter() {
            remove_from_active_positions(&env, &pool);
        }
    }

    /// Return `true` if any active position involves `asset`.
    pub fn asset_in_use(env: Env, vault: Address, asset: Address) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if vault != get_vault(&env) {
            return false;
        }
        for pool in get_active_positions(&env).iter() {
            if let Some(pos) = get_position(&env, &pool) {
                if pos.total_shares > 0 && (asset == pos.asset_a || asset == pos.asset_b) {
                    return true;
                }
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // Trader-callable operations (dispatched via vault.execute_op)
    // -----------------------------------------------------------------------

    /// Add liquidity to the Phoenix `pool`.
    ///
    /// On first call for a given pool, `pool.query_share_token_address()` is
    /// called once and cached; subsequent calls use the cache.
    pub fn add_liquidity(
        env: Env,
        vault: Address,
        pool: Address,
        asset_a: Address,
        asset_b: Address,
        amount_a: i128,
        amount_b: i128,
        min_a: i128,
        min_b: i128,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount_a <= 0 || amount_b <= 0 || min_a < 0 || min_b < 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, PhoenixLpError::NotVault);
        }

        let strategy = env.current_contract_address();
        let expiry = env.ledger().sequence(); // same-ledger
        let deadline = env.ledger().timestamp() + 300;

        // Resolve or create position; query share_token once per pool.
        let mut position = get_position(&env, &pool).unwrap_or_else(|| {
            let share_token = PhoenixPoolAdapter::new(&env, &pool).query_share_token_address();
            PhoenixPosition {
                asset_a: asset_a.clone(),
                asset_b: asset_b.clone(),
                share_token,
                total_shares: 0,
            }
        });

        // Validate assets match cached position on subsequent calls.
        if position.total_shares > 0 && (position.asset_a != asset_a || position.asset_b != asset_b)
        {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }

        token::Client::new(&env, &asset_a).approve(&strategy, &pool, &amount_a, &expiry);
        token::Client::new(&env, &asset_b).approve(&strategy, &pool, &amount_b, &expiry);

        let shares_before = token::Client::new(&env, &position.share_token).balance(&strategy);

        PhoenixPoolAdapter::new(&env, &pool).provide_liquidity(
            strategy.clone(),
            Some(amount_a),
            Some(min_a),
            Some(amount_b),
            Some(min_b),
            None,
            Some(deadline),
        );

        // Return any residual tokens to vault.
        let residual_a = token::Client::new(&env, &asset_a).balance(&strategy);
        let residual_b = token::Client::new(&env, &asset_b).balance(&strategy);
        if residual_a > 0 {
            token::Client::new(&env, &asset_a).transfer(&strategy, &vault, &residual_a);
        }
        if residual_b > 0 {
            token::Client::new(&env, &asset_b).transfer(&strategy, &vault, &residual_b);
        }

        let shares_after = token::Client::new(&env, &position.share_token).balance(&strategy);
        if shares_after < shares_before {
            panic_with_error!(&env, PhoenixLpError::Overflow);
        }
        let shares_minted = shares_after - shares_before;
        if shares_minted <= 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }

        position.total_shares = checked_add(&env, position.total_shares, shares_minted);
        set_position(&env, &pool, &position);
        add_to_active_positions(&env, &pool);
    }

    /// Remove `share_amount` from the Phoenix `pool` and send tokens to `vault`.
    ///
    /// Returns `(amount_a, amount_b)` received from the pool so the vault can
    /// compute post-operation value without a redundant deep LP re-valuation.
    pub fn remove_liquidity(
        env: Env,
        vault: Address,
        pool: Address,
        asset_a: Address,
        asset_b: Address,
        share_amount: i128,
        min_a: i128,
        min_b: i128,
    ) -> (i128, i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if share_amount <= 0 || min_a < 0 || min_b < 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, PhoenixLpError::NotVault);
        }

        let mut position = get_position(&env, &pool)
            .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::InsufficientShares));

        if position.asset_a != asset_a || position.asset_b != asset_b {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        if share_amount > position.total_shares {
            panic_with_error!(&env, PhoenixLpError::InsufficientShares);
        }

        let strategy = env.current_contract_address();
        let expiry = env.ledger().sequence();
        let deadline = env.ledger().timestamp() + 300;

        token::Client::new(&env, &position.share_token).approve(
            &strategy,
            &pool,
            &share_amount,
            &expiry,
        );

        let (amount_a, amount_b) = PhoenixPoolAdapter::new(&env, &pool).withdraw_liquidity(
            vault,
            share_amount,
            min_a,
            min_b,
            Some(deadline),
        );

        position.total_shares = position
            .total_shares
            .checked_sub(share_amount)
            .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::Overflow));
        set_position(&env, &pool, &position);

        if position.total_shares == 0 {
            remove_from_active_positions(&env, &pool);
        }

        (amount_a, amount_b)
    }

    /// Swap within the Phoenix `pool` on behalf of `vault`.
    ///
    /// `asset_in`/`asset_out` must match the pool's registered assets.
    /// `sell_a` is derived from which asset is `asset_in`.
    pub fn swap(
        env: Env,
        vault: Address,
        pool: Address,
        asset_in: Address,
        asset_out: Address,
        amount_in: i128,
        min_out: i128,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount_in <= 0 || min_out < 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, PhoenixLpError::NotVault);
        }

        let position = get_position(&env, &pool)
            .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::NotInitialized));

        let sell_a = if asset_in == position.asset_a && asset_out == position.asset_b {
            true
        } else if asset_in == position.asset_b && asset_out == position.asset_a {
            false
        } else {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount)
        };

        let strategy = env.current_contract_address();
        let expiry = env.ledger().sequence();
        let deadline = env.ledger().timestamp() + 300;

        token::Client::new(&env, &asset_in).approve(&strategy, &pool, &amount_in, &expiry);

        PhoenixPoolAdapter::new(&env, &pool).swap(
            strategy,
            vault,
            sell_a,
            amount_in,
            min_out,
            None,
            Some(deadline),
        );
    }

    // -----------------------------------------------------------------------
    // Legacy direct interface (kept for backwards compatibility)
    // -----------------------------------------------------------------------

    /// Provide liquidity to `pool` on behalf of `from` (vault).
    /// Pulls tokens from `from` via token.transfer.
    pub fn deposit_liquidity(
        env: Env,
        amount_a: i128,
        amount_b: i128,
        min_a: i128,
        min_b: i128,
        pool: Address,
        asset_a: Address,
        asset_b: Address,
        from: Address,
    ) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount_a <= 0 || amount_b <= 0 || min_a < 0 || min_b < 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        from.require_auth();
        if from != get_vault(&env) {
            panic_with_error!(&env, PhoenixLpError::NotVault);
        }

        let strategy = env.current_contract_address();
        let deadline = env.ledger().timestamp() + 300;
        let expiry = env.ledger().sequence();

        // Resolve or create position.
        let mut position = get_position(&env, &pool).unwrap_or_else(|| {
            let share_token = PhoenixPoolAdapter::new(&env, &pool).query_share_token_address();
            PhoenixPosition {
                asset_a: asset_a.clone(),
                asset_b: asset_b.clone(),
                share_token,
                total_shares: 0,
            }
        });

        // Pull tokens from vault.
        token::Client::new(&env, &asset_a).transfer(&from, &strategy, &amount_a);
        token::Client::new(&env, &asset_b).transfer(&from, &strategy, &amount_b);

        token::Client::new(&env, &asset_a).approve(&strategy, &pool, &amount_a, &expiry);
        token::Client::new(&env, &asset_b).approve(&strategy, &pool, &amount_b, &expiry);

        let shares_before = token::Client::new(&env, &position.share_token).balance(&strategy);

        PhoenixPoolAdapter::new(&env, &pool).provide_liquidity(
            strategy.clone(),
            Some(amount_a),
            Some(min_a),
            Some(amount_b),
            Some(min_b),
            None,
            Some(deadline),
        );

        // Return residuals to vault.
        let residual_a = token::Client::new(&env, &asset_a).balance(&strategy);
        let residual_b = token::Client::new(&env, &asset_b).balance(&strategy);
        if residual_a > 0 {
            token::Client::new(&env, &asset_a).transfer(&strategy, &from, &residual_a);
        }
        if residual_b > 0 {
            token::Client::new(&env, &asset_b).transfer(&strategy, &from, &residual_b);
        }

        let shares_after = token::Client::new(&env, &position.share_token).balance(&strategy);
        if shares_after < shares_before {
            panic_with_error!(&env, PhoenixLpError::Overflow);
        }
        let shares_minted = shares_after - shares_before;
        if shares_minted <= 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }

        position.total_shares = checked_add(&env, position.total_shares, shares_minted);
        set_position(&env, &pool, &position);
        add_to_active_positions(&env, &pool);

        shares_minted
    }

    /// Withdraw `share_amount` from `pool`, sending tokens to `to`.
    pub fn withdraw(
        env: Env,
        share_amount: i128,
        min_a: i128,
        min_b: i128,
        pool: Address,
        from: Address,
        to: Address,
    ) -> (i128, i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if share_amount <= 0 || min_a < 0 || min_b < 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        from.require_auth();
        if from != get_vault(&env) {
            panic_with_error!(&env, PhoenixLpError::NotVault);
        }

        let mut position = get_position(&env, &pool)
            .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::InsufficientShares));

        let live_balance = token::Client::new(&env, &position.share_token)
            .balance(&env.current_contract_address());
        if share_amount > live_balance {
            panic_with_error!(&env, PhoenixLpError::InsufficientShares);
        }

        let strategy = env.current_contract_address();
        let expiry = env.ledger().sequence();
        let deadline = env.ledger().timestamp() + 300;

        token::Client::new(&env, &position.share_token).approve(
            &strategy,
            &pool,
            &share_amount,
            &expiry,
        );

        let (amount_a, amount_b) = PhoenixPoolAdapter::new(&env, &pool).withdraw_liquidity(
            to,
            share_amount,
            min_a,
            min_b,
            Some(deadline),
        );

        position.total_shares = position.total_shares.checked_sub(share_amount).unwrap_or(0);
        set_position(&env, &pool, &position);

        if position.total_shares == 0 {
            remove_from_active_positions(&env, &pool);
        }

        (amount_a, amount_b)
    }

    // -----------------------------------------------------------------------
    // Oracle cache refresh (permissionless)
    // -----------------------------------------------------------------------

    pub fn refresh_asset_handler(env: Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let vault = get_vault(&env);
        let factory_opt: Option<Address> =
            env.invoke_contract(&vault, &Symbol::new(&env, "get_factory"), ().into_val(&env));
        if let Some(ref factory) = factory_opt {
            set_factory(&env, factory);
            let ah_opt: Option<Address> = env.invoke_contract(
                factory,
                &Symbol::new(&env, "get_asset_handler"),
                ().into_val(&env),
            );
            if let Some(ref ah) = ah_opt {
                set_asset_handler(&env, ah);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Compute total base-asset value across all active LP positions.
    ///
    /// Budget:
    /// * Pass 1 — storage reads only: collect unique assets.
    /// * ONE `get_prices` call for all unique assets.
    /// * Pass 2 — 2 cross-contract calls per position: `get_reserves` + `total_supply`.
    fn compute_lp_value(env: &Env) -> i128 {
        let active = get_active_positions(env);
        if active.is_empty() {
            return 0;
        }

        let asset_handler = match get_or_cache_asset_handler(env) {
            Some(ah) => ah,
            None => panic_with_error!(env, PhoenixLpError::InvalidOraclePrice),
        };

        // Pass 1: collect unique assets (storage reads only).
        let mut unique_assets: Vec<Address> = Vec::new(env);
        for pool in active.iter() {
            if let Some(pos) = get_position(env, &pool) {
                if pos.total_shares > 0 {
                    if !unique_assets.contains(pos.asset_a.clone()) {
                        unique_assets.push_back(pos.asset_a);
                    }
                    if !unique_assets.contains(pos.asset_b.clone()) {
                        unique_assets.push_back(pos.asset_b);
                    }
                }
            }
        }

        if unique_assets.is_empty() {
            return 0;
        }

        // ONE batch price call.
        let prices: Map<Address, i128> = env.invoke_contract(
            &asset_handler,
            &Symbol::new(env, "get_prices"),
            (unique_assets,).into_val(env),
        );

        // Pass 2: value each position (get_reserves + total_supply per pool).
        let mut total = 0i128;
        for pool in active.iter() {
            let pos = match get_position(env, &pool) {
                Some(p) if p.total_shares > 0 => p,
                _ => continue,
            };

            let (reserve_a, reserve_b) = PhoenixPoolAdapter::new(env, &pool).get_reserves();
            let total_shares = Sep41TokenAdapter::new(env, &pos.share_token).total_supply();

            if total_shares == 0 || (reserve_a == 0 && reserve_b == 0) {
                panic_with_error!(env, PhoenixLpError::InvalidOraclePrice);
            }

            let pa = prices
                .get(pos.asset_a.clone())
                .unwrap_or_else(|| panic_with_error!(env, PhoenixLpError::InvalidOraclePrice));
            let pb = prices
                .get(pos.asset_b.clone())
                .unwrap_or_else(|| panic_with_error!(env, PhoenixLpError::InvalidOraclePrice));

            if pa <= 0 || pb <= 0 {
                panic_with_error!(env, PhoenixLpError::InvalidOraclePrice);
            }

            let va = checked_mul_div(env, reserve_a, pa, PRICE_PRECISION);
            let vb = checked_mul_div(env, reserve_b, pb, PRICE_PRECISION);
            let pool_value = checked_add(env, va, vb);
            let pos_value = checked_mul_div(env, pos.total_shares, pool_value, total_shares);
            total = checked_add(env, total, pos_value);
        }

        total
    }
}

#[cfg(test)]
mod test;
