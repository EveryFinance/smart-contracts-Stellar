//! # Blend Strategy — Multi-Position Lending Manager
//!
//! One deployed instance manages **all** Blend lending positions for the
//! owning vault across any number of Blend pools and assets.
//! Positions are keyed by pool address; each pool holds one supplied asset.
//!
//! ## Budget optimisations
//! * `AssetHandler` is lazy-cached on first valuation.
//! * `get_total_value` calls `asset_handler.get_prices(assets)` once for
//!   all unique lending assets instead of one `get_price` per pool.
//! * `supply` uses same-ledger approval; no zero-revoke needed for Blend
//!   since the pool consumes the full approved amount during submit.
//! * After a full withdrawal from a pool, the pool is removed from
//!   `ActivePositions` so valuation loops stay tight.
//!
//! ## Arg conventions for vault.execute_op dispatch
//! ```text
//! supply                args: [pool, asset, amount]
//! withdraw_from_lending args: [pool, asset, amount]
//! ```

#![no_std]

mod error;
mod storage;

pub use error::BlendStrategyError;

use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, Address, Env, IntoVal, Map, String, Symbol,
    Vec,
};

use storage::{
    add_to_active_positions, get_active_positions, get_name, get_or_cache_asset_handler,
    get_position, get_vault, is_initialized, remove_from_active_positions, set_asset_handler,
    set_factory, set_initialized, set_name, set_position, set_vault, LendingPosition,
    INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
};

// ---------------------------------------------------------------------------
// Blend request type constants
// ---------------------------------------------------------------------------

pub const REQUEST_SUPPLY: u32 = 2;
pub const REQUEST_WITHDRAW: u32 = 3;

const PRICE_PRECISION: i128 = 10_000_000;

fn checked_mul_div(env: &Env, a: i128, b: i128, denominator: i128) -> i128 {
    if denominator <= 0 {
        panic_with_error!(env, BlendStrategyError::InvalidAmount);
    }
    a.checked_mul(b)
        .map(|v| v / denominator)
        .unwrap_or_else(|| panic_with_error!(env, BlendStrategyError::Overflow))
}

// ---------------------------------------------------------------------------
// Blend cross-contract helpers
// ---------------------------------------------------------------------------

#[soroban_sdk::contracttype]
#[derive(Clone)]
pub struct BlendRequest {
    pub request_type: u32,
    pub address: Address,
    pub amount: i128,
}

fn blend_submit(
    env: &Env,
    pool: &Address,
    from: &Address,
    spender: &Address,
    to: &Address,
    requests: Vec<BlendRequest>,
) {
    let args = (from.clone(), spender.clone(), to.clone(), requests).into_val(env);
    env.invoke_contract::<()>(pool, &Symbol::new(env, "submit"), args);
}

fn blend_get_supply(env: &Env, pool: &Address, account: &Address) -> i128 {
    let args = (account.clone(),).into_val(env);
    env.invoke_contract::<i128>(pool, &Symbol::new(env, "get_supply"), args)
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct BlendStrategy;

#[contractimpl]
impl BlendStrategy {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the strategy for a vault.
    ///
    /// No pool- or asset-specific arguments — positions are registered
    /// dynamically on the first `supply` call for each pool.
    pub fn initialize(env: Env, vault: Address, name: String) {
        if is_initialized(&env) {
            panic_with_error!(&env, BlendStrategyError::AlreadyInitialized);
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

    /// Return the total base-asset value of all positions for `vault`.
    ///
    /// Budget profile:
    /// * 1 `blend_get_supply` call per active pool (unavoidable — Blend tracks state)
    /// * 1 `get_prices` batch call for all unique lending assets
    pub fn get_total_value(env: Env, vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if vault != get_vault(&env) {
            return 0;
        }

        let active = get_active_positions(&env);
        if active.is_empty() {
            return 0;
        }

        let strategy_addr = env.current_contract_address();

        // Pass 1: collect (pool, balance) pairs and unique assets.
        let mut pool_balances: Vec<(Address, Address, i128)> = Vec::new(&env);
        let mut unique_assets: Vec<Address> = Vec::new(&env);

        for pool in active.iter() {
            let pos = match get_position(&env, &pool) {
                Some(p) => p,
                None => continue,
            };
            let balance = blend_get_supply(&env, &pool, &strategy_addr);
            if balance == 0 {
                continue;
            }
            if !unique_assets.contains(pos.asset.clone()) {
                unique_assets.push_back(pos.asset.clone());
            }
            pool_balances.push_back((pool, pos.asset, balance));
        }

        if pool_balances.is_empty() {
            return 0;
        }

        // Lazy-resolve AssetHandler for price lookup.
        let asset_handler = match get_or_cache_asset_handler(&env) {
            Some(ah) => ah,
            None => {
                // No AssetHandler — return raw token sum (backwards compatible).
                let mut raw_total: i128 = 0;
                for tup in pool_balances.iter() {
                    let (_, _, bal) = tup;
                    raw_total = raw_total.saturating_add(bal);
                }
                return raw_total;
            }
        };

        // ONE batch price call for all unique assets.
        let prices: Map<Address, i128> = env.invoke_contract(
            &asset_handler,
            &Symbol::new(&env, "get_prices"),
            (unique_assets,).into_val(&env),
        );

        // Pass 2: value each position.
        let mut total: i128 = 0;
        for tup in pool_balances.iter() {
            let (_, asset, balance) = tup;
            let price = prices.get(asset.clone()).unwrap_or(0);
            if price <= 0 {
                continue; // skip pools with no price rather than failing NAV
            }
            total = total.saturating_add(checked_mul_div(&env, balance, price, PRICE_PRECISION));
        }

        total
    }

    /// Return underlying token balances across all active Blend lending positions.
    ///
    /// For each active pool: returns the lent token balance (queried from the
    /// Blend pool). The vault uses this to batch-price all assets without
    /// nesting factory + oracle calls inside the strategy.
    pub fn get_underlying_asset_balances(env: Env, vault: Address) -> Map<Address, i128> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let mut out: Map<Address, i128> = Map::new(&env);
        if vault != get_vault(&env) {
            return out;
        }
        let active = get_active_positions(&env);
        let strategy_addr = env.current_contract_address();
        for pool in active.iter() {
            let pos = match get_position(&env, &pool) {
                Some(p) => p,
                None => continue,
            };
            let balance = blend_get_supply(&env, &pool, &strategy_addr);
            if balance == 0 {
                continue;
            }
            let prev = out.get(pos.asset.clone()).unwrap_or(0);
            out.set(
                pos.asset,
                prev.checked_add(balance)
                    .unwrap_or_else(|| panic_with_error!(&env, BlendStrategyError::Overflow)),
            );
        }
        out
    }

    /// Return the value of all positions (alias — same as `get_total_value`).
    pub fn get_value(env: Env, vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        if vault != get_vault(&env) {
            return 0;
        }
        Self::compute_raw_total(&env)
    }

    /// Return the number of active pools (> 0 means strategy holds positions).
    pub fn get_share_balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_active_positions(&env).len() as i128
    }

    /// Return all pool addresses with a (potentially) non-zero supply balance.
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
            panic_with_error!(&env, BlendStrategyError::NotVault);
        }
        if numerator <= 0 || denominator <= 0 || numerator > denominator {
            panic_with_error!(&env, BlendStrategyError::InvalidAmount);
        }

        let active = get_active_positions(&env);
        if active.is_empty() {
            return;
        }

        let strategy_addr = env.current_contract_address();
        let mut exhausted: Vec<Address> = Vec::new(&env);

        for pool in active.iter() {
            let pos = match get_position(&env, &pool) {
                Some(p) => p,
                None => continue,
            };
            let position = blend_get_supply(&env, &pool, &strategy_addr);
            if position == 0 {
                exhausted.push_back(pool.clone());
                continue;
            }

            let amount = checked_mul_div(&env, position, numerator, denominator);
            if amount == 0 {
                continue;
            }

            let requests: Vec<BlendRequest> = soroban_sdk::vec![
                &env,
                BlendRequest {
                    request_type: REQUEST_WITHDRAW,
                    address: pos.asset,
                    amount,
                }
            ];
            blend_submit(&env, &pool, &strategy_addr, &strategy_addr, &to, requests);

            // Remove from active if fully withdrawn.
            if amount >= position {
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

        let strategy_addr = env.current_contract_address();
        for pool in get_active_positions(&env).iter() {
            if let Some(pos) = get_position(&env, &pool) {
                if pos.asset == asset {
                    if blend_get_supply(&env, &pool, &strategy_addr) > 0 {
                        return true;
                    }
                }
            }
        }
        false
    }

    // -----------------------------------------------------------------------
    // Trader-callable operations (dispatched via vault.execute_op)
    //
    // Args: vault (injected), pool, asset, amount
    // -----------------------------------------------------------------------

    /// Supply `amount` of `asset` to the Blend `pool`.
    ///
    /// The vault pre-funds the strategy with the tokens before calling this.
    pub fn supply(env: Env, vault: Address, pool: Address, asset: Address, amount: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount <= 0 {
            panic_with_error!(&env, BlendStrategyError::InvalidAmount);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, BlendStrategyError::NotVault);
        }

        let strategy_addr = env.current_contract_address();
        // Same-ledger approval — no zero-revoke needed; Blend consumes the full amount.
        let expiry = env.ledger().sequence();
        token::Client::new(&env, &asset).approve(&strategy_addr, &pool, &amount, &expiry);

        let requests: Vec<BlendRequest> = soroban_sdk::vec![
            &env,
            BlendRequest {
                request_type: REQUEST_SUPPLY,
                address: asset.clone(),
                amount,
            }
        ];
        blend_submit(
            &env,
            &pool,
            &strategy_addr,
            &strategy_addr,
            &strategy_addr,
            requests,
        );

        // Register this pool in the active index if first supply.
        if get_position(&env, &pool).is_none() {
            set_position(&env, &pool, &LendingPosition { asset });
        }
        add_to_active_positions(&env, &pool);
    }

    /// Withdraw `amount` from Blend `pool` and return tokens to `vault`.
    pub fn withdraw_from_lending(
        env: Env,
        vault: Address,
        pool: Address,
        asset: Address,
        amount: i128,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount <= 0 {
            panic_with_error!(&env, BlendStrategyError::InvalidAmount);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, BlendStrategyError::NotVault);
        }

        let strategy_addr = env.current_contract_address();
        let position = blend_get_supply(&env, &pool, &strategy_addr);
        if amount > position {
            panic_with_error!(&env, BlendStrategyError::InsufficientPosition);
        }

        let requests: Vec<BlendRequest> = soroban_sdk::vec![
            &env,
            BlendRequest {
                request_type: REQUEST_WITHDRAW,
                address: asset,
                amount,
            }
        ];
        blend_submit(
            &env,
            &pool,
            &strategy_addr,
            &strategy_addr,
            &vault,
            requests,
        );

        // Remove from active if fully withdrawn.
        if amount >= position {
            remove_from_active_positions(&env, &pool);
        }
    }

    // -----------------------------------------------------------------------
    // Legacy investment interface (kept for backwards compatibility)
    // Vault.invest / vault.redeem may call these directly.
    // -----------------------------------------------------------------------

    /// Deposit `amount` of `asset` into Blend `pool` on behalf of `from` (vault).
    ///
    /// Pulls tokens from `from` via transfer_from (requires pre-approval).
    pub fn deposit(env: Env, amount: i128, pool: Address, asset: Address, from: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount <= 0 {
            panic_with_error!(&env, BlendStrategyError::InvalidAmount);
        }
        from.require_auth();
        if from != get_vault(&env) {
            panic_with_error!(&env, BlendStrategyError::NotVault);
        }

        let strategy_addr = env.current_contract_address();
        token::Client::new(&env, &asset).transfer_from(
            &strategy_addr,
            &from,
            &strategy_addr,
            &amount,
        );

        let expiry = env.ledger().sequence();
        token::Client::new(&env, &asset).approve(&strategy_addr, &pool, &amount, &expiry);

        let requests: Vec<BlendRequest> = soroban_sdk::vec![
            &env,
            BlendRequest {
                request_type: REQUEST_SUPPLY,
                address: asset.clone(),
                amount,
            }
        ];
        blend_submit(
            &env,
            &pool,
            &strategy_addr,
            &strategy_addr,
            &strategy_addr,
            requests,
        );

        if get_position(&env, &pool).is_none() {
            set_position(&env, &pool, &LendingPosition { asset });
        }
        add_to_active_positions(&env, &pool);

        amount
    }

    /// Withdraw `amount` of `asset` from Blend `pool`, sending tokens to `to`.
    pub fn withdraw(
        env: Env,
        amount: i128,
        pool: Address,
        asset: Address,
        from: Address,
        to: Address,
    ) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount <= 0 {
            panic_with_error!(&env, BlendStrategyError::InvalidAmount);
        }
        from.require_auth();
        if from != get_vault(&env) {
            panic_with_error!(&env, BlendStrategyError::NotVault);
        }

        let strategy_addr = env.current_contract_address();
        let position = blend_get_supply(&env, &pool, &strategy_addr);
        if amount > position {
            panic_with_error!(&env, BlendStrategyError::InsufficientPosition);
        }

        let token_client = token::Client::new(&env, &asset);
        let balance_before = token_client.balance(&to);

        let requests: Vec<BlendRequest> = soroban_sdk::vec![
            &env,
            BlendRequest {
                request_type: REQUEST_WITHDRAW,
                address: asset,
                amount,
            }
        ];
        blend_submit(&env, &pool, &strategy_addr, &strategy_addr, &to, requests);

        let balance_after = token_client.balance(&to);
        let actual = balance_after.saturating_sub(balance_before);
        if actual <= 0 {
            panic_with_error!(&env, BlendStrategyError::InvalidAmount);
        }

        if amount >= position {
            remove_from_active_positions(&env, &pool);
        }

        actual
    }

    // -----------------------------------------------------------------------
    // Refresh oracle cache (permissionless)
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
    // Private helpers
    // -----------------------------------------------------------------------

    /// Raw token sum across all active positions (no price conversion).
    fn compute_raw_total(env: &Env) -> i128 {
        let strategy_addr = env.current_contract_address();
        let mut total: i128 = 0;
        for pool in get_active_positions(env).iter() {
            total = total.saturating_add(blend_get_supply(env, &pool, &strategy_addr));
        }
        total
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
