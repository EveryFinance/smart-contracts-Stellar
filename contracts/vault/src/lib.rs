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
    auth::{ContractContext, InvokerContractAuthEntry, SubContractInvocation},
    contract, contractimpl, contracttype, panic_with_error, token, Address, Env, IntoVal, String,
    Symbol, TryFromVal, Val, Vec,
};

use storage::{
    clear_announced_fees, clear_op_state, get_active_guards, get_admin,
    get_announced_entry_fee_bps, get_announced_exit_fee_bps, get_announced_fee_activation_ts,
    get_announced_mgmt_fee_bps, get_announced_perf_fee_bps, get_authorized_ops, get_base_asset,
    get_deposit_assets, get_deposit_cap, get_entry_fee_bps, get_exit_cooldown_secs,
    get_exit_fee_bps, get_factory, get_high_water_mark, get_last_deposit_ts_opt,
    get_last_mgmt_fee_ts, get_manager, get_manager_name, get_max_loss_bps, get_mgmt_fee_bps,
    get_op_state, get_ops_paused, get_oracle, get_paused, get_perf_fee_bps, get_portfolio_assets,
    get_private_pool, get_share_token, get_trader, get_treasury, get_user_position,
    get_value_manipulation_guard_enabled, is_initialized, is_member, is_seed_deposited,
    set_active_guards, set_admin, set_announced_entry_fee_bps, set_announced_exit_fee_bps,
    set_announced_fee_activation_ts, set_announced_mgmt_fee_bps, set_announced_perf_fee_bps,
    set_authorized_ops, set_base_asset, set_deposit_assets, set_deposit_cap, set_entry_fee_bps,
    set_exit_cooldown_secs, set_exit_fee_bps, set_factory, set_high_water_mark,
    set_last_deposit_ts, set_last_mgmt_fee_ts, set_manager, set_manager_name, set_max_loss_bps,
    set_member, set_mgmt_fee_bps, set_op_state, set_ops_paused, set_oracle, set_paused,
    set_perf_fee_bps, set_portfolio_assets, set_private_pool, set_seed_deposited, set_share_token,
    set_trader, set_treasury, set_user_position, set_value_manipulation_guard_enabled,
    OperationState, FEE_DENOMINATOR, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
    MAX_ENTRY_EXIT_FEE_BPS, MAX_GUARDS, MAX_MGMT_FEE_BPS, MAX_PERF_FEE_BPS, MAX_PORTFOLIO_ASSETS,
    SECONDS_PER_YEAR,
};

use events::{
    deposit_event, execute_op_event, manager_changed_event, mgmt_fee_event, pause_event,
    perf_fee_event, trader_changed_event, withdraw_event,
};

/// Fixed-point precision for share-price calculations (1e7 = Stellar decimals).
const PRICE_PRECISION: i128 = 10_000_000;
/// Default NAV-loss guard tolerance (10%) for manager/trader operations.
const DEFAULT_MAX_LOSS_BPS: u32 = 1_000;
/// Delay before announced fee increases can be committed.
const FEE_INCREASE_DELAY_SECS: u64 = 86_400;

const OP_DEPOSIT: u32 = 1;
const OP_WITHDRAW: u32 = 2;

fn is_reserved_guard_function(env: &Env, fn_name: &Symbol) -> bool {
    *fn_name == Symbol::new(env, "withdraw_fraction")
        || *fn_name == Symbol::new(env, "get_total_value")
        || *fn_name == Symbol::new(env, "asset_in_use")
        || *fn_name == Symbol::new(env, "pause")
        || *fn_name == Symbol::new(env, "unpause")
        || *fn_name == Symbol::new(env, "initialize")
        || *fn_name == Symbol::new(env, "deposit")
        || *fn_name == Symbol::new(env, "withdraw")
        || *fn_name == Symbol::new(env, "deposit_liquidity")
        || *fn_name == Symbol::new(env, "get_value")
        || *fn_name == Symbol::new(env, "get_share_balance")
        || *fn_name == Symbol::new(env, "get_lp_balance")
        || *fn_name == Symbol::new(env, "get_router")
        || *fn_name == Symbol::new(env, "get_protocol_address")
}

fn arg_as_i128(env: &Env, args: &Vec<Val>, idx: u32) -> i128 {
    args.get(idx)
        .and_then(|v| i128::try_from_val(env, &v).ok())
        .unwrap_or_else(|| panic_with_error!(env, VaultError::TradeGuardRejected))
}

fn arg_as_bool(env: &Env, args: &Vec<Val>, idx: u32) -> bool {
    args.get(idx)
        .and_then(|v| bool::try_from_val(env, &v).ok())
        .unwrap_or_else(|| panic_with_error!(env, VaultError::TradeGuardRejected))
}

fn arg_as_address(env: &Env, args: &Vec<Val>, idx: u32) -> Address {
    args.get(idx)
        .and_then(|v| Address::try_from_val(env, &v).ok())
        .unwrap_or_else(|| panic_with_error!(env, VaultError::TradeGuardRejected))
}

fn push_transfer_auth(
    env: &Env,
    entries: &mut Vec<InvokerContractAuthEntry>,
    token: Address,
    from: Address,
    to: Address,
    amount: i128,
) {
    if amount <= 0 {
        panic_with_error!(env, VaultError::InvalidAmount);
    }
    entries.push_back(InvokerContractAuthEntry::Contract(SubContractInvocation {
        context: ContractContext {
            contract: token,
            fn_name: Symbol::new(env, "transfer"),
            args: (from, to, amount).into_val(env),
        },
        sub_invocations: Vec::new(env),
    }));
}

fn assert_transfer_asset_allowed(env: &Env, asset: &Address) {
    let portfolio = get_portfolio_assets(env);
    if portfolio.is_empty() {
        if *asset != get_base_asset(env) {
            panic_with_error!(env, VaultError::AssetNotInPortfolio);
        }
        return;
    }
    if !portfolio.contains(asset.clone()) {
        panic_with_error!(env, VaultError::AssetNotInPortfolio);
    }
    if let Some(factory) = get_factory(env) {
        let is_auth: bool = env.invoke_contract(
            &factory,
            &Symbol::new(env, "is_authorized_asset"),
            (asset.clone(),).into_val(env),
        );
        if !is_auth {
            panic_with_error!(env, VaultError::AssetNotAuthorized);
        }
    }
}

fn strategy_asset(env: &Env, guard: &Address, fn_name: &str) -> Address {
    env.invoke_contract(guard, &Symbol::new(env, fn_name), ().into_val(env))
}

fn authorize_execute_op_transfers(
    env: &Env,
    vault: &Address,
    guard: &Address,
    fn_name: &Symbol,
    args: &Vec<Val>,
) {
    let mut entries: Vec<InvokerContractAuthEntry> = Vec::new(env);
    let guard_addr = guard.clone();

    if *fn_name == Symbol::new(env, "supply") {
        let asset = strategy_asset(env, guard, "asset");
        assert_transfer_asset_allowed(env, &asset);
        let amount = arg_as_i128(env, args, 0);
        push_transfer_auth(env, &mut entries, asset, vault.clone(), guard_addr, amount);
    } else if *fn_name == Symbol::new(env, "add_liquidity") {
        let asset_a = strategy_asset(env, guard, "asset_a");
        let asset_b = strategy_asset(env, guard, "asset_b");
        assert_transfer_asset_allowed(env, &asset_a);
        assert_transfer_asset_allowed(env, &asset_b);
        let amount_a = arg_as_i128(env, args, 0);
        let amount_b = arg_as_i128(env, args, 1);
        push_transfer_auth(
            env,
            &mut entries,
            asset_a,
            vault.clone(),
            guard_addr.clone(),
            amount_a,
        );
        push_transfer_auth(
            env,
            &mut entries,
            asset_b,
            vault.clone(),
            guard_addr,
            amount_b,
        );
    } else if *fn_name == Symbol::new(env, "swap") {
        if args.len() == 4 {
            let from_asset = arg_as_address(env, args, 0);
            let to_asset = arg_as_address(env, args, 1);
            let asset_a = strategy_asset(env, guard, "asset_a");
            let asset_b = strategy_asset(env, guard, "asset_b");
            let valid_pair = (from_asset == asset_a && to_asset == asset_b)
                || (from_asset == asset_b && to_asset == asset_a);
            if !valid_pair {
                panic_with_error!(env, VaultError::TradeGuardRejected);
            }
            assert_transfer_asset_allowed(env, &from_asset);
            let amount_in = arg_as_i128(env, args, 2);
            push_transfer_auth(
                env,
                &mut entries,
                from_asset,
                vault.clone(),
                guard_addr,
                amount_in,
            );
        } else if args.len() == 3 {
            let sell_a = arg_as_bool(env, args, 0);
            let asset = if sell_a {
                strategy_asset(env, guard, "asset_a")
            } else {
                strategy_asset(env, guard, "asset_b")
            };
            assert_transfer_asset_allowed(env, &asset);
            let amount_in = arg_as_i128(env, args, 1);
            push_transfer_auth(
                env,
                &mut entries,
                asset,
                vault.clone(),
                guard_addr,
                amount_in,
            );
        } else {
            panic_with_error!(env, VaultError::TradeGuardRejected);
        }
    }

    if !entries.is_empty() {
        env.authorize_as_current_contract(entries);
    }
}

fn asset_handler(env: &Env) -> Option<Address> {
    get_factory(env).and_then(|factory| {
        env.invoke_contract(
            &factory,
            &Symbol::new(env, "get_asset_handler"),
            ().into_val(env),
        )
    })
}

fn price_asset_to_base(
    env: &Env,
    asset: &Address,
    base_asset: &Address,
    amount: i128,
    asset_handler_opt: &Option<Address>,
    oracle_opt: &Option<Address>,
) -> i128 {
    if amount == 0 {
        return 0;
    }
    if *asset == *base_asset {
        return amount;
    }

    let price: i128 = if let Some(ref ah) = asset_handler_opt {
        env.invoke_contract(
            ah,
            &Symbol::new(env, "get_price"),
            (asset.clone(),).into_val(env),
        )
    } else if let Some(ref oracle) = oracle_opt {
        env.invoke_contract(
            oracle,
            &Symbol::new(env, "get_price"),
            (asset.clone(),).into_val(env),
        )
    } else {
        panic_with_error!(env, VaultError::OracleRequired);
    };

    if price <= 0 {
        panic_with_error!(env, VaultError::InvalidOraclePrice);
    }
    checked_mul_div(env, amount, price, PRICE_PRECISION)
}

fn checked_add(env: &Env, a: i128, b: i128) -> i128 {
    a.checked_add(b)
        .unwrap_or_else(|| panic_with_error!(env, VaultError::Overflow))
}

fn checked_sub(env: &Env, a: i128, b: i128) -> i128 {
    a.checked_sub(b)
        .unwrap_or_else(|| panic_with_error!(env, VaultError::Overflow))
}

fn checked_mul_div(env: &Env, a: i128, b: i128, denominator: i128) -> i128 {
    if denominator <= 0 {
        panic_with_error!(env, VaultError::InvalidAmount);
    }
    a.checked_mul(b)
        .map(|v| v / denominator)
        .unwrap_or_else(|| panic_with_error!(env, VaultError::Overflow))
}

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

    pub fn set_transfers_enabled(env: &Env, share_token: &Address, enabled: bool) {
        let args = (enabled,).into_val(env);
        env.invoke_contract::<()>(
            share_token,
            &Symbol::new(env, "set_transfers_enabled"),
            args,
        );
    }

    pub fn transfers_enabled(env: &Env, share_token: &Address) -> bool {
        let args = ().into_val(env);
        env.invoke_contract(share_token, &Symbol::new(env, "transfers_enabled"), args)
    }
}

// ---------------------------------------------------------------------------
// Vault initialization params
// ---------------------------------------------------------------------------

/// Parameters passed to [`Vault::initialize`].
#[contracttype]
#[derive(Clone, Debug)]
pub struct VaultParams {
    /// Vault admin — can change manager and treasury.  Separate from manager.
    pub admin: Address,
    /// Account that manages strategies, fees, and pausing.
    pub manager: Address,
    /// Optional human-readable name of the manager (for display).
    pub manager_name: Option<String>,
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
    /// Address that receives all fee payments (entry, mgmt, perf).
    pub treasury: Address,
    /// Entry fee in basis points (0–500).
    pub entry_fee_bps: u32,
    /// Exit fee in basis points (0–500).
    pub exit_fee_bps: u32,
    /// Annual management fee in basis points (0–300).
    pub mgmt_fee_bps: u32,
    /// Performance fee in basis points (0–3000).
    pub perf_fee_bps: u32,
    /// Optional factory address. When provided, the vault records it atomically
    /// at construction time so `add_portfolio_asset` / `add_active_guard` can
    /// validate against the factory's global whitelist.
    ///
    /// Pass `None` for standalone vaults that do not use the factory registry.
    pub factory: Option<Address>,

    /// Whether the vault starts in private-pool mode (member-only deposits).
    /// Can be changed later via `set_private_pool` (admin only).
    pub is_private: bool,
}

/// PnL report returned by `get_user_pnl`.  All values in base-asset units.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UserPnLReport {
    /// Base-asset value paid for currently held shares.
    pub cost_basis: i128,
    /// Current market value of held shares (shares × share_price / PRICE_PRECISION).
    pub current_value: i128,
    /// current_value − cost_basis.
    pub unrealized_pnl: i128,
    /// Accumulated realized gain/loss from past withdrawals.
    pub realized_pnl: i128,
    /// unrealized_pnl + realized_pnl.
    pub total_pnl: i128,
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

    /// Initialize the vault. Runs atomically at deployment via `CreateContract`.
    ///
    /// # Errors
    /// * [`VaultError::InvalidAmount`] — if any fee exceeds its cap.
    pub fn __constructor(env: Env, params: VaultParams) {
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

        set_admin(&env, &params.admin);
        set_manager(&env, &params.manager);
        set_trader(&env, &params.trader);
        set_base_asset(&env, &params.base_asset);
        set_share_token(&env, &params.share_token);
        set_treasury(&env, &params.treasury);
        set_entry_fee_bps(&env, params.entry_fee_bps);
        set_exit_fee_bps(&env, params.exit_fee_bps);
        set_mgmt_fee_bps(&env, params.mgmt_fee_bps);
        set_perf_fee_bps(&env, params.perf_fee_bps);
        set_paused(&env, false);
        set_ops_paused(&env, false);
        set_private_pool(&env, params.is_private);
        set_exit_cooldown_secs(&env, 0);
        set_value_manipulation_guard_enabled(&env, false);
        clear_announced_fees(&env);
        set_max_loss_bps(&env, DEFAULT_MAX_LOSS_BPS);

        if let Some(ref name) = params.manager_name {
            set_manager_name(&env, name);
        }

        let now = env.ledger().timestamp();
        set_last_mgmt_fee_ts(&env, now);
        set_high_water_mark(&env, PRICE_PRECISION); // initial NAV/share = 1.0
        if let Some(ref factory) = params.factory {
            set_factory(&env, factory);
        }
    }

    // -----------------------------------------------------------------------
    // Deposit
    // -----------------------------------------------------------------------

    /// Deposit `amount` of any whitelisted deposit asset and receive share tokens.
    ///
    /// Multi-asset v2: if `deposit_assets` list is configured, any asset in that
    /// list is accepted.  Non-base assets are oracle-priced to compute the
    /// base-asset-denominated deposit value used for share minting.
    ///
    /// Legacy mode (no `portfolio_assets` configured): only `base_asset` accepted.
    ///
    /// Flow (dHedge V2 model):
    /// 1. Collect streaming management and performance fees.
    /// 2. Snapshot NAV before transfer (oracle pricing).
    /// 3. Pull `amount` of `asset` from `from` into vault.
    /// 4. `deposit_value` = oracle.price(asset) × amount  (base-asset terms).
    /// 5. `total_shares` = deposit_value × total_supply / nav_before.
    /// 6. Split entry fee as shares minted to manager.
    /// 7. Update user's cost_basis for PnL tracking.
    ///
    /// # Returns
    /// Number of share tokens minted to `from`.
    ///
    /// # Errors
    /// * [`VaultError::Paused`]
    /// * [`VaultError::InvalidAmount`]
    /// * [`VaultError::AssetNotInPortfolio`] — not in deposit asset list
    /// * [`VaultError::DepositCapExceeded`]
    pub fn deposit(
        env: Env,
        amount: i128,
        from: Address,
        asset: Address,
        min_shares_out: i128,
    ) -> i128 {
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
        let treasury = get_treasury(&env);

        if get_private_pool(&env) && from != manager && !is_member(&env, &from) {
            panic_with_error!(&env, VaultError::NotMember);
        }

        // Validate deposit asset.
        let deposit_assets = get_deposit_assets(&env);
        let portfolio = get_portfolio_assets(&env);
        if !portfolio.is_empty() {
            // Multi-asset mode: asset must be in deposit_assets list.
            if !deposit_assets.contains(asset.clone()) {
                panic_with_error!(&env, VaultError::AssetNotInPortfolio);
            }
            if let Some(factory) = get_factory(&env) {
                let is_auth: bool = env.invoke_contract(
                    &factory,
                    &Symbol::new(&env, "is_authorized_asset"),
                    (asset.clone(),).into_val(&env),
                );
                if !is_auth {
                    panic_with_error!(&env, VaultError::AssetNotAuthorized);
                }
            }
        } else if asset != base_asset {
            // Legacy mode: only base_asset accepted.
            panic_with_error!(&env, VaultError::AssetNotInPortfolio);
        }

        // Accrue fees before changing supply.
        Self::collect_fees(&env, &vault, &share_token, &manager);

        // Snapshot NAV and total supply BEFORE transfer.
        let nav = Self::nav(&env, &vault, &base_asset);
        Self::op_guard_pre_check(&env, &from, OP_DEPOSIT, nav);
        let total_supply = share::total_supply(&env, &share_token);
        let share_price = Self::share_price(&env, nav, total_supply);

        // Convert deposits with the same price source used by NAV: factory
        // AssetHandler first, then the legacy vault-wide oracle fallback.
        let asset_handler_opt = asset_handler(&env);
        let oracle_opt = get_oracle(&env);
        let deposit_value = price_asset_to_base(
            &env,
            &asset,
            &base_asset,
            amount,
            &asset_handler_opt,
            &oracle_opt,
        );

        // Total shares the deposit value buys at current price.
        let total_shares = if share_price == 0 || total_supply == 0 {
            deposit_value
        } else {
            checked_mul_div(&env, deposit_value, PRICE_PRECISION, share_price)
        };

        let entry_fee_bps = get_entry_fee_bps(&env) as i128;
        let fee_shares =
            checked_mul_div(&env, total_shares, entry_fee_bps, FEE_DENOMINATOR as i128);
        let user_shares = checked_sub(&env, total_shares, fee_shares);

        if user_shares <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }

        // Deposit cap (checked in base-asset value terms).
        let cap = get_deposit_cap(&env);
        if cap > 0 && checked_add(&env, nav, deposit_value) > cap {
            panic_with_error!(&env, VaultError::DepositCapExceeded);
        }

        // Pull asset from depositor into vault.
        token::Client::new(&env, &asset).transfer(&from, &vault, &amount);

        // Mint shares.
        share::mint(&env, &share_token, &from, user_shares);
        if fee_shares > 0 {
            share::mint(&env, &share_token, &treasury, fee_shares);
        }

        // PnL: increase user cost basis by the deposit value (net, after fee).
        {
            let mut pos = get_user_position(&env, &from);
            pos.cost_basis = checked_add(&env, pos.cost_basis, deposit_value);
            set_user_position(&env, &from, &pos);
        }

        let now = env.ledger().timestamp();
        set_last_deposit_ts(&env, &from, now);
        let nav_after = Self::nav(&env, &vault, &base_asset);
        Self::op_guard_post_checkpoint(&env, &from, OP_DEPOSIT, nav_after);

        if min_shares_out > 0 && user_shares < min_shares_out {
            panic_with_error!(&env, VaultError::SlippageTooHigh);
        }

        deposit_event(&env, &from, amount, user_shares);

        user_shares
    }

    // -----------------------------------------------------------------------
    // Withdraw
    // -----------------------------------------------------------------------

    /// Burn `share_amount` of share tokens and receive a proportional fraction
    /// of every asset in the vault (multi-asset dHedge V2 withdrawal model).
    ///
    /// Flow:
    /// 1. Collect streaming management and performance fees.
    /// 2. Validate share balance and cooldown.
    /// 3. **BURN SHARES FIRST** (reentrancy protection).
    /// 4. Update user's realized PnL.
    /// 5. For each portfolio asset: transfer `fraction × balance` to `to`.
    /// 6. For each active guard: call `withdraw_fraction(vault, num, den, to)`.
    ///    Guards send their underlying tokens directly to `to`.
    ///
    /// Legacy mode (no portfolio_assets configured): same proportional logic
    /// applied to base_asset only, plus legacy single-asset strategies.
    ///
    /// Exit fee fraction stays in the vault — not transferred to manager.
    ///
    /// # Returns
    /// Total value withdrawn in base-asset terms (for events / slippage check).
    ///
    /// # Errors
    /// * [`VaultError::Paused`]
    /// * [`VaultError::InvalidAmount`]
    /// * [`VaultError::InsufficientShares`]
    /// * [`VaultError::CooldownActive`]
    pub fn withdraw(
        env: Env,
        share_amount: i128,
        from: Address,
        to: Address,
        min_base_out: i128,
    ) -> i128 {
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

        let total_supply = share::total_supply(&env, &share_token);
        let nav = Self::nav(&env, &vault, &base_asset);
        Self::op_guard_pre_check(&env, &from, OP_WITHDRAW, nav);
        let share_price = Self::share_price(&env, nav, total_supply);

        // Gross value and exit fee (fee fraction stays in vault).
        let exit_fee_bps = get_exit_fee_bps(&env) as i128;
        // Effective numerator after exit fee: shares_burned × (1 − fee).
        // We apply this to each asset transfer individually.
        let numerator = checked_mul_div(
            &env,
            share_amount,
            FEE_DENOMINATOR as i128 - exit_fee_bps,
            FEE_DENOMINATOR as i128,
        );
        let denominator = total_supply;

        if numerator <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }

        // Slippage: approximate base-asset value to be received.
        let gross_base = checked_mul_div(&env, share_amount, share_price, PRICE_PRECISION);
        let net_base = checked_mul_div(
            &env,
            gross_base,
            FEE_DENOMINATOR as i128 - exit_fee_bps,
            FEE_DENOMINATOR as i128,
        );
        if min_base_out > 0 && net_base < min_base_out {
            panic_with_error!(&env, VaultError::SlippageTooHigh);
        }

        // PnL: compute realized gain/loss before burning shares.
        {
            let mut pos = get_user_position(&env, &from);
            // shares_held_before = share_balance (before fee accrual — approx)
            // We use total_supply for avg cost calculation.
            if pos.cost_basis > 0 && share_balance > 0 {
                let avg_cost_per_share = pos.cost_basis / share_balance;
                let withdrawal_cost = checked_mul_div(&env, avg_cost_per_share, share_amount, 1);
                let withdrawal_value = net_base;
                let realized = checked_sub(&env, withdrawal_value, withdrawal_cost);
                pos.realized_pnl = checked_add(&env, pos.realized_pnl, realized);
                pos.cost_basis = checked_sub(&env, pos.cost_basis, withdrawal_cost).max(0);
            }
            set_user_position(&env, &from, &pos);
        }

        // BURN SHARES FIRST (reentrancy protection).
        share::burn(&env, &share_token, &from, share_amount);

        let portfolio = get_portfolio_assets(&env);

        // Proportional multi-asset withdrawal: distribute each portfolio asset
        // and trigger withdraw_fraction on every active guard.
        if portfolio.is_empty() {
            // No portfolio assets configured: transfer base_asset directly.
            let vault_balance = token::Client::new(&env, &base_asset).balance(&vault);
            if vault_balance < net_base {
                panic_with_error!(&env, VaultError::InsufficientLiquidity);
            }
            token::Client::new(&env, &base_asset).transfer(&vault, &to, &net_base);
        } else {
            // Multi-asset mode: proportional across all portfolio assets.
            for asset in portfolio.iter() {
                let bal = token::Client::new(&env, &asset).balance(&vault);
                if bal == 0 {
                    continue;
                }
                let amt = checked_mul_div(&env, bal, numerator, denominator);
                if amt > 0 {
                    token::Client::new(&env, &asset).transfer(&vault, &to, &amt);
                }
            }
            // Call withdraw_fraction on each active guard (tokens go directly to user).
            let guards = get_active_guards(&env);
            for guard in guards.iter() {
                env.invoke_contract::<()>(
                    &guard,
                    &Symbol::new(&env, "withdraw_fraction"),
                    (vault.clone(), numerator, denominator, to.clone()).into_val(&env),
                );
            }
        }

        let nav_after = Self::nav(&env, &vault, &base_asset);
        Self::op_guard_post_checkpoint(&env, &from, OP_WITHDRAW, nav_after);

        withdraw_event(&env, &to, share_amount, net_base);

        net_base
    }

    // -----------------------------------------------------------------------
    // Guard-dispatched operations (manager or trader)
    // -----------------------------------------------------------------------

    /// Execute any authorised operation through a whitelisted guard contract.
    ///
    /// This is the single unified dispatch entry-point replacing the old
    /// `invest` / `unwind` / `invest_lp` / `unwind_lp` / `execute_trade`
    /// family of functions.
    ///
    /// # Flow
    /// 1. `caller` must be the registered manager **or** trader — both may call.
    /// 2. `guard` must appear in `ActiveGuards`.
    /// 3. `fn_name` must appear in `AuthorizedOps(guard)`.
    /// 4. NAV is snapshotted before dispatch.
    /// 5. The vault **injects its own address** as the first argument, then
    ///    appends the caller-supplied `args`.  This ensures the guard can only
    ///    act on behalf of the vault — the caller cannot substitute a different
    ///    source address.
    /// 6. `guard.<fn_name>(vault, args…)` is called directly by name.
    /// 7. NAV is re-measured.  If the drop exceeds `max_loss_bps` the
    ///    transaction reverts (TVL guard).
    ///
    /// # Arguments
    /// * `caller`  — Manager or trader address (must `require_auth()`).
    /// * `guard`   — Active guard contract address.
    /// * `fn_name` — Guard function name to call (e.g. `"supply"`, `"swap"`).
    /// * `args`    — Extra arguments appended after the injected vault address.
    ///
    /// # Errors
    /// * [`VaultError::NotTrader`]              — caller is not manager or trader
    /// * [`VaultError::StrategyNotWhitelisted`] — guard not in ActiveGuards
    /// * [`VaultError::TradeGuardRejected`]     — fn_name not in AuthorizedOps
    /// * [`VaultError::TvlGuardTripped`]        — NAV drop exceeded max_loss_bps
    pub fn execute_op(
        env: Env,
        caller: Address,
        guard: Address,
        fn_name: Symbol,
        args: soroban_sdk::Vec<soroban_sdk::Val>,
    ) -> soroban_sdk::Val {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if get_ops_paused(&env) {
            panic_with_error!(&env, VaultError::OperationsPaused);
        }
        let trader = get_trader(&env);
        let manager = get_manager(&env);
        if caller != trader && caller != manager {
            panic_with_error!(&env, VaultError::NotTrader);
        }

        // Guard must be in the active-guards list.
        let guards = get_active_guards(&env);
        if !guards.contains(guard.clone()) {
            panic_with_error!(&env, VaultError::StrategyNotWhitelisted);
        }
        if let Some(factory) = get_factory(&env) {
            let is_auth: bool = env.invoke_contract(
                &factory,
                &Symbol::new(&env, "is_authorized_guard"),
                (guard.clone(),).into_val(&env),
            );
            if !is_auth {
                panic_with_error!(&env, VaultError::GuardNotAuthorized);
            }
        }

        // fn_name must be in the authorized set for this guard.
        if is_reserved_guard_function(&env, &fn_name) {
            panic_with_error!(&env, VaultError::TradeGuardRejected);
        }
        let auth_ops = get_authorized_ops(&env, &guard);
        if !auth_ops.contains(fn_name.clone()) {
            panic_with_error!(&env, VaultError::TradeGuardRejected);
        }

        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);

        // Snapshot NAV before dispatch.
        let max_loss_bps = get_max_loss_bps(&env);
        let nav_before = if max_loss_bps > 0 {
            Self::nav(&env, &vault, &base_asset)
        } else {
            0
        };

        // Build full_args: vault address injected first, then caller-provided args.
        // This guarantees the guard can only move funds FROM the vault.
        let mut full_args: soroban_sdk::Vec<soroban_sdk::Val> = soroban_sdk::Vec::new(&env);
        full_args.push_back(vault.clone().into_val(&env));
        for arg in args.iter() {
            full_args.push_back(arg);
        }

        authorize_execute_op_transfers(&env, &vault, &guard, &fn_name, &args);

        // Dispatch directly to the guard's named function.
        let result: soroban_sdk::Val = env.invoke_contract(&guard, &fn_name, full_args);

        // TVL guard: revert if NAV dropped beyond tolerance.
        if max_loss_bps > 0 {
            let nav_after = Self::nav(&env, &vault, &base_asset);
            let allowed_loss = checked_mul_div(
                &env,
                nav_before,
                max_loss_bps as i128,
                FEE_DENOMINATOR as i128,
            );
            let min_nav = checked_sub(&env, nav_before, allowed_loss);
            if nav_after < min_nav {
                panic_with_error!(&env, VaultError::TvlGuardTripped);
            }
        }

        execute_op_event(&env, &guard, &fn_name, &args);

        result
    }

    // -----------------------------------------------------------------------
    // Fee collection
    // -----------------------------------------------------------------------

    /// Permissionlessly settle pending management and performance fees.
    ///
    /// Fees are deterministic from vault state: the caller cannot choose the
    /// fee amount, recipient, NAV, or timestamp. This lets keepers, users, UIs,
    /// or the manager sync fee accounting even when no deposit/withdraw occurs.
    ///
    /// Returns the number of fee shares minted to the treasury.
    pub fn collect_pending_fees(env: Env) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        let vault = env.current_contract_address();
        let share_token = get_share_token(&env);
        let manager = get_manager(&env);
        Self::collect_fees(&env, &vault, &share_token, &manager)
    }

    /// Accrue management fee and performance fee, minting shares to treasury.
    fn collect_fees(env: &Env, vault: &Address, share_token: &Address, _manager: &Address) -> i128 {
        let total_supply = share::total_supply(env, share_token);
        if total_supply == 0 {
            // Nothing to accrue yet.
            let now = env.ledger().timestamp();
            set_last_mgmt_fee_ts(env, now);
            return 0;
        }

        // All fees go to the treasury.
        let treasury = get_treasury(env);
        let base_asset = get_base_asset(env);
        let nav = Self::nav(env, vault, &base_asset);
        let mut minted_shares = 0i128;

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
                let fee_value = nav
                    .checked_mul(mgmt_fee_bps)
                    .and_then(|v| v.checked_mul(elapsed))
                    .map(|v| v / (FEE_DENOMINATOR as i128 * SECONDS_PER_YEAR as i128))
                    .unwrap_or_else(|| panic_with_error!(env, VaultError::Overflow));

                if fee_value > 0 {
                    // Convert fee value to shares at current price.
                    let price = Self::share_price(env, nav, total_supply);
                    let fee_shares = checked_mul_div(env, fee_value, PRICE_PRECISION, price.max(1));
                    if fee_shares > 0 {
                        share::mint(env, share_token, &treasury, fee_shares);
                        minted_shares = checked_add(env, minted_shares, fee_shares);
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
            let current_price = Self::share_price(env, new_nav, new_supply);
            let hwm = get_high_water_mark(env);

            if current_price > hwm && hwm > 0 {
                let gain = current_price - hwm;
                // fee_value = total_supply * gain * perf_fee_bps / (PRICE_PRECISION * 10_000)
                let fee_value = new_supply
                    .checked_mul(gain)
                    .and_then(|v| v.checked_mul(perf_fee_bps))
                    .map(|v| v / (PRICE_PRECISION * FEE_DENOMINATOR as i128))
                    .unwrap_or_else(|| panic_with_error!(env, VaultError::Overflow));

                if fee_value > 0 {
                    let fee_shares =
                        checked_mul_div(env, fee_value, PRICE_PRECISION, current_price.max(1));
                    if fee_shares > 0 {
                        share::mint(env, share_token, &treasury, fee_shares);
                        minted_shares = checked_add(env, minted_shares, fee_shares);
                        perf_fee_event(env, fee_shares, current_price);
                    }
                }
                set_high_water_mark(env, current_price);
            }
        }

        minted_shares
    }

    // -----------------------------------------------------------------------
    // Emergency controls (admin only)
    // -----------------------------------------------------------------------

    /// Pause deposits and withdrawals. Admin only.
    pub fn pause_deposits(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, VaultError::NotAdmin);
        }
        set_paused(&env, true);
        pause_event(&env, true);
    }

    /// Unpause deposits and withdrawals. Admin only.
    pub fn unpause_deposits(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, VaultError::NotAdmin);
        }
        set_paused(&env, false);
        pause_event(&env, false);
    }

    /// Pause manager operations (execute_op). Admin only.
    pub fn pause_operations(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, VaultError::NotAdmin);
        }
        set_ops_paused(&env, true);
    }

    /// Unpause manager operations (execute_op). Admin only.
    pub fn unpause_operations(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, VaultError::NotAdmin);
        }
        set_ops_paused(&env, false);
    }

    // -----------------------------------------------------------------------
    // Role management (manager only)
    // -----------------------------------------------------------------------

    /// Transfer the manager role to a new address.
    ///
    /// The vault admin or the current manager may authorise this call.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    pub fn set_manager(env: Env, caller: Address, new_manager: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        let factory = get_factory(&env);
        let factory_authorized = factory.as_ref().map(|f| caller == *f).unwrap_or(false);
        if caller != get_admin(&env) && caller != get_manager(&env) && !factory_authorized {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_manager(&env, &new_manager);
        manager_changed_event(&env, &new_manager);
    }

    /// Set or update the manager's display name. Admin or manager only.
    pub fn set_manager_name(env: Env, caller: Address, name: String) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) && caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_manager_name(&env, &name);
    }

    /// Set or update the treasury address. Admin only.
    pub fn set_treasury(env: Env, caller: Address, treasury: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        set_treasury(&env, &treasury);
    }

    /// Enable or disable transferable vault shares. Admin only.
    ///
    /// Shares are non-transferable by default so exit cooldown and per-user PnL
    /// accounting remain accurate. If transfers are enabled, cooldown becomes a
    /// same-address friction control and PnL becomes informational unless a
    /// future transfer-aware accounting design is added.
    pub fn set_share_transfers_enabled(env: Env, caller: Address, enabled: bool) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, VaultError::NotAdmin);
        }
        let share_token = get_share_token(&env);
        share::set_transfers_enabled(&env, &share_token, enabled);
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
    /// A typical production value is `100` (1 % per transaction).
    ///
    /// Setting `0` is rejected because a zero tolerance disables the NAV-loss
    /// guard entirely, which would allow a manager to execute a damaging
    /// operation without any on-chain slippage protection.  Use the default
    /// of [`DEFAULT_MAX_LOSS_BPS`] (1 000 bps = 10 %) or another non-zero value.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::InvalidAmount`] — if `bps == 0` or `bps > FEE_DENOMINATOR`
    pub fn set_max_loss_bps(env: Env, caller: Address, bps: u32) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        // Keep the NAV-loss guard active; 10 000 bps would allow total loss.
        if bps == 0 || bps >= FEE_DENOMINATOR {
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

    /// Enable or disable private-pool mode. Admin only.
    pub fn set_private_pool(env: Env, caller: Address, is_private: bool) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, VaultError::NotAdmin);
        }
        set_private_pool(&env, is_private);
    }

    /// Add an allowlisted member for private-pool deposits. Admin only.
    pub fn add_member(env: Env, caller: Address, member: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, VaultError::NotAdmin);
        }
        set_member(&env, &member, true);
    }

    /// Remove an allowlisted member for private-pool deposits. Admin only.
    pub fn remove_member(env: Env, caller: Address, member: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, VaultError::NotAdmin);
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
    // Factory-only lifecycle
    // -----------------------------------------------------------------------

    /// Perform the one-time anti-inflation seed deposit. Factory only.
    ///
    /// The factory calls this after transferring `seed_amount` of base_asset
    /// directly into the vault.  This function mints `seed_amount` shares to
    /// a burn address (all-zeros), ensuring `total_supply > 0` from day one
    /// and eliminating the first-depositor inflation attack.
    ///
    /// Can only be called once per vault (enforced via `SeedDeposited` flag).
    ///
    /// # Errors
    /// * [`VaultError::SeedAlreadyDeposited`]
    pub fn seed_deposit(env: Env, caller: Address, seed_amount: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        let factory =
            get_factory(&env).unwrap_or_else(|| panic_with_error!(&env, VaultError::FactoryNotSet));
        if caller != factory {
            panic_with_error!(&env, VaultError::NotFactory);
        }
        if seed_amount <= 0 {
            panic_with_error!(&env, VaultError::InvalidAmount);
        }
        if is_seed_deposited(&env) {
            panic_with_error!(&env, VaultError::SeedAlreadyDeposited);
        }

        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);
        let vault_balance = token::Client::new(&env, &base_asset).balance(&vault);
        if vault_balance < seed_amount {
            panic_with_error!(&env, VaultError::InsufficientLiquidity);
        }

        let share_token = get_share_token(&env);
        // Burn address: all-zeros, no one controls this key.
        let burn_addr = Address::from_str(
            &env,
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
        );
        share::mint(&env, &share_token, &burn_addr, seed_amount);
        set_seed_deposited(&env);
    }

    // -----------------------------------------------------------------------
    // Portfolio asset management (manager only)
    // -----------------------------------------------------------------------

    /// Add an asset to the vault's portfolio asset list.
    ///
    /// The asset must be in the factory's global authorized asset list.
    /// The oracle must be set before adding non-base assets (needed for NAV).
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::AssetNotAuthorized`] — not in factory whitelist
    /// * [`VaultError::AssetAlreadyPresent`] — already in portfolio
    /// * [`VaultError::TooManyAssets`] — MAX_PORTFOLIO_ASSETS reached
    pub fn add_portfolio_asset(env: Env, caller: Address, asset: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }

        // Validate against factory whitelist.
        if let Some(factory) = get_factory(&env) {
            let is_auth: bool = env.invoke_contract(
                &factory,
                &Symbol::new(&env, "is_authorized_asset"),
                (asset.clone(),).into_val(&env),
            );
            if !is_auth {
                panic_with_error!(&env, VaultError::AssetNotAuthorized);
            }
        }

        let mut portfolio = get_portfolio_assets(&env);
        if portfolio.contains(asset.clone()) {
            panic_with_error!(&env, VaultError::AssetAlreadyPresent);
        }
        if portfolio.len() as usize >= MAX_PORTFOLIO_ASSETS {
            panic_with_error!(&env, VaultError::TooManyAssets);
        }
        portfolio.push_back(asset);
        set_portfolio_assets(&env, &portfolio);
    }

    /// Remove an asset from the vault's portfolio asset list.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::AssetNotInPortfolio`]
    /// * [`VaultError::AssetHasBalance`] — vault still holds this token
    /// * [`VaultError::AssetInUseByGuard`] — a guard has an active position using this asset
    pub fn remove_portfolio_asset(env: Env, caller: Address, asset: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }

        let portfolio = get_portfolio_assets(&env);
        if !portfolio.contains(asset.clone()) {
            panic_with_error!(&env, VaultError::AssetNotInPortfolio);
        }

        // Check 1: vault must have zero balance of this asset.
        let vault = env.current_contract_address();
        let balance = token::Client::new(&env, &asset).balance(&vault);
        if balance > 0 {
            panic_with_error!(&env, VaultError::AssetHasBalance);
        }

        // Check 2: no active guard may use this asset.
        let guards = get_active_guards(&env);
        for guard in guards.iter() {
            let in_use: bool = env.invoke_contract(
                &guard,
                &Symbol::new(&env, "asset_in_use"),
                (vault.clone(), asset.clone()).into_val(&env),
            );
            if in_use {
                panic_with_error!(&env, VaultError::AssetInUseByGuard);
            }
        }

        // Remove from portfolio.
        let mut updated: Vec<Address> = Vec::new(&env);
        for a in portfolio.iter() {
            if a != asset {
                updated.push_back(a);
            }
        }
        set_portfolio_assets(&env, &updated);

        // Also remove from deposit assets if present.
        let deposits = get_deposit_assets(&env);
        let mut updated_dep: Vec<Address> = Vec::new(&env);
        for a in deposits.iter() {
            if a != asset {
                updated_dep.push_back(a);
            }
        }
        set_deposit_assets(&env, &updated_dep);
    }

    /// Return the vault's current portfolio asset list.
    pub fn get_portfolio_assets(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_portfolio_assets(&env)
    }

    // -----------------------------------------------------------------------
    // Deposit asset management (manager only)
    // -----------------------------------------------------------------------

    /// Add an asset to the deposit-allowed list. Must be in portfolio assets.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::AssetNotInPortfolio`]
    /// * [`VaultError::AssetAlreadyPresent`]
    pub fn add_deposit_asset(env: Env, caller: Address, asset: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }

        // Must be in portfolio first.
        if !get_portfolio_assets(&env).contains(asset.clone()) {
            panic_with_error!(&env, VaultError::AssetNotInPortfolio);
        }

        let mut deposits = get_deposit_assets(&env);
        if deposits.contains(asset.clone()) {
            panic_with_error!(&env, VaultError::AssetAlreadyPresent);
        }
        deposits.push_back(asset);
        set_deposit_assets(&env, &deposits);
    }

    /// Remove an asset from the deposit-allowed list.
    ///
    /// Does not remove from portfolio assets. No constraint on balance.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::AssetNotInPortfolio`] — not in deposit list
    pub fn remove_deposit_asset(env: Env, caller: Address, asset: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }

        let deposits = get_deposit_assets(&env);
        if !deposits.contains(asset.clone()) {
            panic_with_error!(&env, VaultError::AssetNotInPortfolio);
        }

        let mut updated: Vec<Address> = Vec::new(&env);
        for a in deposits.iter() {
            if a != asset {
                updated.push_back(a);
            }
        }
        set_deposit_assets(&env, &updated);
    }

    /// Return the vault's current deposit asset list.
    pub fn get_deposit_assets(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_deposit_assets(&env)
    }

    // -----------------------------------------------------------------------
    // Active guard management (manager only)
    // -----------------------------------------------------------------------

    /// Register a strategy guard contract as active for this vault.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::GuardNotAuthorized`] — not in factory whitelist
    /// * [`VaultError::GuardAlreadyActive`]
    /// * [`VaultError::TooManyGuards`]
    pub fn add_active_guard(env: Env, caller: Address, guard: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }

        // Validate against factory whitelist.
        if let Some(factory) = get_factory(&env) {
            let is_auth: bool = env.invoke_contract(
                &factory,
                &Symbol::new(&env, "is_authorized_guard"),
                (guard.clone(),).into_val(&env),
            );
            if !is_auth {
                panic_with_error!(&env, VaultError::GuardNotAuthorized);
            }
        }

        let mut guards = get_active_guards(&env);
        if guards.contains(guard.clone()) {
            panic_with_error!(&env, VaultError::GuardAlreadyActive);
        }
        if guards.len() as usize >= MAX_GUARDS {
            panic_with_error!(&env, VaultError::TooManyGuards);
        }
        guards.push_back(guard);
        set_active_guards(&env, &guards);
    }

    /// Remove a strategy guard contract from the active list.
    ///
    /// # Errors
    /// * [`VaultError::NotManager`]
    /// * [`VaultError::StrategyNotWhitelisted`] — not in active list
    /// * [`VaultError::GuardHasActivePosition`] — guard still holds positions
    pub fn remove_active_guard(env: Env, caller: Address, guard: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }

        let guards = get_active_guards(&env);
        if !guards.contains(guard.clone()) {
            panic_with_error!(&env, VaultError::StrategyNotWhitelisted);
        }

        // Guard must have no active positions.
        let vault = env.current_contract_address();
        let total_value: i128 = env.invoke_contract(
            &guard,
            &Symbol::new(&env, "get_total_value"),
            (vault.clone(),).into_val(&env),
        );
        if total_value != 0 {
            panic_with_error!(&env, VaultError::GuardHasActivePosition);
        }

        let mut updated: Vec<Address> = Vec::new(&env);
        for g in guards.iter() {
            if g != guard {
                updated.push_back(g);
            }
        }
        set_active_guards(&env, &updated);
    }

    /// Return the list of active guard contracts for this vault.
    pub fn get_active_guards(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_active_guards(&env)
    }

    /// Set the authorized function names for a guard contract. Manager only.
    ///
    /// Only the listed function names may be dispatched by the trader through
    /// `execute_op` for the given guard.
    pub fn set_authorized_ops(env: Env, caller: Address, guard: Address, ops: Vec<Symbol>) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_manager(&env) {
            panic_with_error!(&env, VaultError::NotManager);
        }
        let active_guards = get_active_guards(&env);
        if !active_guards.contains(guard.clone()) {
            panic_with_error!(&env, VaultError::StrategyNotWhitelisted);
        }
        for op in ops.iter() {
            if is_reserved_guard_function(&env, &op) {
                panic_with_error!(&env, VaultError::TradeGuardRejected);
            }
        }
        set_authorized_ops(&env, &guard, &ops);
    }

    /// Return the authorized function names for a guard.
    pub fn get_authorized_ops(env: Env, guard: Address) -> Vec<Symbol> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_authorized_ops(&env, &guard)
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
        Self::share_price(&env, nav, total_supply)
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

    pub fn share_transfers_enabled(env: Env) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let share_token = get_share_token(&env);
        share::transfers_enabled(&env, &share_token)
    }

    pub fn exit_cooldown_is_hard_control(env: Env) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let share_token = get_share_token(&env);
        !share::transfers_enabled(&env, &share_token)
    }

    pub fn pnl_tracking_is_accurate(env: Env) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let share_token = get_share_token(&env);
        !share::transfers_enabled(&env, &share_token)
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_paused(&env)
    }

    pub fn is_ops_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_ops_paused(&env)
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
        unlock.saturating_sub(now)
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

    /// Return the PnL breakdown for a user.
    ///
    /// All values are in base-asset terms (PRICE_PRECISION-scaled where noted).
    pub fn get_user_pnl(env: Env, user: Address) -> UserPnLReport {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let vault = env.current_contract_address();
        let base_asset = get_base_asset(&env);
        let share_token = get_share_token(&env);

        let pos = get_user_position(&env, &user);
        let shares = token::Client::new(&env, &share_token).balance(&user);
        let nav = Self::nav(&env, &vault, &base_asset);
        let total_supply = share::total_supply(&env, &share_token);
        let share_price = Self::share_price(&env, nav, total_supply);

        let current_value = checked_mul_div(&env, shares, share_price, PRICE_PRECISION);
        let unrealized_pnl = checked_sub(&env, current_value, pos.cost_basis);
        let total_pnl = checked_add(&env, pos.realized_pnl, unrealized_pnl);

        UserPnLReport {
            cost_basis: pos.cost_basis,
            current_value,
            unrealized_pnl,
            realized_pnl: pos.realized_pnl,
            total_pnl,
        }
    }

    /// Return the vault's factory reference address.
    pub fn get_factory(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_factory(&env)
    }

    /// Return the vault admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_admin(&env)
    }

    /// Return the fee-recipient treasury address.
    pub fn get_treasury(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_treasury(&env)
    }

    /// Return the manager's display name, or `None` if not set.
    pub fn get_manager_name(env: Env) -> Option<String> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_manager_name(&env)
    }

    // -----------------------------------------------------------------------
    // Internal math helpers
    // -----------------------------------------------------------------------

    fn nav(env: &Env, vault: &Address, base_asset: &Address) -> i128 {
        let mut total: i128 = 0;

        let portfolio = get_portfolio_assets(env);
        // Legacy mode has no PortfolioAssets list. Idle base must still be
        // counted or bootstrap deposits can leave the vault with zero NAV.
        if portfolio.is_empty() {
            let bal = token::Client::new(env, base_asset).balance(vault);
            total = checked_add(env, total, bal);
        }

        // AssetHandler lives in the factory; it is the preferred dHedge-style
        // per-asset price source. The vault-wide oracle is kept as fallback.
        let asset_handler_opt = asset_handler(env);
        let oracle_opt = get_oracle(env);
        for asset in portfolio.iter() {
            let bal = token::Client::new(env, &asset).balance(vault);
            total = checked_add(
                env,
                total,
                price_asset_to_base(
                    env,
                    &asset,
                    base_asset,
                    bal,
                    &asset_handler_opt,
                    &oracle_opt,
                ),
            );
        }

        // Active guard positions always contribute regardless of portfolio config.
        // Guards are manager-whitelisted, so their value is always trusted.
        let guards = get_active_guards(env);
        for guard in guards.iter() {
            let v: i128 = env.invoke_contract(
                &guard,
                &Symbol::new(env, "get_total_value"),
                (vault.clone(),).into_val(env),
            );
            if v < 0 {
                panic_with_error!(env, VaultError::InvalidAmount);
            }
            total = checked_add(env, total, v);
        }

        total
    }

    fn share_price(env: &Env, nav: i128, total_supply: i128) -> i128 {
        if total_supply == 0 || nav == 0 {
            PRICE_PRECISION // bootstrap price = 1.0
        } else {
            let p = checked_mul_div(env, nav, PRICE_PRECISION, total_supply);
            // Floor at 1 to prevent a zero share price when
            // nav * PRICE_PRECISION < total_supply (extreme dilution scenario).
            // A zero price would cause withdraw to compute base_gross = 0 and
            // revert with InvalidAmount, permanently locking all funds.
            if p == 0 {
                1
            } else {
                p
            }
        }
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
}

#[cfg(test)]
mod test;
