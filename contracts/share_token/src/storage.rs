use soroban_sdk::{contracttype, panic_with_error, Address, Env, String};

use crate::error::ShareTokenError;

// ---------------------------------------------------------------------------
// TTL constants (in ledgers; ~6 s/ledger on Stellar mainnet)
// ---------------------------------------------------------------------------

/// Number of ledgers added to the instance TTL on every bump.
/// 34 560 ledgers ≈ 2.4 days.
pub const INSTANCE_BUMP_AMOUNT: u32 = 34_560;

/// Trigger a bump when remaining instance TTL falls below this value.
/// 17 280 ledgers ≈ 1.2 days.
pub const INSTANCE_LIFETIME_THRESHOLD: u32 = 17_280;

/// Number of ledgers added to persistent-storage entries on every bump.
/// 518 400 ledgers ≈ 360 days.
pub const PERSISTENT_BUMP_AMOUNT: u32 = 518_400;

/// Trigger a persistent bump when remaining TTL falls below this value.
/// 259 200 ledgers ≈ 180 days.
pub const PERSISTENT_LIFETIME_THRESHOLD: u32 = 259_200;

// ---------------------------------------------------------------------------
// Composite key for allowances
// ---------------------------------------------------------------------------

/// The map key used for per-account allowance entries.
/// Both fields are included so that (from, spender) pairs are unique.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceKey {
    /// The token-holder who granted the allowance.
    pub from: Address,
    /// The account permitted to spend on behalf of `from`.
    pub spender: Address,
}

/// Stored allowance value including amount and expiry ledger.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AllowanceValue {
    /// Remaining spendable amount.
    pub amount: i128,
    /// Last ledger sequence where this allowance is valid (inclusive).
    pub expiration_ledger: u32,
}

// ---------------------------------------------------------------------------
// Top-level storage key enum
// ---------------------------------------------------------------------------

/// All storage keys used by the ShareToken contract.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    /// Address of the vault (admin) — stored in INSTANCE storage.
    Admin,
    /// Human-readable token name — stored in INSTANCE storage.
    Name,
    /// Short token ticker symbol — stored in INSTANCE storage.
    Symbol,
    /// Number of decimal places — stored in INSTANCE storage.
    Decimals,
    /// Running total of minted tokens — stored in INSTANCE storage.
    TotalSupply,
    /// Whether user share transfers are enabled. Defaults to false.
    TransfersEnabled,
    /// Per-account token balance — stored in PERSISTENT storage.
    Balance(Address),
    /// Per-(from, spender) allowance — stored in PERSISTENT storage.
    Allowance(AllowanceKey),
}

// ---------------------------------------------------------------------------
// Instance-storage helpers (Admin, Name, Symbol, Decimals, TotalSupply)
// ---------------------------------------------------------------------------

/// Store the admin address in instance storage.
pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

/// Read the admin address from instance storage.
///
/// # Panics
/// Panics with [`ShareTokenError::NotInitialized`] if no admin has been stored.
pub fn get_admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, ShareTokenError::NotInitialized))
}

/// Returns `true` when the contract has been initialized.
pub fn has_admin(env: &Env) -> bool {
    env.storage().instance().has(&DataKey::Admin)
}

/// Store the token name in instance storage.
pub fn set_name(env: &Env, name: &String) {
    env.storage().instance().set(&DataKey::Name, name);
}

/// Read the token name from instance storage.
pub fn get_name(env: &Env) -> String {
    env.storage()
        .instance()
        .get(&DataKey::Name)
        .unwrap_or_else(|| panic_with_error!(env, ShareTokenError::NotInitialized))
}

/// Store the token symbol in instance storage.
pub fn set_symbol(env: &Env, symbol: &String) {
    env.storage().instance().set(&DataKey::Symbol, symbol);
}

/// Read the token symbol from instance storage.
pub fn get_symbol(env: &Env) -> String {
    env.storage()
        .instance()
        .get(&DataKey::Symbol)
        .unwrap_or_else(|| panic_with_error!(env, ShareTokenError::NotInitialized))
}

/// Store the decimals value in instance storage.
pub fn set_decimals(env: &Env, decimals: u32) {
    env.storage().instance().set(&DataKey::Decimals, &decimals);
}

/// Read the decimals value from instance storage.
pub fn get_decimals(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::Decimals)
        .unwrap_or_else(|| panic_with_error!(env, ShareTokenError::NotInitialized))
}

/// Store the total supply in instance storage.
pub fn set_total_supply(env: &Env, supply: i128) {
    env.storage().instance().set(&DataKey::TotalSupply, &supply);
}

/// Read the total supply from instance storage, defaulting to 0 if not set.
pub fn get_total_supply(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::TotalSupply)
        .unwrap_or(0_i128)
}

/// Store whether user share transfers are enabled.
pub fn set_transfers_enabled(env: &Env, enabled: bool) {
    env.storage()
        .instance()
        .set(&DataKey::TransfersEnabled, &enabled);
}

/// Return whether user share transfers are enabled.
pub fn get_transfers_enabled(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&DataKey::TransfersEnabled)
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Persistent-storage helpers (Balance, Allowance)
// ---------------------------------------------------------------------------

/// Write an account balance to persistent storage and extend its TTL.
/// When `balance` is zero the entry is removed so dead accounts do not
/// accumulate indefinitely in persistent state.
pub fn set_balance(env: &Env, addr: &Address, balance: i128) {
    let key = DataKey::Balance(addr.clone());
    if balance == 0 {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &balance);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }
}

/// Read an account balance from persistent storage.
///
/// If the key exists and is non-zero its TTL is extended before returning.
/// A stored zero (left over from before the remove-on-zero policy) is cleaned
/// up eagerly. A missing key returns `0`.
pub fn get_balance(env: &Env, addr: &Address) -> i128 {
    let key = DataKey::Balance(addr.clone());
    if let Some(balance) = env.storage().persistent().get::<DataKey, i128>(&key) {
        if balance == 0 {
            env.storage().persistent().remove(&key);
            0_i128
        } else {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            balance
        }
    } else {
        0_i128
    }
}

/// Write an allowance entry to persistent storage and extend its TTL.
/// When `amount` is zero the entry is removed so revoked allowances do not
/// accumulate indefinitely in persistent state.
pub fn set_allowance(
    env: &Env,
    from: &Address,
    spender: &Address,
    amount: i128,
    expiration_ledger: u32,
) {
    let key = DataKey::Allowance(AllowanceKey {
        from: from.clone(),
        spender: spender.clone(),
    });
    if amount == 0 {
        env.storage().persistent().remove(&key);
    } else {
        let value = AllowanceValue {
            amount,
            expiration_ledger,
        };
        env.storage().persistent().set(&key, &value);
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_LIFETIME_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }
}

/// Read an allowance entry from persistent storage.
///
/// Expired entries are removed rather than TTL-bumped so they do not
/// linger indefinitely. Returns `0` for absent or expired allowances.
pub fn get_allowance(env: &Env, from: &Address, spender: &Address) -> i128 {
    let key = DataKey::Allowance(AllowanceKey {
        from: from.clone(),
        spender: spender.clone(),
    });
    if let Some(value) = env
        .storage()
        .persistent()
        .get::<DataKey, AllowanceValue>(&key)
    {
        if env.ledger().sequence() > value.expiration_ledger {
            env.storage().persistent().remove(&key);
            0_i128
        } else {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            value.amount
        }
    } else {
        0_i128
    }
}

/// Read the raw allowance value, including expiry metadata.
///
/// Expired entries are removed and `None` is returned. Returns `None` when
/// no allowance has been set.
pub fn get_allowance_value(env: &Env, from: &Address, spender: &Address) -> Option<AllowanceValue> {
    let key = DataKey::Allowance(AllowanceKey {
        from: from.clone(),
        spender: spender.clone(),
    });
    if let Some(value) = env
        .storage()
        .persistent()
        .get::<DataKey, AllowanceValue>(&key)
    {
        if env.ledger().sequence() > value.expiration_ledger {
            env.storage().persistent().remove(&key);
            None
        } else {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            Some(value)
        }
    } else {
        None
    }
}
