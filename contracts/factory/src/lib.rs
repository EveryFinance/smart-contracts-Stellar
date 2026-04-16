//! # Factory — Vault Registry
//!
//! The Factory is a lightweight **registry** that tracks all vaults deployed
//! through this protocol. It does **not** deploy contracts itself (Soroban
//! contracts are deployed via `HostFunction::UploadContractWasm` /
//! `HostFunction::CreateContract` at the transaction level); instead the
//! deployer registers the resulting vault address with the factory after
//! calling `vault.initialize(…)`.
//!
//! ## Responsibilities
//! * Keep an append-only list of all registered vault addresses.
//! * Provide a paginated query interface so off-chain tooling can enumerate
//!   vaults without hitting gas limits.
//! * Allow the admin to remove a vault from the registry (does **not** destroy
//!   the vault contract).
//!
//! ## Auth model
//! * `initialize`     — permissionless (once).
//! * `register_vault` — admin only.
//! * `remove_vault`   — admin only.
//! * `set_admin`      — current admin only.
//! * `get_vaults` / `get_vault_count` — public.

#![no_std]

mod error;
mod events;
mod storage;

pub use error::FactoryError;

use soroban_sdk::{contract, contractimpl, panic_with_error, Address, Env, IntoVal, Symbol, Vec};

use storage::{
    get_admin, get_vaults, is_initialized, set_admin, set_vaults, INSTANCE_BUMP_AMOUNT,
    INSTANCE_LIFETIME_THRESHOLD,
};

use events::{admin_changed_event, vault_registered_event, vault_removed_event};

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct Factory;

#[contractimpl]
impl Factory {
    fn assert_not_registered(env: &Env, vault: &Address) {
        if Self::is_registered(env.clone(), vault.clone()) {
            panic_with_error!(env, FactoryError::VaultAlreadyRegistered);
        }
    }

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the factory with an admin address.
    ///
    /// # Errors
    /// * [`FactoryError::AlreadyInitialized`]
    pub fn initialize(env: Env, admin: Address) {
        if is_initialized(&env) {
            panic_with_error!(&env, FactoryError::AlreadyInitialized);
        }
        admin.require_auth();
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        set_admin(&env, &admin);
        set_vaults(&env, &Vec::new(&env));
    }

    // -----------------------------------------------------------------------
    // Registry operations
    // -----------------------------------------------------------------------

    /// Register a newly deployed vault. Admin only.
    ///
    /// The vault must already be initialized before registration.
    /// This function verifies `vault.get_manager()` and ensures the provided
    /// `manager` matches on-chain state.
    ///
    /// # Arguments
    /// * `caller`  – Must equal the admin.
    /// * `vault`   – Address of the initialized vault contract.
    /// * `manager` – Manager address (informational; stored in the event only).
    ///
    /// # Errors
    /// * [`FactoryError::NotAdmin`]
    /// * [`FactoryError::VaultAlreadyRegistered`]
    /// * [`FactoryError::ManagerMismatch`]
    pub fn register_vault(env: Env, caller: Address, vault: Address, manager: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }

        Self::assert_not_registered(&env, &vault);

        let onchain_manager: Address =
            env.invoke_contract(&vault, &Symbol::new(&env, "get_manager"), ().into_val(&env));
        if onchain_manager != manager {
            panic_with_error!(&env, FactoryError::ManagerMismatch);
        }

        let mut vaults = get_vaults(&env);
        vaults.push_back(vault.clone());
        set_vaults(&env, &vaults);
        vault_registered_event(&env, &vault, &onchain_manager);
    }

    /// Verify a vault is initialized and register it. Admin only.
    ///
    /// Unlike [`register_vault`], this function calls `vault.get_manager()` to
    /// confirm the vault contract exists **and** is properly initialized before
    /// adding it to the registry.  The manager address from the vault itself is
    /// used in the registration event, removing the need for the caller to supply
    /// it separately and preventing spoofed manager information.
    ///
    /// # Errors
    /// * [`FactoryError::NotAdmin`]
    /// * [`FactoryError::VaultAlreadyRegistered`]
    /// * Panics (propagated) if `vault.get_manager()` fails — i.e. the vault is
    ///   not initialized or the address is not a vault contract.
    pub fn verify_and_register_vault(env: Env, caller: Address, vault: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }

        Self::assert_not_registered(&env, &vault);

        // Verify the vault is initialized by reading its manager from the contract.
        let manager: Address =
            env.invoke_contract(&vault, &Symbol::new(&env, "get_manager"), ().into_val(&env));

        let mut vaults = get_vaults(&env);
        vaults.push_back(vault.clone());
        set_vaults(&env, &vaults);
        vault_registered_event(&env, &vault, &manager);
    }

    /// Remove a vault from the registry. Admin only.
    ///
    /// The vault contract itself is not destroyed; it is only de-listed.
    ///
    /// # Errors
    /// * [`FactoryError::NotAdmin`]
    /// * [`FactoryError::VaultNotFound`]
    pub fn remove_vault(env: Env, caller: Address, vault: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }

        let vaults = get_vaults(&env);
        let mut found = false;
        let mut new_vaults: Vec<Address> = Vec::new(&env);
        for v in vaults.iter() {
            if v == vault && !found {
                // Remove only the first occurrence.
                found = true;
            } else {
                new_vaults.push_back(v);
            }
        }
        if !found {
            panic_with_error!(&env, FactoryError::VaultNotFound);
        }
        set_vaults(&env, &new_vaults);
        vault_removed_event(&env, &vault);
    }

    // -----------------------------------------------------------------------
    // Admin management
    // -----------------------------------------------------------------------

    /// Transfer admin role to a new address. Current admin only.
    ///
    /// # Errors
    /// * [`FactoryError::NotAdmin`]
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }

        set_admin(&env, &new_admin);
        admin_changed_event(&env, &new_admin);
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// Return a page of registered vault addresses.
    ///
    /// # Arguments
    /// * `offset` – Starting index (0-based).
    /// * `limit`  – Maximum number of entries to return (capped at 50).
    pub fn get_vaults(env: Env, offset: u32, limit: u32) -> Vec<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        let all = get_vaults(&env);
        let total = all.len();
        let cap = limit.min(50);
        let mut result: Vec<Address> = Vec::new(&env);

        let mut i = offset;
        while i < total && i < offset + cap {
            result.push_back(all.get(i).unwrap());
            i += 1;
        }
        result
    }

    /// Return the total number of registered vaults.
    pub fn get_vault_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_vaults(&env).len()
    }

    /// Return the admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_admin(&env)
    }

    /// Return true if `vault` is registered.
    pub fn is_registered(env: Env, vault: Address) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let vaults = get_vaults(&env);
        for v in vaults.iter() {
            if v == vault {
                return true;
            }
        }
        false
    }
}

#[cfg(test)]
mod test;
