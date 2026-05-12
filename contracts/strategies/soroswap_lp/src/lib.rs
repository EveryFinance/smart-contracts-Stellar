//! # Soroswap LP Strategy — Multi-Position AMM Liquidity Manager
//!
//! One deployed instance of this contract manages **all** Soroswap LP positions
//! for the owning vault.  Positions are keyed by LP-token address (each
//! Soroswap pair has a unique LP token == pair contract).
//!
//! ## Budget optimisations
//! * `token0_is_asset_a` is cached per position at first `add_liquidity` call
//!   (one `pair.token0()` cross-contract call per pair, never repeated).
//! * `AssetHandler` address is lazy-cached on first valuation
//!   (eliminates `factory.get_asset_handler()` on every NAV call).
//! * `get_total_value` calls `asset_handler.get_prices(Vec<Address>)` once
//!   to price all underlying assets in one cross-contract round-trip.
//! * `remove_liquidity` uses same-ledger approval and no explicit zero-revoke.
//!
//! ## Arg conventions for vault.execute_op dispatch
//! ```text
//! add_liquidity     args: [lp_token, asset_a, asset_b, amount_a, amount_b, min_a, min_b]
//! remove_liquidity  args: [lp_token, lp_amount, min_a, min_b]
//! swap              args: [from_asset, to_asset, amount_in, min_out]
//! ```

#![no_std]
#![allow(clippy::too_many_arguments)]

mod error;
mod interfaces;
mod storage;

pub use error::SoroswapLpError;

use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, Address, Env, IntoVal, Map, String, Symbol,
    Vec,
};

use interfaces::{PairAdapter, RouterAdapter};

use storage::{
    add_to_active_positions, get_active_positions, get_or_cache_asset_handler, get_position,
    get_router, get_vault, is_initialized, remove_from_active_positions, set_active_positions,
    set_asset_handler, set_factory, set_initialized, set_name, set_position, set_router, set_vault,
    LpPosition, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
};

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

fn checked_sub(env: &Env, a: i128, b: i128) -> i128 {
    a.checked_sub(b)
        .unwrap_or_else(|| panic_with_error!(env, SoroswapLpError::Overflow))
}

#[contract]
pub struct SoroswapLpStrategy;

#[allow(clippy::too_many_arguments)]
#[contractimpl]
impl SoroswapLpStrategy {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the strategy for a vault.
    ///
    /// No pair-specific arguments — pairs are registered dynamically on first
    /// `add_liquidity` call.
    ///
    /// # Auth
    /// The vault's current on-chain manager must authorise this call.
    pub fn initialize(env: Env, vault: Address, router: Address, name: String) {
        if is_initialized(&env) {
            panic_with_error!(&env, SoroswapLpError::AlreadyInitialized);
        }

        let vault_manager: Address =
            env.invoke_contract(&vault, &Symbol::new(&env, "get_manager"), ().into_val(&env));
        vault_manager.require_auth();

        set_vault(&env, &vault);
        set_router(&env, &router);
        set_name(&env, &name);

        // Cache factory to avoid calling back into vault during valuation.
        let factory_opt: Option<Address> =
            env.invoke_contract(&vault, &Symbol::new(&env, "get_factory"), ().into_val(&env));
        if let Some(ref factory) = factory_opt {
            set_factory(&env, factory);
            // Pre-cache AssetHandler if already configured.
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

    /// Return the LP balance for a specific pair (`lp_token`).
    pub fn get_lp_balance(env: Env, lp_token: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_position(&env, &lp_token)
            .map(|p| p.lp_balance)
            .unwrap_or(0)
    }

    /// Return the number of active LP positions (> 0 means strategy holds LP).
    ///
    /// Used by the vault's strategy-removal guard.
    pub fn get_share_balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_active_positions(&env).len() as i128
    }

    /// Return the total base-asset value of all LP positions.
    pub fn get_value(env: Env, vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        if vault != get_vault(&env) {
            return 0;
        }
        Self::compute_lp_value(&env)
    }

    // -----------------------------------------------------------------------
    // Metadata
    // -----------------------------------------------------------------------

    pub fn get_router(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_router(&env)
    }

    pub fn get_name(env: Env) -> String {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        storage::get_name(&env)
    }

    /// Return all LP token addresses with an active (non-zero) position.
    pub fn get_active_positions(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_active_positions(&env)
    }

    // -----------------------------------------------------------------------
    // Guard interface (called by vault)
    // -----------------------------------------------------------------------

    /// Return total base-asset value of all positions for `vault`.
    pub fn get_total_value(env: Env, vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        if vault != get_vault(&env) {
            return 0;
        }
        Self::compute_lp_value(&env)
    }

    /// Return underlying token balances across all active LP positions.
    ///
    /// For each active pair the strategy holds: computes
    /// `amount_asset = lp_balance * reserve_asset / total_lp`
    /// and sums across all positions per asset.
    ///
    /// The vault uses this to price all assets in a single batch call
    /// instead of triggering a full `get_total_value` (which nests factory +
    /// AssetHandler + oracle calls inside the strategy).
    pub fn get_underlying_asset_balances(env: Env, vault: Address) -> Map<Address, i128> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let mut out: Map<Address, i128> = Map::new(&env);
        if vault != get_vault(&env) {
            return out;
        }
        let active = get_active_positions(&env);
        for lp_token in active.iter() {
            let pos = match get_position(&env, &lp_token) {
                Some(p) if p.lp_balance > 0 => p,
                _ => continue,
            };
            let pair = PairAdapter::new(&env, &lp_token);
            let (r0, r1) = pair.get_reserves();
            let total_lp = pair.total_supply();
            if total_lp == 0 {
                continue;
            }
            let (ra, rb) = if pos.token0_is_asset_a {
                (r0, r1)
            } else {
                (r1, r0)
            };
            let amount_a = checked_mul_div(&env, pos.lp_balance, ra, total_lp);
            let amount_b = checked_mul_div(&env, pos.lp_balance, rb, total_lp);
            let prev_a = out.get(pos.asset_a.clone()).unwrap_or(0);
            let prev_b = out.get(pos.asset_b.clone()).unwrap_or(0);
            out.set(pos.asset_a, checked_add(&env, prev_a, amount_a));
            out.set(pos.asset_b, checked_add(&env, prev_b, amount_b));
        }
        out
    }

    /// Proportionally withdraw `numerator/denominator` of every active LP
    /// position and send underlying tokens directly to `to`.
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

        let active = get_active_positions(&env);
        if active.is_empty() {
            return;
        }

        let router = get_router(&env);
        let strategy = env.current_contract_address();
        let deadline = env.ledger().timestamp() + 300;
        let mut new_active: Vec<Address> = Vec::new(&env);

        for lp_token in active.iter() {
            let mut pos = match get_position(&env, &lp_token) {
                Some(p) if p.lp_balance > 0 => p,
                _ => continue,
            };

            let lp_amount = checked_mul_div(&env, pos.lp_balance, numerator, denominator);
            if lp_amount == 0 {
                new_active.push_back(lp_token.clone());
                continue;
            }

            // Same-ledger approval — no zero-revoke needed.
            let expiry = env.ledger().sequence();
            token::Client::new(&env, &lp_token).approve(&strategy, &router, &lp_amount, &expiry);

            RouterAdapter::new(&env, &router).remove_liquidity(
                pos.asset_a.clone(),
                pos.asset_b.clone(),
                lp_amount,
                0,
                0,
                to.clone(),
                deadline,
            );

            pos.lp_balance = checked_sub(&env, pos.lp_balance, lp_amount);
            set_position(&env, &lp_token, &pos);

            if pos.lp_balance > 0 {
                new_active.push_back(lp_token.clone());
            }
        }

        set_active_positions(&env, &new_active);
    }

    /// Return `true` if any active position involves `asset`.
    pub fn asset_in_use(env: Env, vault: Address, asset: Address) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if vault != get_vault(&env) {
            return false;
        }
        for lp_token in get_active_positions(&env).iter() {
            if let Some(pos) = get_position(&env, &lp_token) {
                if pos.lp_balance > 0 && (asset == pos.asset_a || asset == pos.asset_b) {
                    return true;
                }
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // Trader-callable operations (dispatched via vault.execute_op)
    //
    // Vault injects itself as the first argument and pre-funds the strategy
    // with required input tokens before dispatching.
    //
    // Args (caller-supplied, vault prepends itself):
    //   add_liquidity    → [lp_token, asset_a, asset_b, amount_a, amount_b, min_a, min_b]
    //   remove_liquidity → [lp_token, lp_amount, min_a, min_b]
    //   swap             → [from_asset, to_asset, amount_in, min_out]
    // -----------------------------------------------------------------------

    /// Add liquidity to the Soroswap pair identified by `lp_token`.
    ///
    /// On first call for a given `lp_token`, `pair.token0()` is called once to
    /// cache the reserve-to-asset mapping; subsequent calls use the cache.
    ///
    /// Returns `(amount_a_used, amount_b_used, lp_received)`.
    pub fn add_liquidity(
        env: Env,
        vault: Address,
        lp_token: Address,
        asset_a: Address,
        asset_b: Address,
        amount_a: i128,
        amount_b: i128,
        min_a: i128,
        min_b: i128,
    ) -> (i128, i128, i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount_a <= 0 || amount_b <= 0 || min_a < 0 || min_b < 0 {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, SoroswapLpError::NotVault);
        }

        let router = get_router(&env);
        let strategy = env.current_contract_address();
        let deadline = env.ledger().timestamp() + 300;
        let expiry = env.ledger().sequence(); // same-ledger approval

        // Resolve or create position for this pair.
        let mut position = get_position(&env, &lp_token).unwrap_or_else(|| {
            // First time this pair is used: call token0() once to cache ordering.
            let token0 = PairAdapter::new(&env, &lp_token).token0();
            LpPosition {
                asset_a: asset_a.clone(),
                asset_b: asset_b.clone(),
                lp_balance: 0,
                token0_is_asset_a: token0 == asset_a,
            }
        });

        // Validate assets match the cached position.
        if position.lp_balance > 0 && (position.asset_a != asset_a || position.asset_b != asset_b) {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }

        token::Client::new(&env, &asset_a).approve(&strategy, &router, &amount_a, &expiry);
        token::Client::new(&env, &asset_b).approve(&strategy, &router, &amount_b, &expiry);

        let (a_used, b_used, lp_received) = RouterAdapter::new(&env, &router).add_liquidity(
            asset_a.clone(),
            asset_b.clone(),
            amount_a,
            amount_b,
            min_a,
            min_b,
            strategy.clone(),
            deadline,
        );

        if a_used > amount_a || b_used > amount_b {
            panic_with_error!(&env, SoroswapLpError::Overflow);
        }

        // Return dust to vault.
        let dust_a = amount_a - a_used;
        let dust_b = amount_b - b_used;
        if dust_a > 0 {
            token::Client::new(&env, &asset_a).transfer(&strategy, &vault, &dust_a);
        }
        if dust_b > 0 {
            token::Client::new(&env, &asset_b).transfer(&strategy, &vault, &dust_b);
        }

        if lp_received <= 0 {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }

        let new_lp = checked_add(&env, position.lp_balance, lp_received);
        let actual_lp = token::Client::new(&env, &lp_token).balance(&strategy);
        if actual_lp < new_lp {
            if position.lp_balance == 0 {
                panic_with_error!(&env, SoroswapLpError::InvalidAmount);
            }
            panic_with_error!(&env, SoroswapLpError::Overflow);
        }

        position.lp_balance = new_lp;
        set_position(&env, &lp_token, &position);
        add_to_active_positions(&env, &lp_token);

        (a_used, b_used, lp_received)
    }

    /// Remove `lp_amount` from the pair identified by `lp_token`.
    ///
    /// Underlying tokens go directly to `vault`.
    /// Uses same-ledger approval (no zero-revoke).
    ///
    /// Returns `(amount_a, amount_b)` received from the router so the vault can
    /// compute post-operation value without a redundant deep LP revaluation.
    pub fn remove_liquidity(
        env: Env,
        vault: Address,
        lp_token: Address,
        asset_a: Address,
        asset_b: Address,
        lp_amount: i128,
        min_a: i128,
        min_b: i128,
    ) -> (i128, i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if lp_amount <= 0 || min_a < 0 || min_b < 0 {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, SoroswapLpError::NotVault);
        }

        let mut position = get_position(&env, &lp_token)
            .unwrap_or_else(|| panic_with_error!(&env, SoroswapLpError::InsufficientLpBalance));

        if position.asset_a != asset_a || position.asset_b != asset_b {
            panic_with_error!(&env, SoroswapLpError::InvalidAmount);
        }
        if lp_amount > position.lp_balance {
            panic_with_error!(&env, SoroswapLpError::InsufficientLpBalance);
        }

        let router = get_router(&env);
        let strategy = env.current_contract_address();
        let deadline = env.ledger().timestamp() + 300;
        let expiry = env.ledger().sequence(); // same-ledger, no zero-revoke needed

        token::Client::new(&env, &lp_token).approve(&strategy, &router, &lp_amount, &expiry);

        let (amount_a, amount_b) = RouterAdapter::new(&env, &router)
            .remove_liquidity(asset_a, asset_b, lp_amount, min_a, min_b, vault, deadline);

        position.lp_balance = checked_sub(&env, position.lp_balance, lp_amount);
        set_position(&env, &lp_token, &position);

        if position.lp_balance == 0 {
            remove_from_active_positions(&env, &lp_token);
        }

        (amount_a, amount_b)
    }

    /// Swap `amount_in` of `from_asset` for `to_asset` via Soroswap router.
    ///
    /// Output tokens go directly to `vault`.
    /// Uses same-ledger approval.
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

        let router = get_router(&env);
        let strategy = env.current_contract_address();
        let expiry = env.ledger().sequence();
        let deadline = env.ledger().timestamp() + 300;

        token::Client::new(&env, &from_asset).approve(&strategy, &router, &amount_in, &expiry);

        let path = soroban_sdk::vec![&env, from_asset, to_asset];
        RouterAdapter::new(&env, &router)
            .swap_exact_tokens_for_tokens(amount_in, min_out, path, vault, deadline);
    }

    // -----------------------------------------------------------------------
    // Oracle cache refresh (permissionless — reads only from vault's factory)
    // -----------------------------------------------------------------------

    /// Refresh the cached AssetHandler address from the vault's factory.
    ///
    /// Call this after the factory's AssetHandler is updated post-deployment.
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
    /// Budget profile (per call):
    /// * Pass 1 — storage reads only (no cross-contract calls): collect unique assets.
    /// * ONE `asset_handler.get_prices(Vec<Address>)` call for all assets.
    /// * Pass 2 — 2 cross-contract calls per position: `get_reserves` + `total_supply`.
    fn compute_lp_value(env: &Env) -> i128 {
        let active = get_active_positions(env);
        if active.is_empty() {
            return 0;
        }

        // Lazy-resolve and cache AssetHandler (avoids factory call on every valuation).
        let asset_handler = match get_or_cache_asset_handler(env) {
            Some(ah) => ah,
            None => panic_with_error!(env, SoroswapLpError::InvalidOraclePrice),
        };

        // Pass 1: collect unique assets from all active positions (storage reads only).
        let mut unique_assets: Vec<Address> = Vec::new(env);
        for lp_token in active.iter() {
            if let Some(pos) = get_position(env, &lp_token) {
                if pos.lp_balance > 0 {
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

        // ONE batch price call for all unique assets.
        let prices: Map<Address, i128> = env.invoke_contract(
            &asset_handler,
            &Symbol::new(env, "get_prices"),
            (unique_assets,).into_val(env),
        );

        // Pass 2: value each position (2 cross-contract calls per position).
        let mut total = 0i128;
        for lp_token in active.iter() {
            let pos = match get_position(env, &lp_token) {
                Some(p) if p.lp_balance > 0 => p,
                _ => continue,
            };

            let pair = PairAdapter::new(env, &lp_token);
            let (r0, r1) = pair.get_reserves();
            let total_lp = pair.total_supply();

            if total_lp == 0 || (r0 == 0 && r1 == 0) {
                panic_with_error!(env, SoroswapLpError::InvalidOraclePrice);
            }

            let (ra, rb) = if pos.token0_is_asset_a {
                (r0, r1)
            } else {
                (r1, r0)
            };

            let pa = prices
                .get(pos.asset_a.clone())
                .unwrap_or_else(|| panic_with_error!(env, SoroswapLpError::InvalidOraclePrice));
            let pb = prices
                .get(pos.asset_b.clone())
                .unwrap_or_else(|| panic_with_error!(env, SoroswapLpError::InvalidOraclePrice));

            if pa <= 0 || pb <= 0 {
                panic_with_error!(env, SoroswapLpError::InvalidOraclePrice);
            }

            let va = checked_mul_div(env, ra, pa, PRICE_PRECISION);
            let vb = checked_mul_div(env, rb, pb, PRICE_PRECISION);
            let pool_value = checked_add(env, va, vb);
            let pos_value = checked_mul_div(env, pos.lp_balance, pool_value, total_lp);
            total = checked_add(env, total, pos_value);
        }

        total
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
