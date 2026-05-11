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

use interfaces::{PhoenixPoolAdapter, Sep41TokenAdapter};

use storage::{
    get_asset_a, get_asset_b, get_factory, get_name, get_paused, get_phoenix_pool,
    get_share_token, get_total_shares, get_vault, is_initialized, set_asset_a, set_asset_b,
    set_factory, set_manager, set_name, set_paused, set_phoenix_pool, set_share_token,
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

        // Cache factory locally: get_total_value must not call back into vault
        // (re-entry), so we resolve vault → factory once during initialization.
        let factory_opt: Option<Address> = env.invoke_contract(
            &vault,
            &Symbol::new(&env, "get_factory"),
            ().into_val(&env),
        );
        if let Some(ref factory) = factory_opt {
            set_factory(&env, factory);
        }
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

    /// Return the total base-asset value of this vault's LP position.
    ///
    /// Uses reserve decomposition with prices from the vault's AssetHandler.
    /// Returns 0 if the vault has no AssetHandler configured.
    pub fn get_value(env: Env, vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        Self::compute_lp_value(&env, &vault)
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

    // -----------------------------------------------------------------------
    // Multi-asset guard interface v2
    // -----------------------------------------------------------------------

    /// Return the total base-asset value of this vault's LP position.
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

    /// Withdraw `numerator/denominator` fraction of LP position directly to `to`.
    ///
    /// Calls `pool.withdraw_liquidity(to, share_amount, …)` so tokens go
    /// directly to the user.
    ///
    /// # Errors
    /// * [`PhoenixLpError::NotVault`] if `vault` ≠ registered vault.
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
        if numerator <= 0 || denominator <= 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }

        let share_token = get_share_token(&env);
        let strategy = env.current_contract_address();
        let live_balance = token::Client::new(&env, &share_token).balance(&strategy);
        if live_balance == 0 {
            return;
        }

        let share_amount = live_balance * numerator / denominator;
        if share_amount == 0 {
            return;
        }

        let pool = get_phoenix_pool(&env);
        let expiry = env.ledger().sequence() + 100;
        let deadline = env.ledger().timestamp() + 300;

        token::Client::new(&env, &share_token).approve(&strategy, &pool, &share_amount, &expiry);
        PhoenixPoolAdapter::new(&env, &pool)
            .withdraw_liquidity(to, share_amount, 0, 0, Some(deadline));
        let now = env.ledger().sequence();
        token::Client::new(&env, &share_token).approve(&strategy, &pool, &0i128, &now);
    }

    /// Return `true` if this strategy has a non-zero position for `vault`
    /// involving `asset` (either token_a or token_b).
    pub fn asset_in_use(env: Env, vault: Address, asset: Address) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if vault != get_vault(&env) {
            return false;
        }
        let share_token = get_share_token(&env);
        let strategy = env.current_contract_address();
        if token::Client::new(&env, &share_token).balance(&strategy) == 0 {
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
    // Production note: `add_liquidity` uses `transfer_from` which requires
    // the vault to have pre-approved this strategy for each token amount.
    // In tests `mock_all_auths()` bypasses the approval check.
    //
    // Fees on Phoenix V2-style pools are compounded into share token value —
    // no separate collect_fees step is required.
    // -----------------------------------------------------------------------

    /// Add liquidity to the Phoenix pool on behalf of `vault`.
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

        if amount_a <= 0 || amount_b <= 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        if get_paused(&env) {
            panic_with_error!(&env, PhoenixLpError::Paused);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, PhoenixLpError::NotVault);
        }

        let asset_a = get_asset_a(&env);
        let asset_b = get_asset_b(&env);
        let share_token = get_share_token(&env);
        let pool = get_phoenix_pool(&env);
        let strategy = env.current_contract_address();
        let expiry = env.ledger().sequence() + 100;
        let deadline = env.ledger().timestamp() + 300;

        // Pull both tokens from vault into strategy.
        token::Client::new(&env, &asset_a).transfer_from(
            &strategy, &vault, &strategy, &amount_a,
        );
        token::Client::new(&env, &asset_b).transfer_from(
            &strategy, &vault, &strategy, &amount_b,
        );

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

        // Revoke residual allowances.
        let zero = 0i128;
        let now = env.ledger().sequence();
        token::Client::new(&env, &asset_a).approve(&strategy, &pool, &zero, &now);
        token::Client::new(&env, &asset_b).approve(&strategy, &pool, &zero, &now);

        // Return any residual tokens to vault.
        let client_a = token::Client::new(&env, &asset_a);
        let client_b = token::Client::new(&env, &asset_b);
        let residual_a = client_a.balance(&strategy);
        let residual_b = client_b.balance(&strategy);
        if residual_a > 0 {
            client_a.transfer(&strategy, &vault, &residual_a);
        }
        if residual_b > 0 {
            client_b.transfer(&strategy, &vault, &residual_b);
        }

        let shares_after = token::Client::new(&env, &share_token).balance(&strategy);
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
    }

    /// Remove `share_amount` from the Phoenix pool and send tokens to `vault`.
    ///
    /// Called via `vault.execute_op(caller, strategy, "remove_liquidity",
    ///   [share_amount, min_a, min_b])`.
    pub fn remove_liquidity(
        env: Env,
        vault: Address,
        share_amount: i128,
        min_a: i128,
        min_b: i128,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if share_amount <= 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        if get_paused(&env) {
            panic_with_error!(&env, PhoenixLpError::Paused);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, PhoenixLpError::NotVault);
        }

        let share_token = get_share_token(&env);
        let pool = get_phoenix_pool(&env);
        let strategy = env.current_contract_address();

        let live_balance = token::Client::new(&env, &share_token).balance(&strategy);
        if share_amount > live_balance {
            panic_with_error!(&env, PhoenixLpError::InsufficientShares);
        }

        let expiry = env.ledger().sequence() + 100;
        let deadline = env.ledger().timestamp() + 300;

        token::Client::new(&env, &share_token).approve(&strategy, &pool, &share_amount, &expiry);

        // Underlying tokens go directly to vault.
        PhoenixPoolAdapter::new(&env, &pool).withdraw_liquidity(
            vault,
            share_amount,
            min_a,
            min_b,
            Some(deadline),
        );

        let now = env.ledger().sequence();
        token::Client::new(&env, &share_token).approve(&strategy, &pool, &0i128, &now);

        let new_total = get_total_shares(&env)
            .checked_sub(share_amount)
            .unwrap_or_else(|| panic_with_error!(&env, PhoenixLpError::Overflow));
        set_total_shares(&env, new_total);
    }

    /// Swap within the Phoenix pool on behalf of `vault`.
    ///
    /// `sell_a = true` sells asset A for asset B; `sell_a = false` does the reverse.
    /// Validation: `amount_in > 0`, `min_out >= 0`, caller must be the registered vault.
    ///
    /// Called via `vault.execute_op(caller, strategy, "swap",
    ///   [sell_a, amount_in, min_out])`.
    pub fn swap(
        env: Env,
        vault: Address,
        sell_a: bool,
        amount_in: i128,
        min_out: i128,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount_in <= 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        if min_out < 0 {
            panic_with_error!(&env, PhoenixLpError::InvalidAmount);
        }
        if get_paused(&env) {
            panic_with_error!(&env, PhoenixLpError::Paused);
        }
        if vault != get_vault(&env) {
            panic_with_error!(&env, PhoenixLpError::NotVault);
        }

        let asset_a = get_asset_a(&env);
        let asset_b = get_asset_b(&env);
        let offer_asset = if sell_a { asset_a } else { asset_b };
        let pool = get_phoenix_pool(&env);
        let strategy = env.current_contract_address();
        let expiry = env.ledger().sequence() + 100;
        let deadline = env.ledger().timestamp() + 300;

        // Pull offer token from vault into strategy.
        token::Client::new(&env, &offer_asset).transfer_from(
            &strategy, &vault, &strategy, &amount_in,
        );
        token::Client::new(&env, &offer_asset).approve(&strategy, &pool, &amount_in, &expiry);

        // Swap; ask token goes directly to vault.
        PhoenixPoolAdapter::new(&env, &pool).swap(
            strategy.clone(),
            vault,
            sell_a,
            amount_in,
            min_out,
            None,
            Some(deadline),
        );

        // Revoke residual allowance.
        let now = env.ledger().sequence();
        token::Client::new(&env, &offer_asset).approve(&strategy, &pool, &0i128, &now);
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

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn compute_lp_value(env: &Env, vault: &Address) -> i128 {
        let pool = get_phoenix_pool(env);
        let asset_a = get_asset_a(env);
        let asset_b = get_asset_b(env);
        let share_token = get_share_token(env);
        let strategy = env.current_contract_address();

        let shares = token::Client::new(env, &share_token).balance(&strategy);
        if shares == 0 {
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
        let asset_handler = match asset_handler_opt {
            Some(ah) => ah,
            None => return 0,
        };

        let (reserve_a, reserve_b) = PhoenixPoolAdapter::new(env, &pool).get_reserves();
        let total_shares = Sep41TokenAdapter::new(env, &share_token).total_supply();
        if total_shares == 0 || (reserve_a == 0 && reserve_b == 0) {
            return 0;
        }

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
            return 0;
        }

        let pool_value_a = reserve_a.saturating_mul(price_a) / PRICE_PRECISION;
        let pool_value_b = reserve_b.saturating_mul(price_b) / PRICE_PRECISION;
        let pool_value = pool_value_a.saturating_add(pool_value_b);
        pool_value.saturating_mul(shares) / total_shares
    }
}

#[cfg(test)]
mod test;
