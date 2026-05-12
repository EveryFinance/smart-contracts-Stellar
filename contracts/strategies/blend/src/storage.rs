use soroban_sdk::{contracttype, panic_with_error, Address, Env, String};

use crate::error::BlendStrategyError;

// ---------------------------------------------------------------------------
// TTL constants  (ledgers; ~6 s per ledger on Stellar mainnet)
// ---------------------------------------------------------------------------

/// Ledgers added to the instance entry TTL on every entry-point call.
/// 518 400 ledgers ≈ 30 days.
pub const INSTANCE_BUMP_AMOUNT: u32 = 518_400;

/// Trigger a bump when the remaining instance TTL drops below this value.
/// 259 200 ledgers ≈ 15 days.
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 259_200;

/// Bump amount for the persistent initialized flag.
/// u32::MAX ≈ 248 000 years — effectively permanent.
pub const PERSISTENT_BUMP_AMOUNT: u32 = u32::MAX;

/// Trigger a persistent bump when TTL drops below this threshold.
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = u32::MAX / 2;

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
    /// Human-readable name for this strategy instance.
    Name,
    /// Persistent initialization flag.
    ///
    /// Stored in **persistent** storage (not instance) so it survives instance
    /// TTL expiry.  If only the instance storage `Vault` key were used as the
    /// guard, an attacker could wait for the instance to expire and re-call
    /// `initialize` with a malicious vault, effectively taking over the strategy.
    Initialized,
    /// Factory address — queried for AssetHandler during get_total_value.
    /// Stored locally (fetched from vault during initialize) to avoid re-entry
    /// when vault calls get_total_value from within nav().
    Factory,
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

/// Persist the initialization flag in **persistent** storage.
///
/// Persistent storage survives instance-entry TTL expiry, which prevents an
/// attacker from re-initializing the strategy after the instance expires.
pub fn set_initialized(env: &Env) {
    env.storage().persistent().set(&DataKey::Initialized, &true);
    env.storage().persistent().extend_ttl(
        &DataKey::Initialized,
        PERSISTENT_LIFETIME_THRESHOLD,
        PERSISTENT_BUMP_AMOUNT,
    );
}

/// Return `true` when the contract has been initialized.
///
/// Checks the **persistent** `Initialized` flag rather than the instance-storage
/// `Vault` key so that expiry of the instance entry cannot be exploited to
/// re-run `initialize`.
pub fn is_initialized(env: &Env) -> bool {
    env.storage().persistent().has(&DataKey::Initialized)
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

// ---- factory ---------------------------------------------------------------

/// Persist the factory address (optional, fetched from vault during initialize).
pub fn set_factory(env: &Env, factory: &Address) {
    bump(env);
    env.storage().instance().set(&DataKey::Factory, factory);
}

/// Read the factory address (None if vault has no factory).
pub fn get_factory(env: &Env) -> Option<Address> {
    bump(env);
    env.storage().instance().get(&DataKey::Factory)
}
