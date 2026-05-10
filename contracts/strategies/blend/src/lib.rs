//! # Blend Strategy — Single-Asset Lending Position
//!
//! This contract supplies one asset (e.g. USDC) to the [Blend Protocol]
//! lending pool and earns yield from borrower interest.
//!
//! ## Position ownership
//! The **strategy contract itself** is the account holder inside Blend —
//! not the vault.  This design resolves the Soroban cross-contract
//! authorization chain: the strategy only needs its own auth when calling
//! Blend, and the vault only needs to pre-authorize a single
//! `token.transfer(vault → strategy)` sub-invocation before calling
//! `strategy.deposit`.
//!
//! ## Withdraw-to-user
//! `withdraw(amount, from, to)` calls
//! `blend_pool.submit(from=self, spender=self, to=to, [Withdraw])` so that
//! the underlying asset goes **directly** from Blend to the user address
//! without transiting the vault.
//!
//! ## Blend request types (subset used here)
//! | Value | Meaning |
//! |-------|---------|
//! | 0 | Supply collateral |
//! | 1 | Withdraw collateral |
//! | 2 | Supply |
//! | 3 | Withdraw |
//! | 4 | Borrow  ← **BLOCKED** |
//! | 5 | Repay   ← **BLOCKED** |

#![no_std]

mod error;
mod storage;

pub use error::BlendStrategyError;

use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, Address, Env, IntoVal, String, Symbol, Vec,
};

use storage::{
    get_asset, get_manager, get_name, get_paused, get_protocol, get_vault, is_initialized,
    set_asset, set_initialized, set_manager, set_name, set_paused, set_protocol, set_vault,
    INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
};

// ---------------------------------------------------------------------------
// Blend request type constants
// ---------------------------------------------------------------------------

/// Supply to the Blend pool (earns yield, no collateral obligation).
pub const REQUEST_SUPPLY: u32 = 2;
/// Withdraw previously supplied tokens.
pub const REQUEST_WITHDRAW: u32 = 3;

// ---------------------------------------------------------------------------
// Blend cross-contract client (minimal interface)
// ---------------------------------------------------------------------------
//
// We define a thin manual client instead of importing the full Blend WASM so
// that this crate compiles without an external file dependency.  In a
// production repo you would use `soroban_sdk::contractimport!` pointing at
// the Blend optimised WASM.

/// A single Blend operation bundled in a `submit` call.
#[soroban_sdk::contracttype]
#[derive(Clone)]
pub struct BlendRequest {
    /// One of the REQUEST_* constants above.
    pub request_type: u32,
    /// The asset address for this operation.
    pub address: Address,
    /// The token amount for this operation.
    pub amount: i128,
}

/// Call `blend_pool.submit(from, spender, to, requests)`.
///
/// In tests this dispatches to a [`MockBlendPool`] registered under the same
/// address; in production it dispatches to the real Blend contract.
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

/// Query the live on-chain supply position for `account` from the Blend pool.
///
/// Returns the actual token balance held in Blend, including accrued interest.
/// This is the authoritative source for NAV and withdrawal limit checks.
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

    /// Initialize a new Blend strategy instance.
    ///
    /// Must be called once by the deployer immediately after deployment.
    ///
    /// # Arguments
    /// * `vault`    – The vault contract that owns this strategy.
    /// * `asset`    – The token to supply to Blend (e.g. USDC).
    /// * `protocol` – The Blend pool contract address.
    /// * `manager`  – Address allowed to pause / unpause this strategy.
    /// * `name`     – Human-readable label (e.g. `"Blend USDC"`).
    ///
    /// # Auth
    /// Both the vault's current on-chain manager **and** the designated
    /// strategy `manager` must authorise this call.
    ///
    /// The vault manager auth is derived by cross-calling `vault.get_manager()`
    /// so it cannot be spoofed by a user-supplied argument.  This prevents an
    /// attacker from front-running deployment: supplying the real vault address
    /// requires forging the vault manager's signature, which is impossible;
    /// supplying their own vault wires the strategy to it instead of the
    /// victim's vault.
    ///
    /// # Errors
    /// * [`BlendStrategyError::AlreadyInitialized`] if called more than once.
    pub fn initialize(
        env: Env,
        vault: Address,
        asset: Address,
        protocol: Address,
        manager: Address,
        name: String,
    ) {
        if is_initialized(&env) {
            panic_with_error!(&env, BlendStrategyError::AlreadyInitialized);
        }

        // Fetch the vault's actual manager from on-chain state — this address
        // cannot be manipulated by the caller — and require their signature.
        // This binds initialization to the vault's trusted authority and
        // closes the front-running window that `manager.require_auth()` alone
        // leaves open (an attacker can always sign as their own manager).
        let vault_manager: Address = env.invoke_contract(
            &vault,
            &Symbol::new(&env, "get_manager"),
            ().into_val(&env),
        );
        vault_manager.require_auth();

        // The designated strategy manager also consents to taking on the role.
        manager.require_auth();
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        set_vault(&env, &vault);
        set_asset(&env, &asset);
        set_protocol(&env, &protocol);
        set_manager(&env, &manager);
        set_name(&env, &name);
        set_paused(&env, false);
        // Mark as initialized in persistent storage — survives instance TTL expiry
        // and prevents re-initialization after the instance entry expires.
        set_initialized(&env);
    }

    // -----------------------------------------------------------------------
    // Core strategy operations
    // -----------------------------------------------------------------------

    /// Deposit `amount` of the managed asset into Blend on behalf of `from`
    /// (the vault).
    ///
    /// ## Auth flow
    /// 1. The vault calls `env.authorize_as_current_contract(…)` to
    ///    pre-authorise `token.transfer(vault → strategy, amount)`.
    /// 2. The vault calls `strategy.deposit(amount, vault)`.
    /// 3. This function pulls the tokens from the vault to itself.
    /// 4. It then calls `blend.submit(from=self, spender=self, to=self, [Supply])`,
    ///    using only the strategy's own auth.
    ///
    /// # Returns
    /// The strategy's updated total position value (in asset units).
    ///
    /// # Errors
    /// * [`BlendStrategyError::NotInitialized`]
    /// * [`BlendStrategyError::NotVault`]       if `from` ≠ registered vault.
    /// * [`BlendStrategyError::Paused`]
    /// * [`BlendStrategyError::InvalidAmount`]  if `amount ≤ 0`.
    /// * [`BlendStrategyError::Overflow`]
    pub fn deposit(env: Env, amount: i128, from: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount <= 0 {
            panic_with_error!(&env, BlendStrategyError::InvalidAmount);
        }
        if get_paused(&env) {
            panic_with_error!(&env, BlendStrategyError::Paused);
        }

        from.require_auth();

        let vault = get_vault(&env);
        if from != vault {
            panic_with_error!(&env, BlendStrategyError::NotVault);
        }

        let asset = get_asset(&env);
        let protocol = get_protocol(&env);
        let strategy_addr = env.current_contract_address();

        // Pull tokens from vault into this strategy contract via allowance.
        // The vault approves `strategy_addr` in `vault.invest` before calling this.
        token::Client::new(&env, &asset).transfer_from(
            &strategy_addr,
            &from,
            &strategy_addr,
            &amount,
        );

        // Let the lending protocol pull supplied funds from this strategy.
        let expiry = env.ledger().sequence() + 100;
        token::Client::new(&env, &asset).approve(&strategy_addr, &protocol, &amount, &expiry);

        // Supply to Blend; strategy is the account holder.
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
            &protocol,
            &strategy_addr,
            &strategy_addr,
            &strategy_addr,
            requests,
        );

        // Revoke any residual allowance to the protocol so leftover approval
        // cannot be consumed by a future call or a compromised pool.
        let now = env.ledger().sequence();
        token::Client::new(&env, &asset).approve(&strategy_addr, &protocol, &0i128, &now);

        amount
    }

    /// Withdraw `amount` of the managed asset from Blend and send it directly
    /// to `to` (the user), bypassing the vault.
    ///
    /// ## Auth flow
    /// `from.require_auth()` is satisfied because the vault is the direct
    /// caller (contract invoker); no pre-authorisation is needed for
    /// withdrawal since tokens flow **out** of Blend (not out of the vault).
    ///
    /// # Returns
    /// The actual amount withdrawn (= `amount` unless Blend rounding applies).
    ///
    /// # Errors
    /// * [`BlendStrategyError::NotVault`]           if `from` ≠ registered vault.
    /// * [`BlendStrategyError::Paused`]
    /// * [`BlendStrategyError::InvalidAmount`]      if `amount ≤ 0`.
    /// * [`BlendStrategyError::InsufficientPosition`] if `amount` > position.
    pub fn withdraw(env: Env, amount: i128, from: Address, to: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount <= 0 {
            panic_with_error!(&env, BlendStrategyError::InvalidAmount);
        }
        if get_paused(&env) {
            panic_with_error!(&env, BlendStrategyError::Paused);
        }

        from.require_auth();

        let vault = get_vault(&env);
        if from != vault {
            panic_with_error!(&env, BlendStrategyError::NotVault);
        }

        let asset = get_asset(&env);
        let protocol = get_protocol(&env);
        let strategy_addr = env.current_contract_address();

        // Guard against over-withdrawal using the live on-chain position.
        // This includes accrued interest so users can always withdraw their
        // full earnings; the Blend pool enforces the same limit internally.
        let position = blend_get_supply(&env, &protocol, &strategy_addr);
        if amount > position {
            panic_with_error!(&env, BlendStrategyError::InsufficientPosition);
        }

        // Blend sends the asset directly to `to`.
        let requests: Vec<BlendRequest> = soroban_sdk::vec![
            &env,
            BlendRequest {
                request_type: REQUEST_WITHDRAW,
                address: asset.clone(),
                amount,
            }
        ];

        // Measure the balance delta to capture the actual amount Blend
        // delivers.  Blend's internal rounding may transfer slightly less
        // than the requested amount; returning the requested value instead
        // would cause the vault's share accounting to diverge from reality.
        let token_client = token::Client::new(&env, &asset);
        let balance_before = token_client.balance(&to);

        blend_submit(
            &env,
            &protocol,
            &strategy_addr,
            &strategy_addr,
            &to,
            requests,
        );

        let balance_after = token_client.balance(&to);
        let actual = balance_after.saturating_sub(balance_before);
        if actual <= 0 {
            panic_with_error!(&env, BlendStrategyError::InvalidAmount);
        }
        actual
    }

    // -----------------------------------------------------------------------
    // Valuation
    // -----------------------------------------------------------------------

    /// Return the strategy's current position value in asset units.
    ///
    /// Queries the live Blend pool position for this strategy contract so the
    /// value includes accrued interest and cannot be manipulated by the manager.
    /// The vault uses this value to compute NAV and share price.
    ///
    /// # Arguments
    /// * `_vault` – Reserved for future per-vault accounting; currently unused.
    pub fn get_value(env: Env, _vault: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let protocol = get_protocol(&env);
        let strategy_addr = env.current_contract_address();
        blend_get_supply(&env, &protocol, &strategy_addr)
    }

    // -----------------------------------------------------------------------
    // Metadata
    // -----------------------------------------------------------------------

    /// Return the address of the asset this strategy manages.
    pub fn asset(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_asset(&env)
    }

    /// Return the Blend pool address.
    pub fn get_protocol_address(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_protocol(&env)
    }

    /// Return the human-readable strategy name.
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

    // -----------------------------------------------------------------------
    // Emergency controls
    // -----------------------------------------------------------------------

    /// Pause the strategy, blocking all deposits and withdrawals.
    ///
    /// # Auth
    /// Manager must authorize this call.
    ///
    /// # Errors
    /// * [`BlendStrategyError::NotManager`] if caller ≠ registered manager.
    pub fn pause(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        let manager = get_manager(&env);
        if caller != manager {
            panic_with_error!(&env, BlendStrategyError::NotManager);
        }
        set_paused(&env, true);
    }

    /// Resume the strategy after a pause.
    ///
    /// # Auth
    /// Manager must authorize this call.
    ///
    /// # Errors
    /// * [`BlendStrategyError::NotManager`] if caller ≠ registered manager.
    pub fn unpause(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        let manager = get_manager(&env);
        if caller != manager {
            panic_with_error!(&env, BlendStrategyError::NotManager);
        }
        set_paused(&env, false);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
