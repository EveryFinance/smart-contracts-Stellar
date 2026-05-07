use soroban_sdk::{contracttype, panic_with_error, Address, Env, String};

use crate::error::BlendStrategyError;

// ---------------------------------------------------------------------------
// TTL constants  (ledgers; ~6 s per ledger on Stellar mainnet)
// ---------------------------------------------------------------------------

/// Ledgers added to the instance entry TTL on every entry-point call.
/// 34 560 ledgers ≈ 2.4 days.
pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;

/// Trigger a bump when the remaining instance TTL drops below this value.
/// 17 280 ledgers ≈ 1.2 days.
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;

// ---------------------------------------------------------------------------
// Storage key enum
// ---------------------------------------------------------------------------

/// All keys stored in INSTANCE storage by the Blend strategy.
///
/// Everything is in instance storage because the strategy's liveness is
/// coupled to the vault's liveness; long-term persistent storage is not
/// required on the strategy side (Blend holds the actual position state).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Address of the vault that owns this strategy deployment.
    Vault,
    /// Address of the single asset (e.g. USDC) managed by this strategy.
    Asset,
    /// Address of the Blend lending-pool contract.
    Protocol,
    /// Address of the manager account allowed to pause/unpause.
    Manager,
    /// Human-readable name for this strategy instance.
    Name,
    /// Whether the strategy is currently paused.
    Paused,
}

// ---------------------------------------------------------------------------
// Helpers — every helper extends the instance TTL so callers do not need to.
// ---------------------------------------------------------------------------

fn bump(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

// ---- initialization guard --------------------------------------------------

/// Return `true` when the contract has been initialized.
pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Vault)
}

// ---- vault -----------------------------------------------------------------

/// Persist the vault address.
pub fn set_vault(env: &Env, vault: &Address) {
    bump(env);
    env.storage().instance().set(&DataKey::Vault, vault);
}

/// Read the vault address.
///
/// # Panics
/// Panics with [`BlendStrategyError::NotInitialized`] when the contract has
/// not yet been initialized.
pub fn get_vault(env: &Env) -> Address {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Vault)
        .unwrap_or_else(|| panic_with_error!(env, BlendStrategyError::NotInitialized))
}

// ---- asset -----------------------------------------------------------------

/// Persist the managed asset address.
pub fn set_asset(env: &Env, asset: &Address) {
    bump(env);
    env.storage().instance().set(&DataKey::Asset, asset);
}

/// Read the managed asset address.
pub fn get_asset(env: &Env) -> Address {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Asset)
        .unwrap_or_else(|| panic_with_error!(env, BlendStrategyError::NotInitialized))
}

// ---- protocol (Blend pool) -------------------------------------------------

/// Persist the Blend pool address.
pub fn set_protocol(env: &Env, protocol: &Address) {
    bump(env);
    env.storage().instance().set(&DataKey::Protocol, protocol);
}

/// Read the Blend pool address.
pub fn get_protocol(env: &Env) -> Address {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Protocol)
        .unwrap_or_else(|| panic_with_error!(env, BlendStrategyError::NotInitialized))
}

// ---- manager ---------------------------------------------------------------

/// Persist the manager address.
pub fn set_manager(env: &Env, manager: &Address) {
    bump(env);
    env.storage().instance().set(&DataKey::Manager, manager);
}

/// Read the manager address.
pub fn get_manager(env: &Env) -> Address {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Manager)
        .unwrap_or_else(|| panic_with_error!(env, BlendStrategyError::NotInitialized))
}

// ---- name ------------------------------------------------------------------

/// Persist the strategy name.
pub fn set_name(env: &Env, name: &String) {
    bump(env);
    env.storage().instance().set(&DataKey::Name, name);
}

/// Read the strategy name.
pub fn get_name(env: &Env) -> String {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Name)
        .unwrap_or_else(|| panic_with_error!(env, BlendStrategyError::NotInitialized))
}

// ---- paused flag -----------------------------------------------------------

/// Write the paused flag.
pub fn set_paused(env: &Env, paused: bool) {
    bump(env);
    env.storage().instance().set(&DataKey::Paused, &paused);
}

/// Read the paused flag (defaults to `false` if not yet set).
pub fn get_paused(env: &Env) -> bool {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
}

