//! # Phoenix LP Strategy — AMM Liquidity Position via Phoenix Protocol
//!
//! Mirrors the [`SoroswapLpStrategy`] but uses Phoenix Protocol's pool
//! interface, which differs in the following ways:
//!
//! * `provide_liquidity` takes a `depositor` parameter and **does not** return
//!   the amounts used; it just mints share tokens to the depositor.
//! * `withdraw_liquidity` takes a `recipient` and returns `(amount_a, amount_b)`,
//!   sending tokens directly to the recipient — which this strategy sets to the
//!   **user** address, not the vault.
//! * `auto_stake` is always `false` — the vault does not participate in Phoenix
//!   farming.
//!
//! ## Auth flow for `deposit_liquidity`
//! The vault pre-authorises `token_a.transfer(vault → strategy)` and
//! `token_b.transfer(vault → strategy)` before calling this function.

#![no_std]

mod error;
mod interfaces;
mod storage;

pub use error::PhoenixLpError;

use soroban_sdk::{contract, contractimpl, panic_with_error, token, Address, Env, IntoVal, String, Symbol};

use interfaces::{OracleAdapter, PhoenixPoolAdapter, Sep41TokenAdapter};

use storage::{
    get_asset_a, get_asset_b, get_manager, get_name, get_oracle, get_paused, get_phoenix_pool,
    get_share_token, get_total_shares, get_vault, is_initialized, set_asset_a, set_asset_b,
    set_manager, set_name, set_oracle, set_paused, set_phoenix_pool, set_share_token,
    set_total_shares, set_vault, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
};

/// Fixed-point precision matching the vault and oracle (7 decimal places).
const PRICE_PRECISION: i128 = 10_000_000;

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct PhoenixLpStrategy;

#[contractimpl]
impl PhoenixLpStrategy {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the Phoenix LP strategy.
    ///
    /// Automatically queries the share-token address from the Phoenix pool so
    /// the caller does not need to supply it separately.
    ///
    /// # Arguments
    /// * `vault`        – Owning vault contract.
    /// * `asset_a`      – First token in the pair.
    /// * `asset_b`      – Second token in the pair.
    /// * `phoenix_pool` – Phoenix pool contract address.
    /// * `manager`      – Address required **at initialization only** to prevent
    ///                    front-running.  All post-initialization admin operations
    ///                    (`pause`, `unpause`, `set_oracle`) are authorized by the
    ///                    vault's live on-chain manager, not this stored address.
    /// * `name`         – Human-readable strategy name.
    ///
    /// # Auth
    /// Both the vault's current on-chain manager and the designated strategy
    /// `manager` must authorise this call.  The `manager` parameter is required
    /// at initialization only to prevent front-running; after initialization the
    /// vault's live on-chain manager (from `vault.get_manager()`) controls all
    /// admin operations on this strategy.
    ///
    /// # Errors
    /// * [`PhoenixLpError::AlreadyInitialized`]
    pub fn initialize(
        env: Env,
        vault: Address,
        asset_a: Address,
        asset_b: Address,
        phoenix_pool: Address,
        manager: Address,
        name: String,
    ) {
        if is_initialized(&env) {
            panic_with_error!(&env, PhoenixLpError::AlreadyInitialized);
        }

        // The vault itself must authorize initialization so that only the real
        // vault (or a deployer acting on its behalf in the same transaction) can
        // wire this strategy to it.  A fake vault controlled by an attacker can
        // authorize itself, but a real vault can only be authorized by its own
        // manager — who must also sign below.
        vault.require_auth();

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

        // Auto-query share token from pool.
        let share_token = PhoenixPoolAdapter::new(&env, &phoenix_pool).query_share_token_address();

        set_vault(&env, &vault);
        set_asset_a(&env, &asset_a);
        set_asset_b(&env, &asset_b);
        set_phoenix_pool(&env, &phoenix_pool);
        set_share_token(&env, &share_token);
        set_manager(&env, &manager);
        set_name(&env, &name);
        set_paused(&env, false);
        set_total_shares(&env, 0);
    }

    // -----------------------------------------------------------------------
    // Core operations
    // -----------------------------------------------------------------------

    /// Provide liquidity to the Phoenix pool.
    ///
    /// The vault must pre-authorise both `token.transfer(vault → strategy)`
    /// calls before invoking this function.
    ///
    /// # Returns
    /// Phoenix share tokens credited to this strategy.
    ///
    /// # Errors
    /// * [`PhoenixLpError::NotVault`] / [`PhoenixLpError::Paused`] /
    ///   [`PhoenixLpError::InvalidAmount`]
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
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        if get_paused(&env) {
            panic_with_error!(&env, PhoenixLpError::Paused);
        }

        from.require_auth();
        if from != get_vault(&env) {
            panic_with_error!(&env, PhoenixLpError::NotVault);
        }

        let asset_a = get_asset_a(&env);
        let asset_b = get_asset_b(&env);
        let share_token = get_share_token(&env);
        let pool = get_phoenix_pool(&env);
        let strategy = env.current_contract_address();
        let deadline = env.ledger().timestamp() + 300;
        let expiry = env.ledger().sequence() + 100;

        // Pull tokens from vault.
        token::Client::new(&env, &asset_a).transfer(&from, &strategy, &amount_a);
        token::Client::new(&env, &asset_b).transfer(&from, &strategy, &amount_b);

        // Approve to pool.
        token::Client::new(&env, &asset_a).approve(&strategy, &pool, &amount_a, &expiry);
        token::Client::new(&env, &asset_b).approve(&strategy, &pool, &amount_b, &expiry);

        let shares_before = token::Client::new(&env, &share_token).balance(&strategy);

        PhoenixPoolAdapter::new(&env, &pool).provide_liquidity(
            strategy.clone(),
            Some(amount_a),
            Some(min_a),
            Some(amount_b),
            Some(min_b),
            None,
            Some(deadline),
        );

        // Revoke any unspent allowance so a later pool compromise cannot
        // drain tokens that the pool did not consume on this call.
        let zero = 0i128;
        let now = env.ledger().sequence();
        token::Client::new(&env, &asset_a).approve(&strategy, &pool, &zero, &now);
        token::Client::new(&env, &asset_b).approve(&strategy, &pool, &zero, &now);

        // Return residual tokens to the vault.  Phoenix pools commonly consume
        // fewer tokens than provided when the pool ratio forces one side to be
        // the binding constraint; the unused portion would otherwise be stranded
        // in this strategy contract forever.
        let client_a = token::Client::new(&env, &asset_a);
        let client_b = token::Client::new(&env, &asset_b);
        let residual_a = client_a.balance(&strategy);
        let residual_b = client_b.balance(&strategy);
        if residual_a > 0 {
            client_a.transfer(&strategy, &from, &residual_a);
        }
        if residual_b > 0 {
            client_b.transfer(&strategy, &from, &residual_b);
        }

        let shares_after = token::Client::new(&env, &share_token).balance(&strategy);

        // Guard against a misbehaving pool that burns or redirects shares.
        if shares_after < shares_before {
            panic_with_error!(&env, PhoenixLpError::Overflow);
        }
        let shares_minted = shares_after - shares_before;
        if shares_minted <= 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }

        let new_total = get_total_shares(&env)
            .checked_add(shares_minted)
            .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::Overflow));
        set_total_shares(&env, new_total);

        shares_minted
    }

    /// Withdraw from the Phoenix pool.
    ///
    /// Underlying tokens go **directly to `to`** (the user).
    ///
    /// # Returns
    /// `(amount_a, amount_b)` delivered to `to`.
    ///
    /// # Errors
    /// * [`PhoenixLpError::NotVault`] / [`PhoenixLpError::Paused`] /
    ///   [`PhoenixLpError::InvalidAmount`] / [`PhoenixLpError::InsufficientShares`]
    pub fn withdraw(
        env: Env,
        share_amount: i128,
        min_a: i128,
        min_b: i128,
        from: Address,
        to: Address,
    ) -> (i128, i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if share_amount <= 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        if get_paused(&env) {
            panic_with_error!(&env, PhoenixLpError::Paused);
        }

        from.require_auth();
        if from != get_vault(&env) {
            panic_with_error!(&env, PhoenixLpError::NotVault);
        }

        let share_token = get_share_token(&env);
        let pool = get_phoenix_pool(&env);
        let strategy = env.current_contract_address();

        // Use the live on-chain balance rather than the internal counter to cap
        // withdrawals; the counter can desync if shares arrive via direct transfer.
        let live_balance = token::Client::new(&env, &share_token).balance(&strategy);
        if share_amount > live_balance {
            panic_with_error!(&env, PhoenixLpError::InsufficientShares);
        }
        let expiry = env.ledger().sequence() + 100;
        let deadline = env.ledger().timestamp() + 300;

        // Approve share token to pool.
        token::Client::new(&env, &share_token).approve(&strategy, &pool, &share_amount, &expiry);

        let (amount_a, amount_b) = PhoenixPoolAdapter::new(&env, &pool).withdraw_liquidity(
            to,
            share_amount,
            min_a,
            min_b,
            Some(deadline),
        );

        // Revoke any residual share-token allowance after the pool call.
        let zero = 0i128;
        let now = env.ledger().sequence();
        token::Client::new(&env, &share_token).approve(&strategy, &pool, &zero, &now);

        let new_total = get_total_shares(&env)
            .checked_sub(share_amount)
            .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::Overflow));
        set_total_shares(&env, new_total);

        (amount_a, amount_b)
    }

    // -----------------------------------------------------------------------
    // Metadata / views
    // -----------------------------------------------------------------------

    /// Return the total Phoenix share tokens held by this strategy.
    pub fn get_share_balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_total_shares(&env)
    }

    /// Return the Phoenix share token balance held by this strategy.
    ///
    /// **Unit warning:** Without an oracle configured, this returns share-token
    /// units (not base-asset value). Set an oracle for reserve-decomposition NAV.
    pub fn get_value(env: Env, _vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        let pool = get_phoenix_pool(&env);
        let asset_a = get_asset_a(&env);
        let asset_b = get_asset_b(&env);
        let share_token = get_share_token(&env);
        let strategy = env.current_contract_address();

        // Use the actual on-chain share-token balance rather than the internal
        // counter to prevent desync if tokens are transferred directly to or
        // from this contract outside the normal deposit/withdraw flow.
        let shares = token::Client::new(&env, &share_token).balance(&strategy);
        if shares == 0 {
            return 0;
        }

        let oracle = match get_oracle(&env) {
            Some(o) => o,
            // Without an oracle the share units are not base-asset-denominated;
            // returning them as NAV would inflate share price.
            None => return 0,
        };

        // Pool reserves and total share supply.
        let (reserve_a, reserve_b) = PhoenixPoolAdapter::new(&env, &pool).get_reserves();
        let total_shares = Sep41TokenAdapter::new(&env, &share_token).total_supply();

        if total_shares == 0 || (reserve_a == 0 && reserve_b == 0) {
            // Cannot compute reserve decomposition — treat as zero NAV contribution.
            return 0;
        }

        let oracle_client = OracleAdapter::new(&env, &oracle);
        let price_a = oracle_client.get_price(&asset_a);
        let price_b = oracle_client.get_price(&asset_b);

        if price_a <= 0 || price_b <= 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidOraclePrice);
        }

        let pool_value_a = reserve_a
            .checked_mul(price_a)
            .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::Overflow))
            / PRICE_PRECISION;
        let pool_value_b = reserve_b
            .checked_mul(price_b)
            .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::Overflow))
            / PRICE_PRECISION;
        let pool_value = pool_value_a
            .checked_add(pool_value_b)
            .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::Overflow));

        // position_value = pool_value * shares / total_shares
        pool_value
            .checked_mul(shares)
            .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::Overflow))
            / total_shares
    }

    pub fn asset_a(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_asset_a(&env)
    }
    pub fn asset_b(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_asset_b(&env)
    }
    pub fn share_token(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_share_token(&env)
    }
    pub fn get_name(env: Env) -> String {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_name(&env)
    }
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
    // Oracle configuration
    // -----------------------------------------------------------------------

    /// Set the oracle used for reserve-decomposition NAV.
    ///
    /// **Vault manager only** — authorized via `vault.get_manager()` on-chain.
    pub fn set_oracle(env: Env, caller: Address, oracle: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        let vault = get_vault(&env);
        let vault_manager: Address = env.invoke_contract(
            &vault,
            &Symbol::new(&env, "get_manager"),
            ().into_val(&env),
        );
        if caller != vault_manager {
            panic_with_error!(&env, PhoenixLpError::NotManager);
        }
        set_oracle(&env, &oracle);
    }

    // -----------------------------------------------------------------------
    // Emergency controls
    // -----------------------------------------------------------------------

    /// Pause the strategy.
    ///
    /// **Vault manager only** — authorized via `vault.get_manager()` on-chain.
    pub fn pause(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        let vault = get_vault(&env);
        let vault_manager: Address = env.invoke_contract(
            &vault,
            &Symbol::new(&env, "get_manager"),
            ().into_val(&env),
        );
        if caller != vault_manager {
            panic_with_error!(&env, PhoenixLpError::NotManager);
        }
        set_paused(&env, true);
    }

    /// Unpause the strategy.
    ///
    /// **Vault manager only** — authorized via `vault.get_manager()` on-chain.
    pub fn unpause(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        let vault = get_vault(&env);
        let vault_manager: Address = env.invoke_contract(
            &vault,
            &Symbol::new(&env, "get_manager"),
            ().into_val(&env),
        );
        if caller != vault_manager {
            panic_with_error!(&env, PhoenixLpError::NotManager);
        }
        set_paused(&env, false);
    }
}

#[cfg(test)]
mod test;
