//! Vault storage layout.
//!
//! All vault state is kept in **instance storage** so it shares the same TTL
//! as the contract instance itself.  Per-account data (balances, allowances)
//! lives in the separate ShareToken contract which uses persistent storage.
//!
//! ## Storage key reference
//!
//! | Key                    | Type           | Description                                    |
//! |------------------------|----------------|------------------------------------------------|
//! | `Manager`              | `Address`      | Fund manager / operator                        |
//! | `Trader`               | `Address`      | Account authorised to call execute_op          |
//! | `BaseAsset`            | `Address`      | Denomination token (e.g. USDC)                 |
//! | `ShareToken`           | `Address`      | SEP-41 share token contract                    |
//! | `Paused`               | `bool`         | Emergency pause flag                           |
//! | `EntryFeeBps`          | `u32`          | Deposit fee in basis points (max 500 bps)      |
//! | `ExitFeeBps`           | `u32`          | Withdrawal fee in basis points (max 500 bps)   |
//! | `MgmtFeeBps`           | `u32`          | Annual management fee (max 300 bps)            |
//! | `PerfFeeBps`           | `u32`          | Performance fee (max 3 000 bps)                |
//! | `LastMgmtFeeTs`        | `u64`          | UNIX timestamp of last management fee accrual  |
//! | `HighWaterMark`        | `i128`         | NAV-per-share high-water mark for perf fees    |
//! | `DepositCap`           | `i128`         | Maximum total NAV allowed (0 = uncapped)       |
//! | `Oracle`               | `Address`      | Price oracle for portfolio asset NAV           |
//! | `MaxLossBps`           | `u32`          | Maximum NAV loss per execute_op (bps, 0 = off) |
//! | `PortfolioAssets`      | `Vec<Address>` | All assets tracked in NAV                      |
//! | `DepositAssets`        | `Vec<Address>` | Assets users may deposit                       |
//! | `ActiveGuards`         | `Vec<Address>` | Active strategy/guard contracts                |
//! | `TrackedAssets`        | `Vec<Address>` | Portfolio assets with non-zero idle balances   |
//! | `PositionGuards`       | `Vec<Address>` | Active guards with non-zero strategy value     |
//! | `AuthorizedOps(guard)` | `Vec<Symbol>`  | Permitted function names per guard             |
//! | `Factory`              | `Address`      | Factory for whitelist validation               |

use crate::error::VaultError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, String, Symbol, Vec};

// ---------------------------------------------------------------------------
// TTL constants (in ledgers; ~6 s/ledger on Stellar mainnet)
// ---------------------------------------------------------------------------

/// Ledgers added to the instance TTL on every entry-point call.
/// 34 560 ledgers ≈ 2.4 days.
pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;

/// Trigger a bump when the remaining instance TTL drops below this threshold.
/// 17 280 ledgers ≈ 1.2 days.
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;

// ---------------------------------------------------------------------------
// Fee constants
// ---------------------------------------------------------------------------

/// Denominator for all basis-point fee parameters (1 bps = 1 / 10 000).
pub const FEE_DENOMINATOR: u32 = 10_000;

/// Maximum entry or exit fee: 5 % = 500 bps.
pub const MAX_ENTRY_EXIT_FEE_BPS: u32 = 500;

/// Maximum annual management fee: 3 % = 300 bps.
pub const MAX_MGMT_FEE_BPS: u32 = 300;

/// Maximum performance fee: 30 % = 3 000 bps.
pub const MAX_PERF_FEE_BPS: u32 = 3_000;

/// Seconds in one Julian year — used to stream management fees.
pub const SECONDS_PER_YEAR: u64 = 31_536_000;

// ---------------------------------------------------------------------------
// Storage key enum
// ---------------------------------------------------------------------------

/// All keys stored in instance storage by the Vault contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Vault admin — can change manager and treasury; separate from manager role.
    Admin,

    /// Fund manager — sole address allowed to call `execute_op`,
    /// fee-configuration methods, guard/portfolio management, etc.
    Manager,

    /// Trader — address allowed to call `execute_op` alongside the manager.
    Trader,

    /// Denomination token (e.g. USDC) that the vault accepts for deposits and
    /// returns on withdrawals.  All NAV figures are expressed in base-asset units.
    BaseAsset,

    /// SEP-41 share token contract.  The vault is its admin and calls
    /// `mint` / `burn` to manage share supply.
    ShareToken,

    /// Deposit/withdrawal pause flag. When `true`, `deposit` and `withdraw` revert.
    Paused,

    /// Operations pause flag. When `true`, `execute_op` reverts.
    OpsPaused,

    /// Entry fee in basis points charged on deposit.  Fee is taken as share
    /// tokens minted to the manager (dHedge V2 §1 Step 6).
    EntryFeeBps,

    /// Exit fee in basis points charged on withdrawal.  Fee fraction remains
    /// in the vault, benefiting remaining shareholders (dHedge V2 §2 Step 4).
    ExitFeeBps,

    /// Annual management fee in basis points.  Accrued continuously since
    /// `LastMgmtFeeTs` and settled as shares minted to the manager.
    MgmtFeeBps,

    /// Performance fee in basis points.  Charged when NAV-per-share exceeds
    /// the stored `HighWaterMark`.
    PerfFeeBps,

    /// UNIX timestamp (seconds) of the last management-fee collection.
    LastMgmtFeeTs,

    /// NAV-per-share high-water mark used for performance fee eligibility
    /// (PRICE_PRECISION-scaled, same unit as `get_share_price()`).
    HighWaterMark,

    /// Maximum total NAV the vault will accept in deposits.
    /// `0` means uncapped.
    DepositCap,

    /// Optional price oracle contract.  When set, `nav()` calls
    /// `oracle.get_price(price_token)` to convert strategy values to base-asset
    /// terms before summing.
    Oracle,

    /// Maximum allowed NAV loss per manager operation, in basis points.
    ///
    /// After every `invest` / `unwind` / `invest_lp` / `unwind_lp` /
    /// `execute_trade` the vault asserts:
    /// ```text
    /// nav_after ≥ nav_before × (1 − max_loss_bps / 10_000)
    /// ```
    /// Reverts with [`VaultError::TvlGuardTripped`] if violated.
    /// `0` disables the guard entirely.
    MaxLossBps,

    /// Whether the vault is private-pool mode (member-only deposits).
    PrivatePool,

    /// Member-allowlist flag for private-pool deposits.
    Member(Address),

    /// Minimum cooldown (seconds) between a user's latest deposit and withdraw.
    ExitCooldownSecs,

    /// Last deposit timestamp per user (UNIX seconds).
    LastDepositTs(Address),

    /// Announced next entry-fee bps for delayed fee-increase commits.
    AnnouncedEntryFeeBps,
    /// Announced next exit-fee bps for delayed fee-increase commits.
    AnnouncedExitFeeBps,
    /// Announced next management-fee bps for delayed fee-increase commits.
    AnnouncedMgmtFeeBps,
    /// Announced next performance-fee bps for delayed fee-increase commits.
    AnnouncedPerfFeeBps,
    /// UNIX timestamp when announced fee updates become committable.
    AnnouncedFeeActivationTs,

    /// Enable same-ledger operation-type/value manipulation checks.
    ValueManipulationGuardEnabled,

    /// Per-caller operation state for same-ledger manipulation checks.
    OpState(Address),

    // -----------------------------------------------------------------------
    // Multi-asset v2 keys
    // -----------------------------------------------------------------------
    /// Ordered list of all assets tracked in NAV (base_asset + USDT + XLM …).
    /// Every asset in this list is priced via the oracle and included in NAV.
    /// Vault managers may only add assets that appear in factory.AuthorizedAssets.
    PortfolioAssets,

    /// Subset of PortfolioAssets that users may deposit.
    /// Must always be a subset of PortfolioAssets.
    DepositAssets,

    /// Ordered list of active strategy guard contract addresses.
    /// Each guard exposes get_total_value / withdraw_fraction / asset_in_use.
    /// Replaces the old flat Strategies list for multi-asset vaults.
    ActiveGuards,

    /// Ordered list of portfolio assets that currently have a non-zero idle
    /// vault balance. NAV uses this bounded index instead of scanning every
    /// configured portfolio asset on every user operation.
    TrackedAssets,

    /// Ordered list of active guards that currently have a non-zero strategy
    /// position. NAV/withdrawal use this bounded index instead of calling every
    /// active guard, including zero-position guards.
    PositionGuards,

    /// Permitted operation types for a given guard contract.
    /// E.g., a DEX guard may be restricted to [Swap] only, disallowing
    /// AddLiquidity and RemoveLiquidity for this vault.
    AuthorizedOps(Address),

    /// Factory contract address. Used to validate that portfolio assets are
    /// in the factory's global AuthorizedAssets whitelist.
    Factory,

    /// Treasury address — receives all fee payments (entry, mgmt, perf).
    Treasury,

    /// Optional human-readable name of the manager for display purposes.
    ManagerName,

    /// Whether the anti-inflation seed deposit has already been executed.
    /// Set to true by seed_deposit(); prevents a second call.
    SeedDeposited,

    /// Per-user PnL tracking stored in **persistent** storage so it survives
    /// independent of the vault instance TTL.
    UserPosition(Address),
}

/// Per-user same-ledger operation checkpoint.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationState {
    pub ledger: u32,
    pub op_type: u32,
    pub expected_nav_after: i128,
}

// ---------------------------------------------------------------------------
// Internal TTL helper
// ---------------------------------------------------------------------------

/// Extend the instance storage TTL on every call so data is never archived
/// mid-operation.
fn bump(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

// ---------------------------------------------------------------------------
// Initialization guard
// ---------------------------------------------------------------------------

/// Return `true` when the vault has been initialized (i.e. `Manager` key exists).
pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Manager)
}

// ---------------------------------------------------------------------------
// Address helpers — generated by macro
// ---------------------------------------------------------------------------

/// Macro that generates a `set_*` / `get_*` pair for `Address` values in
/// instance storage.  The `get_*` function panics with
/// [`VaultError::NotInitialized`] when the key is absent.
macro_rules! addr_fns {
    ($set:ident, $get:ident, $key:ident) => {
        /// Persist the value in instance storage and extend the TTL.
        #[allow(dead_code)]
        pub fn $set(env: &Env, v: &Address) {
            bump(env);
            env.storage().instance().set(&DataKey::$key, v);
        }
        /// Read the value from instance storage and extend the TTL.
        ///
        /// # Panics
        /// Panics with [`VaultError::NotInitialized`] if the key is absent.
        pub fn $get(env: &Env) -> Address {
            bump(env);
            env.storage()
                .instance()
                .get(&DataKey::$key)
                .unwrap_or_else(|| panic_with_error!(env, VaultError::NotInitialized))
        }
    };
}

addr_fns!(set_admin, get_admin, Admin);
addr_fns!(set_manager, get_manager, Manager);
addr_fns!(set_trader, get_trader, Trader);
addr_fns!(set_base_asset, get_base_asset, BaseAsset);
addr_fns!(set_share_token, get_share_token, ShareToken);
addr_fns!(set_treasury, get_treasury, Treasury);

// ---------------------------------------------------------------------------
// Boolean helpers
// ---------------------------------------------------------------------------

/// Persist the pause flag in instance storage.
pub fn set_paused(env: &Env, v: bool) {
    bump(env);
    env.storage().instance().set(&DataKey::Paused, &v);
}

/// Read the deposit/withdrawal pause flag (defaults to `false` when not yet set).
pub fn get_paused(env: &Env) -> bool {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

/// Persist the operations pause flag in instance storage.
pub fn set_ops_paused(env: &Env, v: bool) {
    bump(env);
    env.storage().instance().set(&DataKey::OpsPaused, &v);
}

/// Read the operations pause flag (defaults to `false` when not yet set).
pub fn get_ops_paused(env: &Env) -> bool {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::OpsPaused)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// u32 helpers — generated by macro
// ---------------------------------------------------------------------------

/// Macro that generates a `set_*` / `get_*` pair for `u32` values in
/// instance storage.  The `get_*` function returns `0` when the key is absent.
macro_rules! u32_fns {
    ($set:ident, $get:ident, $key:ident) => {
        /// Persist the value in instance storage and extend the TTL.
        pub fn $set(env: &Env, v: u32) {
            bump(env);
            env.storage().instance().set(&DataKey::$key, &v);
        }
        /// Read the value from instance storage (defaults to `0` when absent).
        pub fn $get(env: &Env) -> u32 {
            bump(env);
            env.storage().instance().get(&DataKey::$key).unwrap_or(0)
        }
    };
}

u32_fns!(set_entry_fee_bps, get_entry_fee_bps, EntryFeeBps);
u32_fns!(set_exit_fee_bps, get_exit_fee_bps, ExitFeeBps);
u32_fns!(set_mgmt_fee_bps, get_mgmt_fee_bps, MgmtFeeBps);
u32_fns!(set_perf_fee_bps, get_perf_fee_bps, PerfFeeBps);

// ---------------------------------------------------------------------------
// u64 helpers
// ---------------------------------------------------------------------------

/// Persist the last management-fee collection timestamp (UNIX seconds).
pub fn set_last_mgmt_fee_ts(env: &Env, v: u64) {
    bump(env);
    env.storage().instance().set(&DataKey::LastMgmtFeeTs, &v);
}

/// Read the last management-fee collection timestamp (defaults to `0`).
pub fn get_last_mgmt_fee_ts(env: &Env) -> u64 {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::LastMgmtFeeTs)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// i128 helpers
// ---------------------------------------------------------------------------

/// Persist the NAV-per-share high-water mark (PRICE_PRECISION-scaled).
pub fn set_high_water_mark(env: &Env, v: i128) {
    bump(env);
    env.storage().instance().set(&DataKey::HighWaterMark, &v);
}

/// Read the NAV-per-share high-water mark (defaults to `0`).
pub fn get_high_water_mark(env: &Env) -> i128 {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::HighWaterMark)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Deposit cap
// ---------------------------------------------------------------------------

/// Persist the maximum total NAV cap (in base-asset units).
/// Set to `0` to disable the cap.
pub fn set_deposit_cap(env: &Env, v: i128) {
    bump(env);
    env.storage().instance().set(&DataKey::DepositCap, &v);
}

/// Read the deposit cap.  Returns `0` (uncapped) when not configured.
pub fn get_deposit_cap(env: &Env) -> i128 {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::DepositCap)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Oracle
// ---------------------------------------------------------------------------

/// Persist the oracle contract address used for NAV pricing.
pub fn set_oracle(env: &Env, v: &Address) {
    bump(env);
    env.storage().instance().set(&DataKey::Oracle, v);
}

/// Return the oracle contract address, or `None` if not configured.
/// When `None`, `nav()` uses raw strategy values without price conversion.
pub fn get_oracle(env: &Env) -> Option<Address> {
    bump(env);
    env.storage().instance().get(&DataKey::Oracle)
}

// ---------------------------------------------------------------------------
// Manager name
// ---------------------------------------------------------------------------

pub fn set_manager_name(env: &Env, v: &String) {
    bump(env);
    env.storage().instance().set(&DataKey::ManagerName, v);
}

pub fn get_manager_name(env: &Env) -> Option<String> {
    bump(env);
    env.storage().instance().get(&DataKey::ManagerName)
}

// ---------------------------------------------------------------------------
// Max NAV loss per operation (TVL guard)
// ---------------------------------------------------------------------------

/// Persist the maximum allowed NAV loss per manager operation in basis points.
/// `0` disables the TVL guard entirely.
pub fn set_max_loss_bps(env: &Env, v: u32) {
    bump(env);
    env.storage().instance().set(&DataKey::MaxLossBps, &v);
}

/// Read the maximum allowed NAV loss per manager operation.
/// Returns `0` (guard disabled) when not configured.
pub fn get_max_loss_bps(env: &Env) -> u32 {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::MaxLossBps)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Private pool / membership
// ---------------------------------------------------------------------------

pub fn set_private_pool(env: &Env, v: bool) {
    bump(env);
    env.storage().instance().set(&DataKey::PrivatePool, &v);
}

pub fn get_private_pool(env: &Env) -> bool {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::PrivatePool)
        .unwrap_or(false)
}

pub fn set_member(env: &Env, member: &Address, allowed: bool) {
    bump(env);
    if allowed {
        env.storage()
            .instance()
            .set(&DataKey::Member(member.clone()), &true);
    } else {
        env.storage()
            .instance()
            .remove(&DataKey::Member(member.clone()));
    }
}

pub fn is_member(env: &Env, member: &Address) -> bool {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Member(member.clone()))
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Cooldown
// ---------------------------------------------------------------------------

/// Approximate seconds per ledger on Stellar mainnet (~6 s).
const SECS_PER_LEDGER: u64 = 6;

pub fn set_exit_cooldown_secs(env: &Env, v: u64) {
    bump(env);
    env.storage().instance().set(&DataKey::ExitCooldownSecs, &v);
}

pub fn get_exit_cooldown_secs(env: &Env) -> u64 {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::ExitCooldownSecs)
        .unwrap_or(0)
}

/// Persist the per-user last-deposit timestamp in **temporary storage**.
///
/// Temporary storage keys auto-expire after the TTL so they cannot
/// accumulate unbounded per-user entries inside the instance entry, which
/// is size-capped and loaded on every invocation (DoS / bricking risk).
///
/// TTL = cooldown_window_in_ledgers + INSTANCE_LIFETIME_THRESHOLD (safety buffer).
pub fn set_last_deposit_ts(env: &Env, user: &Address, ts: u64) {
    let key = DataKey::LastDepositTs(user.clone());
    env.storage().temporary().set(&key, &ts);

    let cooldown_secs = get_exit_cooldown_secs(env);
    // ceil(cooldown_secs / SECS_PER_LEDGER) converted to u32
    let cooldown_ledgers = cooldown_secs
        .saturating_add(SECS_PER_LEDGER - 1)
        .checked_div(SECS_PER_LEDGER)
        .unwrap_or(0)
        .min(u32::MAX as u64) as u32;
    let ttl = cooldown_ledgers.saturating_add(INSTANCE_LIFETIME_THRESHOLD);
    env.storage().temporary().extend_ttl(&key, ttl, ttl);
}

/// Read the per-user last-deposit timestamp from temporary storage.
/// Returns `None` when no deposit has been made or the key has expired.
pub fn get_last_deposit_ts_opt(env: &Env, user: &Address) -> Option<u64> {
    env.storage()
        .temporary()
        .get(&DataKey::LastDepositTs(user.clone()))
}

// ---------------------------------------------------------------------------
// Announced fee increase state
// ---------------------------------------------------------------------------

pub fn set_announced_entry_fee_bps(env: &Env, v: u32) {
    bump(env);
    env.storage()
        .instance()
        .set(&DataKey::AnnouncedEntryFeeBps, &v);
}

pub fn get_announced_entry_fee_bps(env: &Env) -> Option<u32> {
    bump(env);
    env.storage().instance().get(&DataKey::AnnouncedEntryFeeBps)
}

pub fn set_announced_exit_fee_bps(env: &Env, v: u32) {
    bump(env);
    env.storage()
        .instance()
        .set(&DataKey::AnnouncedExitFeeBps, &v);
}

pub fn get_announced_exit_fee_bps(env: &Env) -> Option<u32> {
    bump(env);
    env.storage().instance().get(&DataKey::AnnouncedExitFeeBps)
}

pub fn set_announced_mgmt_fee_bps(env: &Env, v: u32) {
    bump(env);
    env.storage()
        .instance()
        .set(&DataKey::AnnouncedMgmtFeeBps, &v);
}

pub fn get_announced_mgmt_fee_bps(env: &Env) -> Option<u32> {
    bump(env);
    env.storage().instance().get(&DataKey::AnnouncedMgmtFeeBps)
}

pub fn set_announced_perf_fee_bps(env: &Env, v: u32) {
    bump(env);
    env.storage()
        .instance()
        .set(&DataKey::AnnouncedPerfFeeBps, &v);
}

pub fn get_announced_perf_fee_bps(env: &Env) -> Option<u32> {
    bump(env);
    env.storage().instance().get(&DataKey::AnnouncedPerfFeeBps)
}

pub fn set_announced_fee_activation_ts(env: &Env, ts: u64) {
    bump(env);
    env.storage()
        .instance()
        .set(&DataKey::AnnouncedFeeActivationTs, &ts);
}

pub fn get_announced_fee_activation_ts(env: &Env) -> Option<u64> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::AnnouncedFeeActivationTs)
}

pub fn clear_announced_fees(env: &Env) {
    bump(env);
    env.storage()
        .instance()
        .remove(&DataKey::AnnouncedEntryFeeBps);
    env.storage()
        .instance()
        .remove(&DataKey::AnnouncedExitFeeBps);
    env.storage()
        .instance()
        .remove(&DataKey::AnnouncedMgmtFeeBps);
    env.storage()
        .instance()
        .remove(&DataKey::AnnouncedPerfFeeBps);
    env.storage()
        .instance()
        .remove(&DataKey::AnnouncedFeeActivationTs);
}

// ---------------------------------------------------------------------------
// Value manipulation guard
// ---------------------------------------------------------------------------

pub fn set_value_manipulation_guard_enabled(env: &Env, enabled: bool) {
    bump(env);
    env.storage()
        .instance()
        .set(&DataKey::ValueManipulationGuardEnabled, &enabled);
}

pub fn get_value_manipulation_guard_enabled(env: &Env) -> bool {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::ValueManipulationGuardEnabled)
        .unwrap_or(false)
}

pub fn set_op_state(env: &Env, user: &Address, state: &OperationState) {
    let key = DataKey::OpState(user.clone());
    env.storage().temporary().set(&key, state);
    // OpState only needs to survive within a single ledger. A small TTL
    // prevents unbounded per-user entries from accumulating in instance storage
    // (DoS via entry-size exhaustion). INSTANCE_LIFETIME_THRESHOLD is a
    // conservative safety buffer beyond the ledger close.
    env.storage().temporary().extend_ttl(
        &key,
        INSTANCE_LIFETIME_THRESHOLD,
        INSTANCE_LIFETIME_THRESHOLD,
    );
}

pub fn get_op_state(env: &Env, user: &Address) -> Option<OperationState> {
    env.storage()
        .temporary()
        .get(&DataKey::OpState(user.clone()))
}

pub fn clear_op_state(env: &Env, user: &Address) {
    env.storage()
        .temporary()
        .remove(&DataKey::OpState(user.clone()));
}

// ---------------------------------------------------------------------------
// Persistent storage TTL constants (multi-asset v2)
// ---------------------------------------------------------------------------

/// Ledgers added to persistent user-position entries.
/// 5 256 000 ledgers ≈ 1 year.
pub const PERSISTENT_BUMP_AMOUNT: u32 = 5_256_000;
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 2_628_000; // ≈ 6 months

/// Maximum number of active guards (bounds NAV iteration cost).
pub const MAX_GUARDS: usize = 10;

/// Maximum number of portfolio assets (bounds NAV iteration cost).
pub const MAX_PORTFOLIO_ASSETS: usize = 20;

// ---------------------------------------------------------------------------
// UserPosition struct (PnL tracking)
// ---------------------------------------------------------------------------

/// Per-user PnL tracking record.
///
/// `cost_basis` tracks the base-asset-denominated value paid for currently
/// held shares (updated on deposit and withdrawal).  `realized_pnl` accumulates
/// gain or loss each time shares are burned.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserPosition {
    /// Total cost in base_asset units for the user's current share holdings.
    pub cost_basis: i128,
    /// Accumulated realized gain/loss from all past withdrawals.
    pub realized_pnl: i128,
}

// ---------------------------------------------------------------------------
// Portfolio assets (instance storage)
// ---------------------------------------------------------------------------

pub fn get_portfolio_assets(env: &Env) -> Vec<Address> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::PortfolioAssets)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_portfolio_assets(env: &Env, v: &Vec<Address>) {
    bump(env);
    env.storage().instance().set(&DataKey::PortfolioAssets, v);
}

// ---------------------------------------------------------------------------
// Deposit assets (instance storage)
// ---------------------------------------------------------------------------

pub fn get_deposit_assets(env: &Env) -> Vec<Address> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::DepositAssets)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_deposit_assets(env: &Env, v: &Vec<Address>) {
    bump(env);
    env.storage().instance().set(&DataKey::DepositAssets, v);
}

// ---------------------------------------------------------------------------
// Active guards (instance storage)
// ---------------------------------------------------------------------------

pub fn get_active_guards(env: &Env) -> Vec<Address> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::ActiveGuards)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_active_guards(env: &Env, v: &Vec<Address>) {
    bump(env);
    env.storage().instance().set(&DataKey::ActiveGuards, v);
}

// ---------------------------------------------------------------------------
// Bounded NAV indexes (instance storage)
// ---------------------------------------------------------------------------

pub fn get_tracked_assets(env: &Env) -> Vec<Address> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::TrackedAssets)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_tracked_assets(env: &Env, v: &Vec<Address>) {
    bump(env);
    env.storage().instance().set(&DataKey::TrackedAssets, v);
}

pub fn get_position_guards(env: &Env) -> Vec<Address> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::PositionGuards)
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_position_guards(env: &Env, v: &Vec<Address>) {
    bump(env);
    env.storage().instance().set(&DataKey::PositionGuards, v);
}

// ---------------------------------------------------------------------------
// Authorized ops per guard (instance storage)
// ---------------------------------------------------------------------------

pub fn get_authorized_ops(env: &Env, guard: &Address) -> Vec<Symbol> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::AuthorizedOps(guard.clone()))
        .unwrap_or_else(|| Vec::new(env))
}

pub fn set_authorized_ops(env: &Env, guard: &Address, ops: &Vec<Symbol>) {
    bump(env);
    env.storage()
        .instance()
        .set(&DataKey::AuthorizedOps(guard.clone()), ops);
}

// ---------------------------------------------------------------------------
// Factory reference (instance storage)
// ---------------------------------------------------------------------------

pub fn get_factory(env: &Env) -> Option<Address> {
    bump(env);
    env.storage().instance().get(&DataKey::Factory)
}

#[cfg(test)]
pub fn set_factory(env: &Env, factory: &Address) {
    bump(env);
    env.storage().instance().set(&DataKey::Factory, factory);
}

// ---------------------------------------------------------------------------
// Seed deposit guard (instance storage)
// ---------------------------------------------------------------------------

pub fn is_seed_deposited(env: &Env) -> bool {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::SeedDeposited)
        .unwrap_or(false)
}

pub fn set_seed_deposited(env: &Env) {
    bump(env);
    env.storage().instance().set(&DataKey::SeedDeposited, &true);
}

// ---------------------------------------------------------------------------
// UserPosition PnL tracking (persistent storage)
// ---------------------------------------------------------------------------

pub fn get_user_position(env: &Env, user: &Address) -> UserPosition {
    let key = DataKey::UserPosition(user.clone());
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        env.storage().persistent().get(&key).unwrap()
    } else {
        UserPosition {
            cost_basis: 0,
            realized_pnl: 0,
        }
    }
}

pub fn set_user_position(env: &Env, user: &Address, pos: &UserPosition) {
    let key = DataKey::UserPosition(user.clone());
    env.storage().persistent().set(&key, pos);
    env.storage().persistent().extend_ttl(
        &key,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}
