//! Vault event publishers.
//!
//! Every on-chain action that changes vault state emits a structured event so
//! that off-chain indexers, dashboards, and wallets can reconstruct the vault's
//! history without replaying transactions.
//!
//! ## Topic layout
//! All events use a **two-element topic** `(event_symbol, vault_address)` so
//! that subscribers can filter by event type, by vault instance, or by both.
//!
//! ## Data layout
//! The event data field carries the action-specific payload described for each
//! function below.  All amounts are in the token's native precision (7 decimal
//! places for Stellar assets).

use soroban_sdk::{Address, Env, Symbol};

/// Emitted after a successful [`deposit`](crate::Vault::deposit).
///
/// # Data
/// `(from: Address, base_amount: i128, shares_minted: i128)`
/// * `from`          — The depositor's address.
/// * `base_amount`   — Base-asset units transferred into the vault.
/// * `shares_minted` — Share tokens minted to the depositor (net of entry fee).
pub fn deposit_event(env: &Env, from: &Address, base_amount: i128, shares_minted: i128) {
    env.events().publish(
        (Symbol::new(env, "deposit"), env.current_contract_address()),
        (from.clone(), base_amount, shares_minted),
    );
}

/// Emitted after a successful [`withdraw`](crate::Vault::withdraw).
///
/// # Data
/// `(to: Address, shares_burned: i128, base_amount: i128)`
/// * `to`            — The recipient of the base asset.
/// * `shares_burned` — Share tokens destroyed.
/// * `base_amount`   — Net base-asset units delivered to `to` (after exit fee).
pub fn withdraw_event(env: &Env, to: &Address, shares_burned: i128, base_amount: i128) {
    env.events().publish(
        (Symbol::new(env, "withdraw"), env.current_contract_address()),
        (to.clone(), shares_burned, base_amount),
    );
}

/// Emitted after a successful [`invest`](crate::Vault::invest) or
/// [`invest_lp`](crate::Vault::invest_lp).
///
/// # Data
/// `(strategy: Address, amount: i128)`
/// * `strategy` — The strategy contract that received the allocation.
/// * `amount`   — Base-asset units (or paired amounts) sent to the strategy.
pub fn invest_event(env: &Env, strategy: &Address, amount: i128) {
    env.events().publish(
        (Symbol::new(env, "invest"), env.current_contract_address()),
        (strategy.clone(), amount),
    );
}

/// Emitted after a successful [`unwind`](crate::Vault::unwind) or
/// [`unwind_lp`](crate::Vault::unwind_lp).
///
/// # Data
/// `(strategy: Address, shares: i128)`
/// * `strategy` — The strategy contract that was unwound.
/// * `shares`   — Strategy position units redeemed.
pub fn unwind_event(env: &Env, strategy: &Address, shares: i128) {
    env.events().publish(
        (Symbol::new(env, "unwind"), env.current_contract_address()),
        (strategy.clone(), shares),
    );
}

/// Emitted after a successful [`execute_trade`](crate::Vault::execute_trade).
///
/// # Data
/// `(strategy: Address, amount_in: i128, min_out: i128)`
/// * `strategy`  — The strategy / router contract used.
/// * `amount_in` — Input token amount swapped.
/// * `min_out`   — Minimum output amount specified (slippage bound).
pub fn trade_event(env: &Env, strategy: &Address, amount_in: i128, min_out: i128) {
    env.events().publish(
        (Symbol::new(env, "trade"), env.current_contract_address()),
        (strategy.clone(), amount_in, min_out),
    );
}

/// Emitted when management fees are collected.
///
/// # Data
/// `(shares_minted: i128, timestamp: u64)`
/// * `shares_minted` — Share tokens minted to the manager.
/// * `timestamp`     — Unix timestamp of the collection.
pub fn mgmt_fee_event(env: &Env, shares_minted: i128, timestamp: u64) {
    env.events().publish(
        (Symbol::new(env, "mgmt_fee"), env.current_contract_address()),
        (shares_minted, timestamp),
    );
}

/// Emitted when performance fees are collected (high-water mark exceeded).
///
/// # Data
/// `(shares_minted: i128, nav_per_share: i128)`
/// * `shares_minted`  — Share tokens minted to the manager.
/// * `nav_per_share`  — NAV-per-share at the time of collection (PRICE_PRECISION-scaled).
pub fn perf_fee_event(env: &Env, shares_minted: i128, nav_per_share: i128) {
    env.events().publish(
        (Symbol::new(env, "perf_fee"), env.current_contract_address()),
        (shares_minted, nav_per_share),
    );
}

/// Emitted when the vault is paused or unpaused.
///
/// # Data
/// `paused: bool` — `true` if paused, `false` if unpaused.
pub fn pause_event(env: &Env, paused: bool) {
    env.events().publish(
        (Symbol::new(env, "pause"), env.current_contract_address()),
        paused,
    );
}

/// Emitted when the strategy whitelist is updated via
/// [`set_strategies`](crate::Vault::set_strategies).
///
/// # Data
/// `strategies: Vec<Address>` — The new complete list of whitelisted strategies.
pub fn strategy_set_event(env: &Env, strategies: &soroban_sdk::Vec<Address>) {
    env.events().publish(
        (
            Symbol::new(env, "strategies"),
            env.current_contract_address(),
        ),
        strategies.clone(),
    );
}

/// Emitted when the manager role is transferred.
///
/// # Data
/// `new_manager: Address` — The address that is now the manager.
pub fn manager_changed_event(env: &Env, new_manager: &Address) {
    env.events().publish(
        (
            Symbol::new(env, "manager_changed"),
            env.current_contract_address(),
        ),
        new_manager.clone(),
    );
}

/// Emitted when the trader role is transferred.
///
/// # Data
/// `new_trader: Address` — The address that is now the trader.
pub fn trader_changed_event(env: &Env, new_trader: &Address) {
    env.events().publish(
        (
            Symbol::new(env, "trader_changed"),
            env.current_contract_address(),
        ),
        new_trader.clone(),
    );
}
