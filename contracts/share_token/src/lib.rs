//! # ShareToken — SEP-41 Fungible Token for Vault Shares
//!
//! This contract implements a fully SEP-41 compliant fungible token that
//! represents fractional ownership of an on-chain asset-management vault.
//!
//! ## Design decisions
//! * **Admin = Vault contract**: Only the vault may mint or burn shares.
//!   All other SEP-41 operations (transfer, approve, burn-by-holder) are
//!   permissionless once the caller supplies valid auth.
//! * **Persistent storage** is used for per-account balances and allowances
//!   so they survive archival windows; TTLs are bumped on every read and write.
//! * **Instance storage** is used for immutable/rarely-changed metadata
//!   (name, symbol, decimals, admin) and total supply.
//! * All arithmetic uses `checked_*` variants; overflow → `ShareTokenError::Overflow`.

#![no_std]

mod error;
mod events;
mod storage;

pub use error::ShareTokenError;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, String};

use storage::{
    get_admin, get_allowance, get_allowance_value, get_balance, get_decimals, get_name, get_symbol,
    get_total_supply, has_admin, set_admin, set_allowance, set_balance, set_decimals, set_name,
    set_symbol, set_total_supply, INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD,
};

// ---------------------------------------------------------------------------
// Contract struct
// ---------------------------------------------------------------------------

/// The ShareToken contract struct.  All state lives in the Soroban storage
/// maps; this struct carries no fields itself.
#[contract]
pub struct ShareTokenContract;

// ---------------------------------------------------------------------------
// Helper – amount validation
// ---------------------------------------------------------------------------

/// Validate that `amount` is strictly positive.
///
/// # Panics
/// * [`ShareTokenError::NegativeAmount`] if `amount < 0`
/// * [`ShareTokenError::ZeroAmount`]     if `amount == 0`
fn require_positive(env: &Env, amount: i128) {
    if amount < 0 {
        panic_with_error!(env, ShareTokenError::NegativeAmount);
    }
    if amount == 0 {
        panic_with_error!(env, ShareTokenError::ZeroAmount);
    }
}

// ---------------------------------------------------------------------------
// Contract implementation
// ---------------------------------------------------------------------------

#[contractimpl]
impl ShareTokenContract {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the ShareToken contract.
    ///
    /// Runs atomically at `CreateContract` time, preventing front-running
    /// attacks where an attacker could otherwise call `initialize` first and
    /// claim the admin role.
    ///
    /// # Arguments
    /// * `admin`    – The vault contract address that will be allowed to mint
    ///               and burn tokens (typically the deploying manager, later
    ///               transferred to the vault via `set_admin`).
    /// * `name`     – Human-readable token name (e.g. `"Vault Share Token"`).
    /// * `symbol`   – Short ticker symbol (e.g. `"VST"`).
    /// * `decimals` – Number of decimal places (typically `7` to match Stellar
    ///               native asset precision).
    pub fn __constructor(env: Env, admin: Address, name: String, symbol: String, decimals: u32) {
        if has_admin(&env) {
            panic_with_error!(&env, ShareTokenError::AlreadyInitialized);
        }
        admin.require_auth();

        set_admin(&env, &admin);
        set_name(&env, &name);
        set_symbol(&env, &symbol);
        set_decimals(&env, decimals);
        set_total_supply(&env, 0_i128);

        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    // -----------------------------------------------------------------------
    // SEP-41 read-only metadata
    // -----------------------------------------------------------------------

    /// Return the token name.
    ///
    /// # Errors
    /// * [`ShareTokenError::NotInitialized`] if the contract has not been initialized.
    pub fn name(env: Env) -> String {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_name(&env)
    }

    /// Return the token symbol.
    ///
    /// # Errors
    /// * [`ShareTokenError::NotInitialized`] if the contract has not been initialized.
    pub fn symbol(env: Env) -> String {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_symbol(&env)
    }

    /// Return the number of decimal places.
    ///
    /// # Errors
    /// * [`ShareTokenError::NotInitialized`] if the contract has not been initialized.
    pub fn decimals(env: Env) -> u32 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_decimals(&env)
    }

    /// Return the current total supply of outstanding tokens.
    ///
    /// # Errors
    /// * [`ShareTokenError::NotInitialized`] if the contract has not been initialized.
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        if !has_admin(&env) {
            panic_with_error!(&env, ShareTokenError::NotInitialized);
        }
        get_total_supply(&env)
    }

    /// Return the current admin address.
    ///
    /// # Errors
    /// * [`ShareTokenError::NotInitialized`] if the contract has not been initialized.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_admin(&env)
    }

    // -----------------------------------------------------------------------
    // SEP-41 balance & allowance queries
    // -----------------------------------------------------------------------

    /// Return the token balance of `id`.
    ///
    /// Returns `0` for addresses that have never received tokens.
    ///
    /// # Arguments
    /// * `id` – The address to query.
    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_balance(&env, &id)
    }

    /// Return the current spending allowance granted by `from` to `spender`.
    ///
    /// Returns `0` if no allowance has been granted or if it has been revoked.
    ///
    /// # Arguments
    /// * `from`    – The token holder.
    /// * `spender` – The approved spender.
    pub fn allowance(env: Env, from: Address, spender: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_allowance(&env, &from, &spender)
    }

    // -----------------------------------------------------------------------
    // SEP-41 state-modifying operations
    // -----------------------------------------------------------------------

    /// Approve `spender` to transfer up to `amount` tokens on behalf of the
    /// caller (`from`).
    ///
    /// Setting `amount` to `0` effectively revokes the allowance.
    ///
    /// **Zero-first rule:** To prevent the well-known allowance race condition,
    /// changing a live non-zero allowance directly to another non-zero value is
    /// rejected.  Callers must first set the allowance to `0`, then set the new
    /// value — or use [`increase_allowance`] / [`decrease_allowance`] for atomic
    /// adjustments.
    ///
    /// # Arguments
    /// * `from`              – Token holder (must sign this transaction).
    /// * `spender`           – Address being authorized.
    /// * `amount`            – Maximum tokens the spender may move. Must be ≥ 0.
    /// * `expiration_ledger` – Last ledger where the allowance is valid
    ///                         (inclusive). After this ledger, allowance reads
    ///                         as zero and delegated spending reverts.
    ///
    /// # Auth
    /// `from` must authorize this call.
    ///
    /// # Errors
    /// * [`ShareTokenError::NegativeAmount`]  if `amount < 0`.
    /// * [`ShareTokenError::NonZeroAllowance`] if a live non-zero allowance
    ///   already exists and `amount` is also non-zero.
    pub fn approve(
        env: Env,
        from: Address,
        spender: Address,
        amount: i128,
        expiration_ledger: u32,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        if amount < 0 {
            panic_with_error!(&env, ShareTokenError::NegativeAmount);
        }

        from.require_auth();

        // Reject non-zero → non-zero transitions to prevent the approval
        // race condition: a spender cannot front-run a reduction and spend
        // both the old and new allowance.
        if amount > 0 {
            let current = get_allowance(&env, &from, &spender);
            if current > 0 {
                panic_with_error!(&env, ShareTokenError::NonZeroAllowance);
            }
        }

        set_allowance(&env, &from, &spender, amount, expiration_ledger);
        events::approve_event(&env, from, spender, amount, expiration_ledger);
    }

    /// Atomically increase the allowance granted to `spender` by `delta`.
    ///
    /// Safe alternative to `approve` for raising an existing allowance without
    /// the approval race condition.
    ///
    /// # Arguments
    /// * `from`              – Token holder (must sign).
    /// * `spender`           – Approved spender.
    /// * `delta`             – Amount to add. Must be > 0.
    /// * `expiration_ledger` – New expiry applied to the updated allowance.
    ///
    /// # Errors
    /// * [`ShareTokenError::ZeroAmount`]  / [`ShareTokenError::NegativeAmount`]
    /// * [`ShareTokenError::Overflow`] if the resulting allowance exceeds i128::MAX.
    pub fn increase_allowance(
        env: Env,
        from: Address,
        spender: Address,
        delta: i128,
        expiration_ledger: u32,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        require_positive(&env, delta);
        from.require_auth();

        let current = get_allowance(&env, &from, &spender);
        let new_amount = current
            .checked_add(delta)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        set_allowance(&env, &from, &spender, new_amount, expiration_ledger);
        events::approve_event(&env, from, spender, new_amount, expiration_ledger);
    }

    /// Atomically decrease the allowance granted to `spender` by `delta`.
    ///
    /// Safe alternative to `approve` for lowering an existing allowance without
    /// the approval race condition.  If `delta` exceeds the current allowance
    /// the allowance is set to `0` (floors, does not panic).
    ///
    /// # Arguments
    /// * `from`              – Token holder (must sign).
    /// * `spender`           – Approved spender.
    /// * `delta`             – Amount to subtract. Must be > 0.
    /// * `expiration_ledger` – New expiry applied to the updated allowance.
    ///
    /// # Errors
    /// * [`ShareTokenError::ZeroAmount`] / [`ShareTokenError::NegativeAmount`]
    pub fn decrease_allowance(
        env: Env,
        from: Address,
        spender: Address,
        delta: i128,
        expiration_ledger: u32,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        require_positive(&env, delta);
        from.require_auth();

        let current = get_allowance(&env, &from, &spender);
        let new_amount = current.saturating_sub(delta).max(0);

        set_allowance(&env, &from, &spender, new_amount, expiration_ledger);
        events::approve_event(&env, from, spender, new_amount, expiration_ledger);
    }

    /// Transfer `amount` tokens from the caller to `to`.
    ///
    /// # Arguments
    /// * `from`   – Sender (must sign).
    /// * `to`     – Recipient.
    /// * `amount` – Number of tokens to send. Must be > 0.
    ///
    /// # Auth
    /// `from` must authorize this call.
    ///
    /// # Errors
    /// * [`ShareTokenError::NegativeAmount`]      if `amount < 0`.
    /// * [`ShareTokenError::ZeroAmount`]           if `amount == 0`.
    /// * [`ShareTokenError::InsufficientBalance`] if `from` has fewer tokens
    ///   than `amount`.
    /// * [`ShareTokenError::Overflow`]            on arithmetic overflow (should
    ///   never happen with valid i128 balances).
    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        require_positive(&env, amount);
        from.require_auth();

        let from_balance = get_balance(&env, &from);
        if from_balance < amount {
            panic_with_error!(&env, ShareTokenError::InsufficientBalance);
        }

        // Self-transfer: auth and balance check already done; no state change needed.
        if from == to {
            events::transfer_event(&env, from, to, amount);
            return;
        }

        let new_from_balance = from_balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        let to_balance = get_balance(&env, &to);
        let new_to_balance = to_balance
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        set_balance(&env, &from, new_from_balance);
        set_balance(&env, &to, new_to_balance);

        events::transfer_event(&env, from, to, amount);
    }

    /// Transfer `amount` tokens from `from` to `to` using a pre-approved
    /// allowance.
    ///
    /// The spender's allowance is decremented by `amount`.
    ///
    /// # Arguments
    /// * `spender` – Account authorized to move tokens (must sign).
    /// * `from`    – Token holder whose balance is debited.
    /// * `to`      – Recipient.
    /// * `amount`  – Number of tokens to send. Must be > 0.
    ///
    /// # Auth
    /// `spender` must authorize this call.
    ///
    /// # Errors
    /// * [`ShareTokenError::NegativeAmount`]          if `amount < 0`.
    /// * [`ShareTokenError::ZeroAmount`]               if `amount == 0`.
    /// * [`ShareTokenError::InsufficientAllowance`]   if allowance < amount.
    /// * [`ShareTokenError::InsufficientBalance`]     if `from` balance < amount.
    /// * [`ShareTokenError::Overflow`]                on arithmetic overflow.
    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        require_positive(&env, amount);
        spender.require_auth();

        let allowance_value = get_allowance_value(&env, &from, &spender);
        let (allowance, expiration_ledger) = allowance_value.map_or((0_i128, 0_u32), |v| {
            if env.ledger().sequence() > v.expiration_ledger {
                (0_i128, v.expiration_ledger)
            } else {
                (v.amount, v.expiration_ledger)
            }
        });
        if allowance < amount {
            panic_with_error!(&env, ShareTokenError::InsufficientAllowance);
        }

        let from_balance = get_balance(&env, &from);
        if from_balance < amount {
            panic_with_error!(&env, ShareTokenError::InsufficientBalance);
        }

        let new_allowance = allowance
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        set_allowance(&env, &from, &spender, new_allowance, expiration_ledger);

        // Self-transfer: consume allowance but do not touch balances — a double
        // write to the same storage key would leave the balance inflated by amount.
        if from == to {
            events::transfer_event(&env, from, to, amount);
            return;
        }

        let new_from_balance = from_balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        let to_balance = get_balance(&env, &to);
        let new_to_balance = to_balance
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        set_balance(&env, &from, new_from_balance);
        set_balance(&env, &to, new_to_balance);

        events::transfer_event(&env, from, to, amount);
    }

    /// Burn (destroy) `amount` tokens from the caller's own balance.
    ///
    /// Reduces `total_supply` accordingly.
    ///
    /// # Arguments
    /// * `from`   – Token holder whose tokens are destroyed (must sign).
    /// * `amount` – Number of tokens to burn. Must be > 0.
    ///
    /// # Auth
    /// `from` must authorize this call.
    ///
    /// # Errors
    /// * [`ShareTokenError::NegativeAmount`]      if `amount < 0`.
    /// * [`ShareTokenError::ZeroAmount`]           if `amount == 0`.
    /// * [`ShareTokenError::InsufficientBalance`] if `from` holds fewer tokens.
    /// * [`ShareTokenError::Overflow`]            on arithmetic underflow.
    pub fn burn(env: Env, from: Address, amount: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        require_positive(&env, amount);
        from.require_auth();

        let balance = get_balance(&env, &from);
        if balance < amount {
            panic_with_error!(&env, ShareTokenError::InsufficientBalance);
        }

        let new_balance = balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        let supply = get_total_supply(&env);
        let new_supply = supply
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        set_balance(&env, &from, new_balance);
        set_total_supply(&env, new_supply);

        events::burn_event(&env, from, amount);
    }

    /// Burn `amount` tokens from `from`'s balance using a pre-approved allowance.
    ///
    /// The spender's allowance is decremented by `amount`. Reduces
    /// `total_supply` accordingly.
    ///
    /// # Arguments
    /// * `spender` – Authorized burner (must sign).
    /// * `from`    – Token holder whose tokens are destroyed.
    /// * `amount`  – Number of tokens to burn. Must be > 0.
    ///
    /// # Auth
    /// `spender` must authorize this call.
    ///
    /// # Errors
    /// * [`ShareTokenError::NegativeAmount`]          if `amount < 0`.
    /// * [`ShareTokenError::ZeroAmount`]               if `amount == 0`.
    /// * [`ShareTokenError::InsufficientAllowance`]   if allowance < amount.
    /// * [`ShareTokenError::InsufficientBalance`]     if `from` balance < amount.
    /// * [`ShareTokenError::Overflow`]                on arithmetic overflow.
    pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        require_positive(&env, amount);
        spender.require_auth();

        let allowance_value = get_allowance_value(&env, &from, &spender);
        let (allowance, expiration_ledger) = allowance_value.map_or((0_i128, 0_u32), |v| {
            if env.ledger().sequence() > v.expiration_ledger {
                (0_i128, v.expiration_ledger)
            } else {
                (v.amount, v.expiration_ledger)
            }
        });
        if allowance < amount {
            panic_with_error!(&env, ShareTokenError::InsufficientAllowance);
        }

        let balance = get_balance(&env, &from);
        if balance < amount {
            panic_with_error!(&env, ShareTokenError::InsufficientBalance);
        }

        let new_allowance = allowance
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        let new_balance = balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        let supply = get_total_supply(&env);
        let new_supply = supply
            .checked_sub(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        set_allowance(&env, &from, &spender, new_allowance, expiration_ledger);
        set_balance(&env, &from, new_balance);
        set_total_supply(&env, new_supply);

        events::burn_event(&env, from, amount);
    }

    // -----------------------------------------------------------------------
    // Admin-only operations
    // -----------------------------------------------------------------------

    /// Mint `amount` new tokens and credit them to `to`.
    ///
    /// Only the admin (vault contract) may call this function.
    /// Increases `total_supply` accordingly.
    ///
    /// # Arguments
    /// * `to`     – Recipient of the newly created tokens.
    /// * `amount` – Number of tokens to mint. Must be > 0.
    ///
    /// # Auth
    /// The stored admin address must authorize this call.
    ///
    /// # Errors
    /// * [`ShareTokenError::NotInitialized`]  if the contract has not been set up.
    /// * [`ShareTokenError::NegativeAmount`]  if `amount < 0`.
    /// * [`ShareTokenError::ZeroAmount`]       if `amount == 0`.
    /// * [`ShareTokenError::Overflow`]        on arithmetic overflow.
    pub fn mint(env: Env, to: Address, amount: i128) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        require_positive(&env, amount);

        let admin = get_admin(&env);
        admin.require_auth();

        let to_balance = get_balance(&env, &to);
        let new_balance = to_balance
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        let supply = get_total_supply(&env);
        let new_supply = supply
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(&env, ShareTokenError::Overflow));

        set_balance(&env, &to, new_balance);
        set_total_supply(&env, new_supply);

        events::mint_event(&env, admin, to, amount);
    }

    /// Transfer admin rights to `new_admin`.
    ///
    /// After this call, only `new_admin` can mint tokens and call `set_admin`
    /// again. The old admin loses all privileged access.
    ///
    /// # Arguments
    /// * `new_admin` – The address that will become the new admin.
    ///
    /// # Auth
    /// The current admin must authorize this call.
    ///
    /// # Errors
    /// * [`ShareTokenError::NotInitialized`] if the contract has not been set up.
    pub fn set_admin(env: Env, new_admin: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        let admin = get_admin(&env);
        admin.require_auth();

        set_admin(&env, &new_admin);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
