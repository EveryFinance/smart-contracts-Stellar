//! # Vault — On-Chain Asset Management Core
//!
//! The Vault is the central contract of the asset management protocol.  It
//! accepts deposits in a single **base asset** (e.g. USDC), issues pro-rata
//! **share tokens** (SEP-41) to depositors, and gives a designated **manager**
//! the ability to:
//!
//! * **Invest** idle base-asset funds into whitelisted **strategy** contracts
//!   (Blend lending, Soroswap LP, Phoenix LP, …).
//! * **Unwind** positions from strategy contracts back to the vault.
//! * **Execute spot trades** through whitelisted strategy-level **trade guards**
//!   that enforce slippage and token whitelist policies.
//!
//! ## Share-price accounting
//! Share price is derived from **Net Asset Value (NAV)**:
//! ```text
//! NAV            = vault_base_balance + sum(strategy.get_value(vault))
//! share_price    = NAV / total_supply      (in PRICE_PRECISION units)
//! shares_minted  = deposit_net / share_price
//! base_returned  = shares_burned * share_price
//! ```
//!
//! ## Fee model
//! | Fee        | Trigger        | Recipient        |
//! |------------|----------------|------------------|
//! | Entry fee  | deposit        | manager          |
//! | Exit fee   | withdraw       | stays in vault   |
//! | Mgmt fee   | any deposit /  | manager (shares) |
//! |            | withdraw call  |                  |
//! | Perf fee   | any deposit /  | manager (shares) |
//! |            | withdraw call  |                  |
//!
//! Management fee is streamed continuously (accrued since last collection),
//! and performance fee uses a **high-water mark** per share.
//!
//! ## Auth model
//! * `initialize`    — one-time setup; `manager` and `trader` must authorize.
//! * `deposit`       — any user.
//! * `withdraw`      — share-holder (caller must own shares).
//! * `invest`        — manager only.
//! * `unwind`        — manager only.
//! * `execute_trade` — trader only.
//! * `set_strategies` / `set_trade_guard` / `pause` / `unpause` — manager only.

#![no_std]

mod error;
mod events;
mod storage;

pub use error::VaultError;

use soroban_sdk::{
    contract, contractimpl, contracttype, panic_with_error, token, Address, Env, IntoVal, String,
    Symbol, Vec,
};

use storage::{
    clear_announced_fees, clear_op_state, get_announced_entry_fee_bps, get_announced_exit_fee_bps,
    get_announced_fee_activation_ts, get_announced_mgmt_fee_bps, get_announced_perf_fee_bps,
    get_base_asset, get_deposit_cap, get_entry_fee_bps, get_exit_cooldown_secs, get_exit_fee_bps,
    get_high_water_mark, get_last_deposit_ts_opt, get_last_mgmt_fee_ts, get_manager,
    get_max_concentration_bps, get_max_loss_bps, get_mgmt_fee_bps, get_op_state, get_oracle,
    get_paused, get_perf_fee_bps, get_private_pool, get_share_token, get_strategies,
    get_strategy_price_token, get_trade_guard, get_trader, get_value_manipulation_guard_enabled,
    is_initialized, is_lp_strategy, is_member, set_announced_entry_fee_bps,
    set_announced_exit_fee_bps, set_announced_fee_activation_ts, set_announced_mgmt_fee_bps,
    set_announced_perf_fee_bps, set_base_asset, set_deposit_cap, set_entry_fee_bps,
    set_exit_cooldown_secs, set_exit_fee_bps, set_high_water_mark, set_last_deposit_ts,
    set_last_mgmt_fee_ts, set_lp_strategy, set_manager, set_max_concentration_bps,
    set_max_loss_bps, set_member, set_mgmt_fee_bps, set_op_state, set_oracle, set_paused,
    set_perf_fee_bps, set_private_pool, set_share_token, set_strategies, set_strategy_price_token,
    set_trade_guard, set_trader, set_value_manipulation_guard_enabled, OperationState,
    FEE_DENOMINATOR, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD, MAX_ENTRY_EXIT_FEE_BPS,
    MAX_MGMT_FEE_BPS, MAX_PERF_FEE_BPS, SECONDS_PER_YEAR,
};

use events::{
    deposit_event, invest_event, manager_changed_event, mgmt_fee_event, pause_event,
    perf_fee_event, strategy_set_event, trade_event, trader_changed_event, unwind_event,
    withdraw_event,
};

/// Fixed-point precision for share-price calculations (1e7 = Stellar decimals).
const PRICE_PRECISION: i128 = 10_000_000;
/// Default NAV-loss guard tolerance (10%) for manager operations.
const DEFAULT_MAX_LOSS_BPS: u32 = 1_000;
/// Delay before announced fee increases can be committed.
const FEE_INCREASE_DELAY_SECS: u64 = 86_400;

const OP_DEPOSIT: u32 = 1;
const OP_WITHDRAW: u32 = 2;
const OP_INVEST: u32 = 3;
const OP_UNWIND: u32 = 4;
const OP_TRADE: u32 = 5;
const OP_INVEST_LP: u32 = 6;
const OP_UNWIND_LP: u32 = 7;

// ---------------------------------------------------------------------------
// Cross-contract helpers
// ---------------------------------------------------------------------------

/// Share-token interface helpers (mint / burn / total_supply / balance).
mod share {
    use soroban_sdk::{Address, Env, IntoVal, Symbol};

    pub fn total_supply(env: &Env, share_token: &Address) -> i128 {
        let args = ().into_val(env);
        env.invoke_contract(share_token, &Symbol::new(env, "total_supply"), args)
    }

    pub fn mint(env: &Env, share_token: &Address, to: &Address, amount: i128) {
        let args = (to.clone(), amount).into_val(env);
        env.invoke_contract::<()>(share_token, &Symbol::new(env, "mint"), args);
    }

    pub fn burn(env: &Env, share_token: &Address, from: &Address, amount: i128) {
        let args = (from.clone(), amount).into_val(env);
        env.invoke_contract::<()>(share_token, &Symbol::new(env, "burn"), args);
    }

    pub fn get_admin(env: &Env, share_token: &Address) -> Address {
        let args = ().into_val(env);
        env.invoke_contract(share_token, &Symbol::new(env, "get_admin"), args)
    }

    pub fn set_admin(env: &Env, share_token: &Address, new_admin: &Address) {
        let args = (new_admin.clone(),).into_val(env);
        env.invoke_contract::<()>(share_token, &Symbol::new(env, "set_admin"), args);
    }
}

/// Query a strategy's current NAV contribution.
fn strategy_get_value(env: &Env, strategy: &Address, vault: &Address) -> i128 {
    let args = (vault.clone(),).into_val(env);
    env.invoke_contract(strategy, &Symbol::new(env, "get_value"), args)
}

/// Call strategy.deposit (for single-asset Blend-style strategies).
fn strategy_deposit(env: &Env, strategy: &Address, amount: i128, from: &Address) -> i128 {
    let args = (amount, from.clone()).into_val(env);
    env.invoke_contract(strategy, &Symbol::new(env, "deposit"), args)
}

/// Call strategy.withdraw (for single-asset Blend-style strategies).
fn strategy_withdraw(
    env: &Env,
    strategy: &Address,
    amount: i128,
    from: &Address,
    to: &Address,
) -> i128 {
    let args = (amount, from.clone(), to.clone()).into_val(env);
    env.invoke_contract(strategy, &Symbol::new(env, "withdraw"), args)
}

/// Call `strategy.deposit_liquidity(amount_a, amount_b, min_a, min_b, from)` for LP strategies.
fn strategy_deposit_lp(
    env: &Env,
    strategy: &Address,
    amount_a: i128,
    amount_b: i128,
    min_a: i128,
    min_b: i128,
    from: &Address,
) -> i128 {
    let args = (amount_a, amount_b, min_a, min_b, from.clone()).into_val(env);
    env.invoke_contract(strategy, &Symbol::new(env, "deposit_liquidity"), args)
}

/// Call `strategy.withdraw(lp_amount, min_a, min_b, from, to)` for LP strategies.
fn strategy_withdraw_lp(
    env: &Env,
    strategy: &Address,
    lp_amount: i128,
    min_a: i128,
    min_b: i128,
    from: &Address,
    to: &Address,
) -> (i128, i128) {
    let args = (lp_amount, min_a, min_b, from.clone(), to.clone()).into_val(env);
    env.invoke_contract(strategy, &Symbol::new(env, "withdraw"), args)
}

/// Query `strategy.asset_a()` — first token of an LP pair.
fn strategy_asset_a(env: &Env, strategy: &Address) -> Address {
    let args = ().into_val(env);
    env.invoke_contract(strategy, &Symbol::new(env, "asset_a"), args)
}

/// Query `strategy.asset_b()` — second token of an LP pair.
fn strategy_asset_b(env: &Env, strategy: &Address) -> Address {
    let args = ().into_val(env);
    env.invoke_contract(strategy, &Symbol::new(env, "asset_b"), args)
}

/// Query whether an LP strategy has its internal oracle configured.
fn strategy_has_oracle(env: &Env, strategy: &Address) -> bool {
    let args = ().into_val(env);
    env.invoke_contract(strategy, &Symbol::new(env, "has_oracle"), args)
}

/// Query an exact-in quote from a strategy/router wrapper.
fn strategy_quote_exact_in(
    env: &Env,
    strategy: &Address,
    amount_in: i128,
    path: &Vec<Address>,
) -> i128 {
    let args = (amount_in, path.clone()).into_val(env);
    env.invoke_contract(strategy, &Symbol::new(env, "quote_exact_in"), args)
}

/// Call a trade-guard's `validate_swap_exact_in` function.
/// Returns without panic if validation passes; reverts otherwise.
fn guard_validate(
    env: &Env,
    guard: &Address,
    vault: &Address,
    amount_in: i128,
    min_out: i128,
    path: &Vec<Address>,
    quoted_out: i128,
) {
    let args = (vault.clone(), amount_in, min_out, path.clone(), quoted_out).into_val(env);
    env.invoke_contract::<()>(guard, &Symbol::new(env, "validate_swap_exact_in"), args);
}

/// Call a trade-guard's `validate_invest(vault, amount)`.
/// If a guard is registered for the strategy this must pass; reverts otherwise.
fn guard_validate_invest(env: &Env, guard: &Address, vault: &Address, amount: i128) {
    let args = (vault.clone(), amount).into_val(env);
    env.invoke_contract::<()>(guard, &Symbol::new(env, "validate_invest"), args);
}

/// Call a trade-guard's `validate_unwind(vault, units)`.
/// If a guard is registered for the strategy this must pass; reverts otherwise.
fn guard_validate_unwind(env: &Env, guard: &Address, vault: &Address, units: i128) {
    let args = (vault.clone(), units).into_val(env);
    env.invoke_contract::<()>(guard, &Symbol::new(env, "validate_unwind"), args);
}

// ---------------------------------------------------------------------------
// Vault initialization params
// ---------------------------------------------------------------------------

/// Parameters passed to [`Vault::initialize`].
#[contracttype]
#[derive(Clone, Debug)]
pub struct VaultParams {
    /// Account that manages strategies, fees, and pausing.
    pub manager: Address,
    /// Account that can execute spot trades (may equal `manager`).
    pub trader: Address,
    /// The single deposit/withdrawal asset (e.g. USDC).
    pub base_asset: Address,
    /// Deployed SEP-41 share token contract.
    pub share_token: Address,
    /// Account that currently controls `share_token` admin rights.
    ///
    /// During `initialize`, the vault atomically takes share-token admin by
    /// calling `share_token.set_admin(vault)`. This address must match the
    /// token's current admin unless admin is already the vault.
    pub share_token_admin: Address,
    /// Entry fee in basis points (0–500).
    pub entry_fee_bps: u32,
    /// Exit fee in basis points (0–500).
    pub exit_fee_bps: u32,
    /// Annual management fee in basis points (0–300).
    pub mgmt_fee_bps: u32,
    /// Performance fee in basis points (0–3000).
    pub perf_fee_bps: u32,
}

/// Pending announced fee schedule that can be committed after timelock.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AnnouncedFees {
    pub entry_fee_bps: Option<u32>,
    pub exit_fee_bps: Option<u32>,
    pub mgmt_fee_bps: Option<u32>,
    pub perf_fee_bps: Option<u32>,
    pub activation_ts: Option<u64>,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct Vault;

#[contractimpl]
impl Vault {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the vault.
    ///
    /// Must be called once after deployment. Sets the fee schedule, manager,
    /// trader, base asset, and share token.
    ///
    /// # Errors
    /// * [`VaultError::AlreadyInitialized`]
    /// * [`VaultError::InvalidAmount`] — if any fee exceeds its cap.
    pub fn initialize(env: Env, params: VaultParams) {
        if is_initialized(&env) {
            panic_with_error!(&env, VaultError::AlreadyInitialized);
        }
        params.manager.require_auth();
        params.trader.require_auth();
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if params.entry_fee_bps > MAX_ENTRY_EXIT_FEE_BPS
            || params.exit_fee_bps > MAX_ENTRY_EXIT_FEE_BPS
        {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if params.mgmt_fee_bps > MAX_MGMT_FEE_BPS {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if params.perf_fee_bps > MAX_PERF_FEE_BPS {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }

        let vault = env.current_contract_address();

        let current_share_admin = share::get_admin(&env, &params.share_token);
        if current_share_admin != vault {
            if current_share_admin != params.share_token_admin {
                panic_with_error!(&env, VaultError::ShareTokenAdminMismatch);
            }
            share::set_admin(&env, &params.share_token, &vault);
        }

        set_manager(&env, &params.manager);
        set_trader(&env, &params.trader);
        set_base_asset(&env, &params.base_asset);
        set_share_token(&env, &params.share_token);
        set_entry_fee_bps(&env, params.entry_fee_bps);
        set_exit_fee_bps(&env, params.exit_fee_bps);
        set_mgmt_fee_bps(&env, params.mgmt_fee_bps);
        set_perf_fee_bps(&env, params.perf_fee_bps);
        set_paused(&env, false);
        set_private_pool(&env, false);
        set_exit_cooldown_secs(&env, 0);
        set_value_manipulation_guard_enabled(&env, false);
        clear_announced_fees(&env);
        set_max_loss_bps(&env, DEFAULT_MAX_LOSS_BPS);

        let now = env.ledger().timestamp();
        set_last_mgmt_fee_ts(&env, now);
        set_high_water_mark(&env, PRICE_PRECISION); // initial NAV/share = 1.0
        set_strategies(&env, &Vec::new(&env));
    }

    // -----------------------------------------------------------------------
    // Deposit
    // -----------------------------------------------------------------------

    /// Deposit `amount` of the base asset and receive share tokens.
    ///
    /// Flow (dHedge V2 model):
    /// 1. Collect streaming management and performance fees (mint shares to manager).
    /// 2. Compute `total_shares` from the **full** `amount` at the pre-deposit price.
    /// 3. Split: `fee_shares = total_shares × entry_fee_bps / 10_000`
    ///           `user_shares = total_shares − fee_shares`
    /// 4. Pull `amount` from `from` into the vault (full amount, no base-asset
    ///    transfer to manager — entry fee is charged entirely as minted shares).
    /// 5. Mint `user_shares` to `from`, `fee_shares` to manager.
    ///
    /// Charging the entry fee as shares rather than base asset aligns manager
    /// incentives (they carry the same NAV risk as investors) and avoids the
    /// double-NAV-read the base-asset model requires.
    ///
    /// # Returns
    /// Number of share tokens minted to `from`.
    ///
    /// # Errors
    /// * [`VaultError::Paused`]
    /// * [`VaultError::InvalidAmount`]
    /// * [`VaultError::DepositCapExceeded`]
    pub fn deposit(env: Env, amount: i128, from: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if get_paused(&env) {
            panic_with_error!(&env, VaultError::Paused);
        }
        if amount <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }

        from.require_auth();

        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);
        let share_token = get_share_token(&env);
        let manager = get_manager(&env);

        if get_private_pool(&env) && from != manager && !is_member(&env, &from) {
            panic_with_error!(&env, VaultError::NotMember);
        }

        // Accrue fees before changing supply (keeps share price consistent).
        Self::collect_fees(&env, &vault, &share_token, &manager);

        // Compute share price on pre-deposit NAV and post-fee total supply.
        let nav = Self::nav(&env, &vault, &base_asset);
        Self::op_guard_pre_check(&env, &from, OP_DEPOSIT, nav);
        let total_supply = share::total_supply(&env, &share_token);
        let share_price = Self::share_price(nav, total_supply);

        // Total shares the full deposit amount buys at current price.
        let total_shares = if share_price == 0 || total_supply == 0 {
            // Bootstrap: 1 share per base-asset unit.
            amount
        } else {
            amount * PRICE_PRECISION / share_price
        };

        // Split: entry fee taken as shares minted to manager (dHedge V2 §1 Step 6).
        let entry_fee_bps = get_entry_fee_bps(&env) as i128;
        let fee_shares = total_shares * entry_fee_bps / FEE_DENOMINATOR as i128;
        let user_shares = total_shares - fee_shares;

        if user_shares <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }

        // Deposit cap: full `amount` enters the vault NAV.
        let cap = get_deposit_cap(&env);
        if cap > 0 && nav + amount > cap {
            panic_with_error!(&env, VaultError::DepositCapExceeded);
        }

        // Pull the full deposit into the vault — no base-asset fee transfer.
        token::Client::new(&env, &base_asset).transfer(&from, &vault, &amount);

        // Mint user shares; also mint fee shares to manager if applicable.
        share::mint(&env, &share_token, &from, user_shares);
        if fee_shares > 0 {
            share::mint(&env, &share_token, &manager, fee_shares);
        }
        let now = env.ledger().timestamp();
        set_last_deposit_ts(&env, &from, now);
        let nav_after = Self::nav(&env, &vault, &base_asset);
        Self::op_guard_post_checkpoint(&env, &from, OP_DEPOSIT, nav_after);

        deposit_event(&env, &from, amount, user_shares);

        user_shares
    }

    // -----------------------------------------------------------------------
    // Withdraw
    // -----------------------------------------------------------------------

    /// Burn `share_amount` of share tokens and receive base asset.
    ///
    /// Flow (dHedge V2 model):
    /// 1. Collect streaming management and performance fees.
    /// 2. Compute `base_gross = share_amount × share_price`.
    /// 3. `base_net = base_gross × (1 − exit_fee_bps / 10_000)`.
    ///    The fee fraction **stays in the vault** — it is not transferred to the
    ///    manager.  Remaining shareholders (including the manager's accumulated
    ///    fee shares from entry fees) benefit from the improved NAV per share.
    /// 4. Auto-unwind single-asset strategies if vault lacks `base_net`.
    /// 5. Burn `share_amount` from `from`, send `base_net` to `to`.
    ///
    /// # Returns
    /// Net base-asset amount delivered to `to`.
    ///
    /// # Errors
    /// * [`VaultError::Paused`]
    /// * [`VaultError::InvalidAmount`]
    /// * [`VaultError::InsufficientShares`]
    pub fn withdraw(env: Env, share_amount: i128, from: Address, to: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if get_paused(&env) {
            panic_with_error!(&env, VaultError::Paused);
        }
        if share_amount <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }

        from.require_auth();

        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);
        let share_token = get_share_token(&env);
        let manager = get_manager(&env);
        let cooldown_secs = get_exit_cooldown_secs(&env);
        if cooldown_secs > 0 {
            if let Some(last_deposit_ts) = get_last_deposit_ts_opt(&env, &from) {
                let now = env.ledger().timestamp();
                if now < last_deposit_ts.saturating_add(cooldown_secs) {
                    panic_with_error!(&env, VaultError::CooldownActive);
                }
            }
        }

        // Validate share balance before fee collection changes total supply.
        let share_balance = token::Client::new(&env, &share_token).balance(&from);
        if share_amount > share_balance {
            panic_with_error!(&env, VaultError::InsufficientShares);
        }

        // Accrue fees first.
        Self::collect_fees(&env, &vault, &share_token, &manager);

        // Re-read total supply (increased by fee minting).
        let total_supply = share::total_supply(&env, &share_token);
        let nav = Self::nav(&env, &vault, &base_asset);
        Self::op_guard_pre_check(&env, &from, OP_WITHDRAW, nav);
        let share_price = Self::share_price(nav, total_supply);

        let base_gross = share_amount * share_price / PRICE_PRECISION;

        // Exit fee: user receives (1 − exit_fee_bps) fraction; the remainder
        // stays in the vault and accrues to remaining shareholders (dHedge V2 §2 Step 4).
        let exit_fee_bps = get_exit_fee_bps(&env) as i128;
        let base_net =
            base_gross * (FEE_DENOMINATOR as i128 - exit_fee_bps) / FEE_DENOMINATOR as i128;

        if base_net <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }

        // Auto-unwind: if the vault doesn't hold enough base asset for base_net,
        // proportionally redeem single-asset strategy positions to cover shortfall.
        // LP strategies are skipped (marked via set_lp_strategy); manager unwinds
        // those manually via unwind_lp.
        {
            let vault_balance = token::Client::new(&env, &base_asset).balance(&vault);
            if vault_balance < base_net {
                let shortfall = base_net - vault_balance;
                let strategies = get_strategies(&env);
                let mut total_sa_value: i128 = 0;
                for s in strategies.iter() {
                    if !is_lp_strategy(&env, &s) {
                        total_sa_value = total_sa_value
                            .saturating_add(Self::strategy_value_in_nav(&env, &vault, &s));
                    }
                }
                if total_sa_value > 0 {
                    for s in strategies.iter() {
                        if !is_lp_strategy(&env, &s) {
                            let sv = Self::strategy_value_in_nav(&env, &vault, &s);
                            if sv > 0 {
                                let portion = shortfall.saturating_mul(sv) / total_sa_value;
                                let pull = if portion > sv { sv } else { portion };
                                if pull > 0 {
                                    strategy_withdraw(&env, &s, pull, &vault, &vault);
                                }
                            }
                        }
                    }
                }

                // Cover rounding dust from proportional splits by sweeping any
                // remaining shortfall from single-asset strategies.
                let mut remaining =
                    base_net.saturating_sub(token::Client::new(&env, &base_asset).balance(&vault));
                if remaining > 0 {
                    for s in strategies.iter() {
                        if is_lp_strategy(&env, &s) {
                            continue;
                        }
                        let sv = Self::strategy_value_in_nav(&env, &vault, &s);
                        if sv <= 0 {
                            continue;
                        }
                        let pull = if sv < remaining { sv } else { remaining };
                        strategy_withdraw(&env, &s, pull, &vault, &vault);
                        let current_balance = token::Client::new(&env, &base_asset).balance(&vault);
                        remaining = base_net.saturating_sub(current_balance);
                        if remaining == 0 {
                            break;
                        }
                    }
                }
            }
        }

        if token::Client::new(&env, &base_asset).balance(&vault) < base_net {
            panic_with_error!(&env, VaultError::InsufficientLiquidity);
        }

        // Burn shares and transfer base_net to recipient.
        // No explicit fee transfer — the exit-fee fraction stays in the vault.
        share::burn(&env, &share_token, &from, share_amount);
        token::Client::new(&env, &base_asset).transfer(&vault, &to, &base_net);
        let nav_after = Self::nav(&env, &vault, &base_asset);
        Self::op_guard_post_checkpoint(&env, &from, OP_WITHDRAW, nav_after);

        withdraw_event(&env, &to, share_amount, base_net);

        base_net
    }

    // -----------------------------------------------------------------------
    // Strategy management (manager only)
    // -----------------------------------------------------------------------

    /// Replace the list of whitelisted strategy contracts. Manager only.
    ///
    /// Strategies being **removed** from the whitelist must have a zero
    /// position (`get_value(vault) == 0`) or the call reverts.  This prevents
    /// a manager from silently abandoning active positions, which would cause
    /// NAV understatement and share-price manipulation.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::StrategyHasActivePosition`] — if a removed strategy
    ///   still reports a non-zero value.
    pub fn set_strategies(env: Env, caller: Address, strategies: Vec<Address>) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }

        // Safety check: any strategy being removed must have no active position.
        let vault = env.current_contract_address();
        let current = get_strategies(&env);
        for s in current.iter() {
            let mut still_present = false;
            for new_s in strategies.iter() {
                if new_s == s {
                    still_present = true;
                    break;
                }
            }
            if !still_present {
                let value = strategy_get_value(&env, &s, &vault);
                if value != 0 {
                    panic_with_error!(&env, VaultError::StrategyHasActivePosition);
                }
            }
        }

        set_strategies(&env, &strategies);
        strategy_set_event(&env, &strategies);
    }

    /// Associate a trade guard with a strategy. Manager only.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::StrategyNotWhitelisted`]
    pub fn set_trade_guard(env: Env, caller: Address, strategy: Address, guard: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if !Self::is_strategy_whitelisted(&env, &strategy) {
            panic_with_error!(&env, VaultError::StrategyNotWhitelisted);
        }
        set_trade_guard(&env, &strategy, &guard);
    }

    // -----------------------------------------------------------------------
    // Invest / Unwind (manager only)
    // -----------------------------------------------------------------------

    /// Transfer `amount` of base asset from the vault into a single-asset strategy.
    ///
    /// Calls `strategy.deposit(amount, vault)` which pulls the tokens.
    /// **For two-asset LP strategies (Soroswap, Phoenix) use [`invest_lp`].**
    ///
    /// # Returns
    /// Number of strategy position units received.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::StrategyNotWhitelisted`]
    /// * [`VaultError::InvalidAmount`]
    pub fn invest(env: Env, caller: Address, strategy: Address, amount: i128) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if amount <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if !Self::is_strategy_whitelisted(&env, &strategy) {
            panic_with_error!(&env, VaultError::StrategyNotWhitelisted);
        }

        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);

        // If a guard is configured for this strategy, it must approve the invest.
        if let Some(guard) = get_trade_guard(&env, &strategy) {
            guard_validate_invest(&env, &guard, &vault, amount);
        }

        // Snapshot NAV before for post-execution guard.
        let max_loss_bps = get_max_loss_bps(&env);
        let vm_guard_enabled = get_value_manipulation_guard_enabled(&env);
        let nav_before = if max_loss_bps > 0 || vm_guard_enabled {
            Self::nav(&env, &vault, &base_asset)
        } else {
            0
        };
        if vm_guard_enabled {
            Self::op_guard_pre_check(&env, &caller, OP_INVEST, nav_before);
        }

        // Approve strategy to pull `amount` from vault.
        let expiry = env.ledger().sequence() + 100;
        token::Client::new(&env, &base_asset).approve(&vault, &strategy, &amount, &expiry);

        let units = strategy_deposit(&env, &strategy, amount, &vault);

        // Compute nav_after once; reuse for both concentration and NAV guard checks.
        let max_conc = get_max_concentration_bps(&env);
        let nav_after = if max_conc > 0 || max_loss_bps > 0 || vm_guard_enabled {
            Self::nav(&env, &vault, &base_asset)
        } else {
            0
        };

        // Concentration limit: ensure one strategy doesn't dominate the vault.
        if max_conc > 0 && nav_after > 0 {
            let sv = Self::strategy_value_in_nav(&env, &vault, &strategy);
            if sv * FEE_DENOMINATOR as i128 / nav_after > max_conc as i128 {
                panic_with_error!(&env, VaultError::ConcentrationLimitExceeded);
            }
        }

        // Post-execution NAV guard.
        if max_loss_bps > 0 {
            let min_nav = nav_before
                .saturating_sub(nav_before * max_loss_bps as i128 / FEE_DENOMINATOR as i128);
            if nav_after < min_nav {
                panic_with_error!(&env, VaultError::TvlGuardTripped);
            }
        }
        if vm_guard_enabled {
            Self::op_guard_post_checkpoint(&env, &caller, OP_INVEST, nav_after);
        }

        invest_event(&env, &strategy, amount);

        units
    }

    /// Provide liquidity into a two-asset LP strategy (Soroswap / Phoenix).
    ///
    /// Queries `strategy.asset_a()` and `strategy.asset_b()` to discover the
    /// token pair, approves both tokens to the strategy, then calls
    /// `strategy.deposit_liquidity(amount_a, amount_b, min_a, min_b, vault)`.
    ///
    /// The vault must already hold `amount_a` of `asset_a` **and** `amount_b`
    /// of `asset_b` (the manager typically obtains the second token via
    /// `execute_trade` before calling this function).
    ///
    /// # Returns
    /// LP tokens minted to the strategy.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::StrategyNotWhitelisted`]
    /// * [`VaultError::InvalidAmount`]
    /// * [`VaultError::LpStrategyOracleRequired`]
    pub fn invest_lp(
        env: Env,
        caller: Address,
        strategy: Address,
        amount_a: i128,
        amount_b: i128,
        min_a: i128,
        min_b: i128,
    ) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if amount_a <= 0 || amount_b <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if !Self::is_strategy_whitelisted(&env, &strategy) {
            panic_with_error!(&env, VaultError::StrategyNotWhitelisted);
        }
        if !strategy_has_oracle(&env, &strategy) {
            panic_with_error!(&env, VaultError::LpStrategyOracleRequired);
        }
        if !is_lp_strategy(&env, &strategy) {
            set_lp_strategy(&env, &strategy, true);
        }

        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);

        // If a guard is configured for this strategy, it must approve the invest.
        if let Some(guard) = get_trade_guard(&env, &strategy) {
            guard_validate_invest(&env, &guard, &vault, amount_a + amount_b);
        }

        // Snapshot NAV before.
        let max_loss_bps = get_max_loss_bps(&env);
        let vm_guard_enabled = get_value_manipulation_guard_enabled(&env);
        let nav_before = if max_loss_bps > 0 || vm_guard_enabled {
            Self::nav(&env, &vault, &base_asset)
        } else {
            0
        };
        if vm_guard_enabled {
            Self::op_guard_pre_check(&env, &caller, OP_INVEST_LP, nav_before);
        }

        let expiry = env.ledger().sequence() + 100;

        // Discover the pair tokens from the strategy.
        let asset_a = strategy_asset_a(&env, &strategy);
        let asset_b = strategy_asset_b(&env, &strategy);

        // Approve both tokens so the strategy can pull them.
        token::Client::new(&env, &asset_a).approve(&vault, &strategy, &amount_a, &expiry);
        token::Client::new(&env, &asset_b).approve(&vault, &strategy, &amount_b, &expiry);

        let lp_received =
            strategy_deposit_lp(&env, &strategy, amount_a, amount_b, min_a, min_b, &vault);

        // Compute nav_after once for both concentration and NAV guard.
        let max_conc = get_max_concentration_bps(&env);
        let nav_after = if max_conc > 0 || max_loss_bps > 0 || vm_guard_enabled {
            Self::nav(&env, &vault, &base_asset)
        } else {
            0
        };

        // Concentration limit.
        if max_conc > 0 && nav_after > 0 {
            let sv = Self::strategy_value_in_nav(&env, &vault, &strategy);
            if sv * FEE_DENOMINATOR as i128 / nav_after > max_conc as i128 {
                panic_with_error!(&env, VaultError::ConcentrationLimitExceeded);
            }
        }

        // Post-execution NAV guard.
        if max_loss_bps > 0 {
            let min_nav = nav_before
                .saturating_sub(nav_before * max_loss_bps as i128 / FEE_DENOMINATOR as i128);
            if nav_after < min_nav {
                panic_with_error!(&env, VaultError::TvlGuardTripped);
            }
        }
        if vm_guard_enabled {
            Self::op_guard_post_checkpoint(&env, &caller, OP_INVEST_LP, nav_after);
        }

        invest_event(&env, &strategy, amount_a + amount_b);

        lp_received
    }

    /// Redeem LP tokens from a two-asset strategy back to the vault.
    ///
    /// Calls `strategy.withdraw(lp_amount, min_a, min_b, vault, vault)`;
    /// underlying tokens (`asset_a`, `asset_b`) arrive at the vault.
    ///
    /// # Returns
    /// `(amount_a, amount_b)` received by the vault.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::StrategyNotWhitelisted`]
    /// * [`VaultError::InvalidAmount`]
    /// * [`VaultError::LpStrategyOracleRequired`]
    pub fn unwind_lp(
        env: Env,
        caller: Address,
        strategy: Address,
        lp_amount: i128,
        min_a: i128,
        min_b: i128,
    ) -> (i128, i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if lp_amount <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if !Self::is_strategy_whitelisted(&env, &strategy) {
            panic_with_error!(&env, VaultError::StrategyNotWhitelisted);
        }
        if !strategy_has_oracle(&env, &strategy) {
            panic_with_error!(&env, VaultError::LpStrategyOracleRequired);
        }
        if !is_lp_strategy(&env, &strategy) {
            set_lp_strategy(&env, &strategy, true);
        }

        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);

        // If a guard is configured for this strategy, it must approve the unwind.
        if let Some(guard) = get_trade_guard(&env, &strategy) {
            guard_validate_unwind(&env, &guard, &vault, lp_amount);
        }

        // Snapshot NAV before.
        let max_loss_bps = get_max_loss_bps(&env);
        let vm_guard_enabled = get_value_manipulation_guard_enabled(&env);
        let nav_before = if max_loss_bps > 0 || vm_guard_enabled {
            Self::nav(&env, &vault, &base_asset)
        } else {
            0
        };
        if vm_guard_enabled {
            Self::op_guard_pre_check(&env, &caller, OP_UNWIND_LP, nav_before);
        }

        let (a, b) = strategy_withdraw_lp(&env, &strategy, lp_amount, min_a, min_b, &vault, &vault);

        // Post-execution NAV guard.
        let nav_after = if max_loss_bps > 0 || vm_guard_enabled {
            Self::nav(&env, &vault, &base_asset)
        } else {
            0
        };
        if max_loss_bps > 0 {
            let min_nav = nav_before
                .saturating_sub(nav_before * max_loss_bps as i128 / FEE_DENOMINATOR as i128);
            if nav_after < min_nav {
                panic_with_error!(&env, VaultError::TvlGuardTripped);
            }
        }
        if vm_guard_enabled {
            Self::op_guard_post_checkpoint(&env, &caller, OP_UNWIND_LP, nav_after);
        }

        unwind_event(&env, &strategy, lp_amount);

        (a, b)
    }

    /// Withdraw `units` from a strategy back to the vault.
    ///
    /// Calls `strategy.withdraw(units, vault, vault)` — tokens return to vault.
    /// **Use this for single-asset strategies (e.g. Blend). For two-asset LP
    /// strategies use [`unwind_lp`].**
    ///
    /// # Returns
    /// Base-asset amount returned to the vault.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::StrategyNotWhitelisted`]
    /// * [`VaultError::InvalidAmount`]
    pub fn unwind(env: Env, caller: Address, strategy: Address, units: i128) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if units <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if !Self::is_strategy_whitelisted(&env, &strategy) {
            panic_with_error!(&env, VaultError::StrategyNotWhitelisted);
        }

        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);

        // If a guard is configured for this strategy, it must approve the unwind.
        if let Some(guard) = get_trade_guard(&env, &strategy) {
            guard_validate_unwind(&env, &guard, &vault, units);
        }

        // Snapshot NAV before.
        let max_loss_bps = get_max_loss_bps(&env);
        let vm_guard_enabled = get_value_manipulation_guard_enabled(&env);
        let nav_before = if max_loss_bps > 0 || vm_guard_enabled {
            Self::nav(&env, &vault, &base_asset)
        } else {
            0
        };
        if vm_guard_enabled {
            Self::op_guard_pre_check(&env, &caller, OP_UNWIND, nav_before);
        }

        let amount = strategy_withdraw(&env, &strategy, units, &vault, &vault);

        // Post-execution NAV guard.
        let nav_after = if max_loss_bps > 0 || vm_guard_enabled {
            Self::nav(&env, &vault, &base_asset)
        } else {
            0
        };
        if max_loss_bps > 0 {
            let min_nav = nav_before
                .saturating_sub(nav_before * max_loss_bps as i128 / FEE_DENOMINATOR as i128);
            if nav_after < min_nav {
                panic_with_error!(&env, VaultError::TvlGuardTripped);
            }
        }
        if vm_guard_enabled {
            Self::op_guard_post_checkpoint(&env, &caller, OP_UNWIND, nav_after);
        }

        unwind_event(&env, &strategy, units);

        amount
    }

    // -----------------------------------------------------------------------
    // Spot trading (trader only)
    // -----------------------------------------------------------------------

    /// Execute a spot swap through a strategy's associated trade guard.
    ///
    /// The guard's
    /// `validate_swap_exact_in(vault, amount_in, min_out, path, quoted_out)` is
    /// called first; if it reverts, the swap is aborted. The swap itself is
    /// then forwarded to the strategy contract's `execute_trade` function.
    ///
    /// # Arguments
    /// * `caller`    – Must equal the registered trader address.
    /// * `strategy`  – Whitelisted strategy that will execute the swap.
    /// * `amount_in` – Exact input amount from the vault's base-asset balance.
    /// * `min_out`   – Minimum output required (slippage protection).
    /// * `path`      – Ordered token path [token_in, …, token_out].
    ///
    /// # Returns
    /// Actual output amount received.
    ///
    /// # Errors
    /// * [`VaultError::NotTrader`]
    /// * [`VaultError::StrategyNotWhitelisted`]
    /// * [`VaultError::GuardNotSet`]
    /// * [`VaultError::InvalidAmount`]
    pub fn execute_trade(
        env: Env,
        caller: Address,
        strategy: Address,
        amount_in: i128,
        min_out: i128,
        path: Vec<Address>,
    ) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_trader(&env) {
            panic_with_error!(&env, VaultError::NotTrader);
        }
        if amount_in <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if !Self::is_strategy_whitelisted(&env, &strategy) {
            panic_with_error!(&env, VaultError::StrategyNotWhitelisted);
        }

        let guard = match get_trade_guard(&env, &strategy) {
            Some(g) => g,
            None => panic_with_error!(&env, VaultError::GuardNotSet),
        };

        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);

        let quoted_out = strategy_quote_exact_in(&env, &strategy, amount_in, &path);
        if quoted_out <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }

        // Snapshot NAV before execution for post-trade guard check.
        let max_loss_bps = get_max_loss_bps(&env);
        let vm_guard_enabled = get_value_manipulation_guard_enabled(&env);
        let nav_before = if max_loss_bps > 0 || vm_guard_enabled {
            Self::nav(&env, &vault, &base_asset)
        } else {
            0
        };
        if vm_guard_enabled {
            Self::op_guard_pre_check(&env, &caller, OP_TRADE, nav_before);
        }

        // Guard validates policy (reverts on violation).
        guard_validate(&env, &guard, &vault, amount_in, min_out, &path, quoted_out);

        // Forward trade to strategy.
        let args = (amount_in, min_out, path, vault.clone()).into_val(&env);
        let amount_out: i128 =
            env.invoke_contract(&strategy, &Symbol::new(&env, "execute_trade"), args);
        if amount_out < min_out {
            panic_with_error!(&env, VaultError::TradeGuardRejected);
        }

        // Post-execution NAV guard (dHedge V2 §4 Steps 6–7).
        let nav_after = if max_loss_bps > 0 || vm_guard_enabled {
            Self::nav(&env, &vault, &base_asset)
        } else {
            0
        };
        if max_loss_bps > 0 {
            let min_nav = nav_before
                .saturating_sub(nav_before * max_loss_bps as i128 / FEE_DENOMINATOR as i128);
            if nav_after < min_nav {
                panic_with_error!(&env, VaultError::TvlGuardTripped);
            }
        }
        if vm_guard_enabled {
            Self::op_guard_post_checkpoint(&env, &caller, OP_TRADE, nav_after);
        }

        trade_event(&env, &strategy, amount_in, min_out);

        amount_out
    }

    // -----------------------------------------------------------------------
    // Fee collection (internal — called before every deposit/withdraw)
    // -----------------------------------------------------------------------

    /// Accrue management fee and performance fee, minting shares to manager.
    fn collect_fees(env: &Env, vault: &Address, share_token: &Address, manager: &Address) {
        let total_supply = share::total_supply(env, share_token);
        if total_supply == 0 {
            // Nothing to accrue yet.
            let now = env.ledger().timestamp();
            set_last_mgmt_fee_ts(env, now);
            return;
        }

        let base_asset = get_base_asset(env);
        let nav = Self::nav(env, vault, &base_asset);

        // ---- Management fee (streaming) ------------------------------------
        let mgmt_fee_bps = get_mgmt_fee_bps(env) as i128;
        let last_ts = get_last_mgmt_fee_ts(env);
        let now = env.ledger().timestamp();
        // Always advance the timestamp so that enabling fees later does not
        // cause a large backdated charge (dHedge V2 pattern).
        set_last_mgmt_fee_ts(env, now);
        if mgmt_fee_bps > 0 {
            let elapsed = now.saturating_sub(last_ts) as i128;

            if elapsed > 0 && nav > 0 {
                // fee_value = NAV * mgmt_fee_bps * elapsed / (10_000 * SECONDS_PER_YEAR)
                let fee_value = nav.saturating_mul(mgmt_fee_bps).saturating_mul(elapsed)
                    / (FEE_DENOMINATOR as i128 * SECONDS_PER_YEAR as i128);

                if fee_value > 0 {
                    // Convert fee value to shares at current price.
                    let price = Self::share_price(nav, total_supply);
                    let fee_shares = fee_value * PRICE_PRECISION / price.max(1);
                    if fee_shares > 0 {
                        share::mint(env, share_token, manager, fee_shares);
                        mgmt_fee_event(env, fee_shares, now);
                    }
                }
            }
        }

        // ---- Performance fee (high-water mark) ----------------------------
        let perf_fee_bps = get_perf_fee_bps(env) as i128;
        if perf_fee_bps > 0 && total_supply > 0 {
            // Re-read total supply (may have increased from mgmt fee minting).
            let new_supply = share::total_supply(env, share_token);
            let new_nav = Self::nav(env, vault, &base_asset);
            let current_price = Self::share_price(new_nav, new_supply);
            let hwm = get_high_water_mark(env);

            if current_price > hwm && hwm > 0 {
                let gain = current_price - hwm;
                // fee_value = total_supply * gain * perf_fee_bps / (PRICE_PRECISION * 10_000)
                let fee_value = new_supply.saturating_mul(gain).saturating_mul(perf_fee_bps)
                    / (PRICE_PRECISION * FEE_DENOMINATOR as i128);

                if fee_value > 0 {
                    let fee_shares = fee_value * PRICE_PRECISION / current_price.max(1);
                    if fee_shares > 0 {
                        share::mint(env, share_token, manager, fee_shares);
                        perf_fee_event(env, fee_shares, current_price);
                    }
                }
                set_high_water_mark(env, current_price);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Emergency controls (manager only)
    // -----------------------------------------------------------------------

    /// Pause deposits and withdrawals. Manager only.
    pub fn pause(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_paused(&env, true);
        pause_event(&env, true);
    }

    /// Unpause deposits and withdrawals. Manager only.
    pub fn unpause(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_paused(&env, false);
        pause_event(&env, false);
    }

    // -----------------------------------------------------------------------
    // Role management (manager only)
    // -----------------------------------------------------------------------

    /// Transfer the manager role to a new address.
    ///
    /// The current manager must authorise the call.  After this call the
    /// **new** manager controls strategy management, fee configuration, and
    /// pause/unpause.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    pub fn set_manager(env: Env, caller: Address, new_manager: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_manager(&env, &new_manager);
        manager_changed_event(&env, &new_manager);
    }

    /// Transfer the trader role to a new address. Manager only.
    ///
    /// After this call only `new_trader` can call `execute_trade`.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    pub fn set_trader(env: Env, caller: Address, new_trader: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_trader(&env, &new_trader);
        trader_changed_event(&env, &new_trader);
    }

    // -----------------------------------------------------------------------
    // Fee & cap configuration (manager only)
    // -----------------------------------------------------------------------

    /// Update the entry fee. Manager only. Capped at [`MAX_ENTRY_EXIT_FEE_BPS`].
    ///
    /// # Errors
    /// * [`VaultError::NotManager`] / [`VaultError::InvalidAmount`]
    pub fn set_entry_fee_bps(env: Env, caller: Address, bps: u32) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if bps > MAX_ENTRY_EXIT_FEE_BPS {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if bps > get_entry_fee_bps(&env) {
            panic_with_error!(&env, VaultError::FeeIncreaseDelayActive);
        }
        set_entry_fee_bps(&env, bps);
    }

    /// Update the exit fee. Manager only. Capped at [`MAX_ENTRY_EXIT_FEE_BPS`].
    ///
    /// # Errors
    /// * [`VaultError::NotManager`] / [`VaultError::InvalidAmount`]
    pub fn set_exit_fee_bps(env: Env, caller: Address, bps: u32) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if bps > MAX_ENTRY_EXIT_FEE_BPS {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if bps > get_exit_fee_bps(&env) {
            panic_with_error!(&env, VaultError::FeeIncreaseDelayActive);
        }
        set_exit_fee_bps(&env, bps);
    }

    /// Update the annual management fee. Manager only. Capped at [`MAX_MGMT_FEE_BPS`].
    ///
    /// The timestamp is NOT reset; accrued-since-last-collection will use the
    /// old rate for the elapsed period and the new rate going forward (standard
    /// fund accounting).
    ///
    /// # Errors
    /// * [`VaultError::NotManager`] / [`VaultError::InvalidAmount`]
    pub fn set_mgmt_fee_bps(env: Env, caller: Address, bps: u32) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if bps > MAX_MGMT_FEE_BPS {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if bps > get_mgmt_fee_bps(&env) {
            panic_with_error!(&env, VaultError::FeeIncreaseDelayActive);
        }
        set_mgmt_fee_bps(&env, bps);
    }

    /// Update the performance fee. Manager only. Capped at [`MAX_PERF_FEE_BPS`].
    ///
    /// # Errors
    /// * [`VaultError::NotManager`] / [`VaultError::InvalidAmount`]
    pub fn set_perf_fee_bps(env: Env, caller: Address, bps: u32) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if bps > MAX_PERF_FEE_BPS {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if bps > get_perf_fee_bps(&env) {
            panic_with_error!(&env, VaultError::FeeIncreaseDelayActive);
        }
        set_perf_fee_bps(&env, bps);
    }

    /// Announce a fee-increase schedule that can be committed after timelock.
    pub fn announce_fee_increase(
        env: Env,
        caller: Address,
        entry_fee_bps: u32,
        exit_fee_bps: u32,
        mgmt_fee_bps: u32,
        perf_fee_bps: u32,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if entry_fee_bps > MAX_ENTRY_EXIT_FEE_BPS
            || exit_fee_bps > MAX_ENTRY_EXIT_FEE_BPS
            || mgmt_fee_bps > MAX_MGMT_FEE_BPS
            || perf_fee_bps > MAX_PERF_FEE_BPS
        {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        set_announced_entry_fee_bps(&env, entry_fee_bps);
        set_announced_exit_fee_bps(&env, exit_fee_bps);
        set_announced_mgmt_fee_bps(&env, mgmt_fee_bps);
        set_announced_perf_fee_bps(&env, perf_fee_bps);
        set_announced_fee_activation_ts(
            &env,
            env.ledger()
                .timestamp()
                .saturating_add(FEE_INCREASE_DELAY_SECS),
        );
    }

    /// Commit the latest announced fee-increase schedule once delay elapsed.
    pub fn commit_fee_increase(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }

        let activation_ts = get_announced_fee_activation_ts(&env)
            .unwrap_or_else(|| panic_with_error!(&env, VaultError::NoFeeIncreaseAnnounced));
        if env.ledger().timestamp() < activation_ts {
            panic_with_error!(&env, VaultError::FeeIncreaseDelayActive);
        }

        let next_entry = get_announced_entry_fee_bps(&env).unwrap_or(get_entry_fee_bps(&env));
        let next_exit = get_announced_exit_fee_bps(&env).unwrap_or(get_exit_fee_bps(&env));
        let next_mgmt = get_announced_mgmt_fee_bps(&env).unwrap_or(get_mgmt_fee_bps(&env));
        let next_perf = get_announced_perf_fee_bps(&env).unwrap_or(get_perf_fee_bps(&env));

        set_entry_fee_bps(&env, next_entry);
        set_exit_fee_bps(&env, next_exit);
        set_mgmt_fee_bps(&env, next_mgmt);
        set_perf_fee_bps(&env, next_perf);
        clear_announced_fees(&env);
    }

    /// Cancel any previously announced fee-increase schedule.
    pub fn renounce_fee_increase(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        clear_announced_fees(&env);
    }

    /// Set the maximum NAV loss allowed per manager operation. Manager only.
    ///
    /// After every `invest`, `unwind`, `invest_lp`, `unwind_lp`, and
    /// `execute_trade` the vault checks:
    /// ```text
    /// nav_after ≥ nav_before × (1 − max_loss_bps / 10_000)
    /// ```
    /// If the check fails the transaction reverts with
    /// [`VaultError::TvlGuardTripped`], preventing the manager from silently
    /// draining TVL through repeated high-slippage operations.
    ///
    /// Set `0` to disable the guard (default).  A typical production value is
    /// `100` (1 % per transaction).
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    pub fn set_max_loss_bps(env: Env, caller: Address, bps: u32) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if bps > FEE_DENOMINATOR {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        set_max_loss_bps(&env, bps);
    }

    /// Register the oracle contract used to price strategy positions. Manager only.
    ///
    /// Once set, `nav()` will call `oracle.get_price(price_token)` for each
    /// strategy that has a price token registered via [`set_strategy_oracle_token`].
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    pub fn set_oracle(env: Env, caller: Address, oracle: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_oracle(&env, &oracle);
    }

    /// Set the oracle price token for a strategy. Manager only.
    ///
    /// When `nav()` is computed it will call `oracle.get_price(price_token)` and
    /// multiply by `strategy.get_value(vault)` to get the base-asset value.
    /// This is only supported for single-asset strategies.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`] / [`VaultError::StrategyNotWhitelisted`]
    pub fn set_strategy_oracle_token(
        env: Env,
        caller: Address,
        strategy: Address,
        price_token: Address,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if !Self::is_strategy_whitelisted(&env, &strategy) {
            panic_with_error!(&env, VaultError::StrategyNotWhitelisted);
        }
        if is_lp_strategy(&env, &strategy) {
            panic_with_error!(&env, VaultError::LpStrategyOracleUnsupported);
        }
        set_strategy_price_token(&env, &strategy, &price_token);
    }

    /// Set the maximum single-strategy concentration in basis points. Manager only.
    ///
    /// After each `invest` or `invest_lp` call the vault checks whether the
    /// strategy's share of total NAV exceeds this cap.  Set `0` to disable
    /// the limit.  Maximum meaningful value is `10_000` (100 %, i.e. uncapped).
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    pub fn set_max_concentration_bps(env: Env, caller: Address, bps: u32) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if bps > FEE_DENOMINATOR {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        set_max_concentration_bps(&env, bps);
    }

    /// Mark a strategy as an LP (two-asset) strategy. Manager only.
    ///
    /// LP strategies are skipped during the proportional auto-unwind that
    /// occurs when the vault's base-asset balance is insufficient to cover
    /// a withdrawal.  The manager must manually `unwind_lp` LP positions.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`] / [`VaultError::StrategyNotWhitelisted`]
    pub fn set_lp_strategy(env: Env, caller: Address, strategy: Address, is_lp: bool) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if !Self::is_strategy_whitelisted(&env, &strategy) {
            panic_with_error!(&env, VaultError::StrategyNotWhitelisted);
        }
        if !is_lp && is_lp_strategy(&env, &strategy) {
            panic_with_error!(&env, VaultError::LpStrategyFlagImmutable);
        }
        set_lp_strategy(&env, &strategy, is_lp);
    }

    /// Set the vault's maximum deposit cap (in base-asset units). Manager only.
    ///
    /// A cap of `0` means **uncapped** (no limit).  Set to a positive value to
    /// prevent the vault from accepting deposits beyond that NAV ceiling.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    pub fn set_deposit_cap(env: Env, caller: Address, cap: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        if cap < 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        set_deposit_cap(&env, cap);
    }

    /// Enable or disable private-pool mode. Manager only.
    pub fn set_private_pool(env: Env, caller: Address, is_private: bool) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_private_pool(&env, is_private);
    }

    /// Add an allowlisted member for private-pool deposits. Manager only.
    pub fn add_member(env: Env, caller: Address, member: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_member(&env, &member, true);
    }

    /// Remove an allowlisted member for private-pool deposits. Manager only.
    pub fn remove_member(env: Env, caller: Address, member: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_member(&env, &member, false);
        clear_op_state(&env, &member);
    }

    /// Set per-user exit cooldown in seconds. Manager only.
    pub fn set_exit_cooldown_secs(env: Env, caller: Address, secs: u64) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_exit_cooldown_secs(&env, secs);
    }

    /// Toggle same-ledger value-manipulation guard. Manager only.
    pub fn set_value_guard_enabled(env: Env, caller: Address, enabled: bool) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_value_manipulation_guard_enabled(&env, enabled);
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// Return the current NAV (vault base-asset balance + strategy values).
    pub fn get_nav(env: Env) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);
        Self::nav(&env, &vault, &base_asset)
    }

    /// Return the current share price (NAV / total_supply) in PRICE_PRECISION units.
    pub fn get_share_price(env: Env) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);
        let share_token = get_share_token(&env);
        let nav = Self::nav(&env, &vault, &base_asset);
        let total_supply = share::total_supply(&env, &share_token);
        Self::share_price(nav, total_supply)
    }

    pub fn get_manager(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_manager(&env)
    }

    pub fn get_trader(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_trader(&env)
    }

    pub fn get_base_asset(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_base_asset(&env)
    }

    pub fn get_share_token(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_share_token(&env)
    }

    pub fn get_strategies(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_strategies(&env)
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_paused(&env)
    }

    pub fn is_private_pool(env: Env) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_private_pool(&env)
    }

    pub fn is_member_allowed(env: Env, member: Address) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        is_member(&env, &member)
    }

    pub fn get_exit_cooldown_secs(env: Env) -> u64 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_exit_cooldown_secs(&env)
    }

    pub fn get_exit_remaining_cooldown(env: Env, user: Address) -> u64 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let cooldown = get_exit_cooldown_secs(&env);
        if cooldown == 0 {
            return 0;
        }
        let last_deposit = match get_last_deposit_ts_opt(&env, &user) {
            Some(ts) => ts,
            None => return 0,
        };
        let now = env.ledger().timestamp();
        let unlock = last_deposit.saturating_add(cooldown);
        if now >= unlock {
            0
        } else {
            unlock - now
        }
    }

    pub fn get_announced_fees(env: Env) -> AnnouncedFees {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        AnnouncedFees {
            entry_fee_bps: get_announced_entry_fee_bps(&env),
            exit_fee_bps: get_announced_exit_fee_bps(&env),
            mgmt_fee_bps: get_announced_mgmt_fee_bps(&env),
            perf_fee_bps: get_announced_perf_fee_bps(&env),
            activation_ts: get_announced_fee_activation_ts(&env),
        }
    }

    pub fn is_value_guard_enabled(env: Env) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_value_manipulation_guard_enabled(&env)
    }

    pub fn get_name(_env: Env) -> String {
        // Name is stored in the share token.
        String::from_str(&_env, "Stellar Asset Management Vault")
    }

    // -----------------------------------------------------------------------
    // Internal math helpers
    // -----------------------------------------------------------------------

    fn nav(env: &Env, vault: &Address, base_asset: &Address) -> i128 {
        let vault_balance = token::Client::new(env, base_asset).balance(vault);
        let strategies = get_strategies(env);
        let mut strategy_value: i128 = 0;
        for strategy in strategies.iter() {
            let v = Self::strategy_value_in_nav(env, vault, &strategy);
            strategy_value = strategy_value.saturating_add(v);
        }
        vault_balance.saturating_add(strategy_value)
    }

    fn share_price(nav: i128, total_supply: i128) -> i128 {
        if total_supply == 0 || nav == 0 {
            PRICE_PRECISION // bootstrap price = 1.0
        } else {
            nav * PRICE_PRECISION / total_supply
        }
    }

    fn is_strategy_whitelisted(env: &Env, strategy: &Address) -> bool {
        let strategies = get_strategies(env);
        for s in strategies.iter() {
            if &s == strategy {
                return true;
            }
        }
        false
    }

    fn op_guard_pre_check(env: &Env, actor: &Address, op_type: u32, nav_before: i128) {
        if !get_value_manipulation_guard_enabled(env) {
            return;
        }
        let now_ledger = env.ledger().sequence();
        if let Some(state) = get_op_state(env, actor) {
            if state.ledger == now_ledger {
                if state.op_type != op_type {
                    panic_with_error!(env, VaultError::OperationTypeMismatch);
                }
                if state.expected_nav_after != nav_before {
                    panic_with_error!(env, VaultError::ValueManipulationDetected);
                }
            } else {
                clear_op_state(env, actor);
            }
        }
    }

    fn op_guard_post_checkpoint(env: &Env, actor: &Address, op_type: u32, nav_after: i128) {
        if !get_value_manipulation_guard_enabled(env) {
            return;
        }
        let state = OperationState {
            ledger: env.ledger().sequence(),
            op_type,
            expected_nav_after: nav_after,
        };
        set_op_state(env, actor, &state);
    }

    /// Convert a strategy's raw value into base-asset NAV terms.
    ///
    /// If an oracle is configured and a price token is registered for the
    /// strategy, this applies `raw * price / PRICE_PRECISION`.
    ///
    /// LP strategies must provide internal oracle-backed valuation via their
    /// own `get_value` implementation; if they hold a non-zero position without
    /// an internal oracle configured, this function reverts.
    fn strategy_value_in_nav(env: &Env, vault: &Address, strategy: &Address) -> i128 {
        let raw_v = strategy_get_value(env, strategy, vault);
        if raw_v == 0 {
            return 0;
        }
        if is_lp_strategy(env, strategy) {
            if !strategy_has_oracle(env, strategy) {
                panic_with_error!(env, VaultError::LpStrategyOracleRequired);
            }
            // LP strategies are valued internally (reserve decomposition) and
            // therefore already return base-asset NAV units from get_value().
            return raw_v;
        }
        if let Some(ref oracle_addr) = get_oracle(env) {
            if let Some(price_token) = get_strategy_price_token(env, strategy) {
                let price_args = (price_token,).into_val(env);
                let price: i128 =
                    env.invoke_contract(oracle_addr, &Symbol::new(env, "get_price"), price_args);
                return raw_v.saturating_mul(price) / PRICE_PRECISION;
            }
        }
        raw_v
    }
}

#[cfg(test)]
mod test;
