//! SoroswapTradeGuard storage layout.
//!
//! All guard state lives in **instance storage** so its TTL is tied to the
//! contract instance.
//!
//! | Key         | Type           | Description                              |
//! |-------------|----------------|------------------------------------------|
//! | `Vault`     | `Address`      | The only address allowed to call validate |
//! | `Manager`   | `Address`      | Account allowed to update the whitelist   |
//! | `Whitelist` | `Vec<Address>` | Set of approved tradeable token addresses |

use crate::error::SoroswapGuardError;
use soroban_sdk::{contracttype, panic_with_error, Address, Env, Vec};

// ---------------------------------------------------------------------------
// TTL constants (ledgers; ~6 s/ledger on Stellar mainnet)
// ---------------------------------------------------------------------------

/// Ledgers added to the instance TTL on every entry-point call.
/// 34 560 ledgers ≈ 2.4 days.
pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;

/// Trigger a bump when the remaining instance TTL drops below this threshold.
/// 17 280 ledgers ≈ 1.2 days.
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;

/// Maximum allowed Soroswap swap path length (inclusive).
/// Paths longer than this are rejected with
/// [`SoroswapGuardError::PathTooLong`].
pub const MAX_PATH_LEN: u32 = 5;

/// Maximum allowed slippage expressed in basis points.
///
/// `(amount_in - min_out) / amount_in` must not exceed this value.
/// 1 000 bps = 10 %.
pub const MAX_SLIPPAGE_BPS: u32 = 1_000;

// ---------------------------------------------------------------------------
// Storage key enum
// ---------------------------------------------------------------------------

/// All keys stored in instance storage by the SoroswapTradeGuard.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// The vault contract address — the only caller allowed to invoke
    /// `validate_swap_exact_in` and `validate_swap_exact_out`.
    Vault,

    /// The manager address — the only caller allowed to update the token
    /// whitelist via `set_whitelist`.
    Manager,

    /// Ordered list of token addresses approved for trading.
    /// Every token in a swap path must appear in this list.
    Whitelist,
}

// ---------------------------------------------------------------------------
// Internal TTL helper
// ---------------------------------------------------------------------------

fn bump(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

// ---------------------------------------------------------------------------
// Initialization guard
// ---------------------------------------------------------------------------

/// Return `true` when the guard has been initialized.
pub fn is_initialized(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Vault)
}

// ---------------------------------------------------------------------------
// Storage accessors
// ---------------------------------------------------------------------------

/// Persist the vault address.
pub fn set_vault(env: &Env, v: &Address) {
    bump(env);
    env.storage().instance().set(&DataKey::Vault, v);
}

/// Read the vault address.
///
/// # Panics
/// Panics with [`SoroswapGuardError::NotInitialized`] if absent.
pub fn get_vault(env: &Env) -> Address {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Vault)
        .unwrap_or_else(|| panic_with_error!(env, SoroswapGuardError::NotInitialized))
}

/// Persist the manager address.
pub fn set_manager(env: &Env, v: &Address) {
    bump(env);
    env.storage().instance().set(&DataKey::Manager, v);
}

/// Read the manager address.
///
/// # Panics
/// Panics with [`SoroswapGuardError::NotInitialized`] if absent.
pub fn get_manager(env: &Env) -> Address {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Manager)
        .unwrap_or_else(|| panic_with_error!(env, SoroswapGuardError::NotInitialized))
}

/// Persist the token whitelist.
pub fn set_whitelist(env: &Env, tokens: &Vec<Address>) {
    bump(env);
    env.storage().instance().set(&DataKey::Whitelist, tokens);
}

/// Read the token whitelist.
///
/// Returns an empty `Vec` when no whitelist has been configured.
pub fn get_whitelist(env: &Env) -> Vec<Address> {
    bump(env);
    env.storage()
        .instance()
        .get(&DataKey::Whitelist)
        .unwrap_or_else(|| Vec::new(env))
}
