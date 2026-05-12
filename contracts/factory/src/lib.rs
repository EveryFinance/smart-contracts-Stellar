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

use soroban_sdk::{
    contract, contractimpl, panic_with_error, token, Address, Env, IntoVal, Symbol, Vec,
};

use storage::{
    clear_pending_admin, get_admin, get_asset_handler, get_authorized_assets,
    get_authorized_guards, get_is_registered, get_pending_admin, get_vault_by_index,
    get_vault_count, get_vault_manager, get_vault_position, is_authorized_asset,
    is_authorized_guard, is_factory_initialized, remove_registered, remove_vault_by_index,
    remove_vault_position, set_admin, set_asset_handler, set_authorized_assets,
    set_authorized_guards, set_factory_initialized, set_pending_admin, set_registered,
    set_vault_by_index, set_vault_count, set_vault_manager, set_vault_position, DataKey,
    INSTANCE_BUMP_AMOUNT, INSTANCE_LIFETIME_THRESHOLD, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_LIFETIME_THRESHOLD,
};

use events::{
    admin_changed_event, asset_authorized_event, asset_deauthorized_event, guard_authorized_event,
    guard_deauthorized_event, vault_created_event, vault_manager_set_event, vault_registered_event,
    vault_removed_event,
};

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct Factory;

#[contractimpl]
impl Factory {
    fn assert_not_registered(env: &Env, vault: &Address) {
        if get_is_registered(env, vault) {
            panic_with_error!(env, FactoryError::VaultAlreadyRegistered);
        }
    }

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Initialize the factory. Runs atomically at deployment via `CreateContract`.
    ///
    /// # Arguments
    /// * `admin`         – Registry admin address.
    /// * `asset_handler` – The AssetHandler contract that holds per-asset oracles.
    ///                     Pass `None` to initialize without a handler (can be set later).
    pub fn __constructor(env: Env, admin: Address, asset_handler: Option<Address>) {
        if is_factory_initialized(&env) {
            panic_with_error!(&env, FactoryError::AlreadyInitialized);
        }
        admin.require_auth();

        set_admin(&env, &admin);
        set_vault_count(&env, 0);
        if let Some(ref ah) = asset_handler {
            set_asset_handler(&env, ah);
        }
        set_factory_initialized(&env);
    }

    /// Set or update the AssetHandler reference. Admin only.
    pub fn set_asset_handler(env: Env, caller: Address, asset_handler: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }
        set_asset_handler(&env, &asset_handler);
    }

    /// Return the AssetHandler contract address.
    pub fn get_asset_handler(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_asset_handler(&env)
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

        let idx = get_vault_count(&env);
        set_vault_by_index(&env, idx, &vault);
        set_vault_position(&env, &vault, idx);
        set_registered(&env, &vault);
        set_vault_manager(&env, &vault, &onchain_manager);
        set_vault_count(&env, idx + 1);
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

        let idx = get_vault_count(&env);
        set_vault_by_index(&env, idx, &vault);
        set_vault_position(&env, &vault, idx);
        set_registered(&env, &vault);
        set_vault_manager(&env, &vault, &manager);
        set_vault_count(&env, idx + 1);
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

        if !get_is_registered(&env, &vault) {
            panic_with_error!(&env, FactoryError::VaultNotFound);
        }

        let count = get_vault_count(&env);
        let last_idx = count - 1;
        let remove_idx = get_vault_position(&env, &vault);

        // Swap-and-pop: fill the gap with the last entry (O(1) writes).
        if remove_idx != last_idx {
            let last_vault = get_vault_by_index(&env, last_idx);
            set_vault_by_index(&env, remove_idx, &last_vault);
            set_vault_position(&env, &last_vault, remove_idx);
        }
        remove_vault_by_index(&env, last_idx);
        remove_vault_position(&env, &vault);
        remove_registered(&env, &vault);
        set_vault_count(&env, last_idx);
        vault_removed_event(&env, &vault);
    }

    // -----------------------------------------------------------------------
    // Admin management
    // -----------------------------------------------------------------------

    /// Begin a two-step admin transfer. Current admin only.
    ///
    /// Records `new_admin` as the pending admin; the transfer is not complete
    /// until `new_admin` calls [`accept_admin`].  This prevents accidental
    /// lock-out if a wrong address is supplied.
    ///
    /// # Errors
    /// * [`FactoryError::NotAdmin`]
    pub fn set_pending_admin(env: Env, caller: Address, new_admin: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }

        set_pending_admin(&env, &new_admin);
    }

    /// Complete a two-step admin transfer. Pending admin only.
    ///
    /// The address previously set via [`set_pending_admin`] must authorize
    /// this call. On success, that address becomes the new admin.
    ///
    /// # Errors
    /// * [`FactoryError::NoPendingAdmin`] if no transfer is in progress.
    pub fn accept_admin(env: Env, caller: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        caller.require_auth();

        let pending = get_pending_admin(&env)
            .unwrap_or_else(|| panic_with_error!(&env, FactoryError::NoPendingAdmin));

        if caller != pending {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }

        set_admin(&env, &pending);
        clear_pending_admin(&env);
        admin_changed_event(&env, &pending);
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

        let total = get_vault_count(&env);
        let cap = limit.min(50);
        let mut result: Vec<Address> = Vec::new(&env);

        let mut i = offset;
        while i < total && i < offset + cap {
            result.push_back(get_vault_by_index(&env, i));
            i += 1;
        }
        result
    }

    /// Return the total number of registered vaults.
    pub fn get_vault_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_vault_count(&env)
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
        get_is_registered(&env, &vault)
    }

    /// Return the admin-assigned manager for a registered vault.
    ///
    /// # Errors
    /// * [`FactoryError::VaultManagerNotFound`] if the vault is not tracked.
    pub fn get_vault_manager(env: Env, vault: Address) -> Address {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_vault_manager(&env, &vault)
            .unwrap_or_else(|| panic_with_error!(&env, FactoryError::VaultManagerNotFound))
    }

    /// Return the full list of protocol-authorized assets.
    pub fn get_authorized_assets(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_authorized_assets(&env)
    }

    /// Return `true` if `asset` is in the protocol-authorized asset list.
    pub fn is_authorized_asset(env: Env, asset: Address) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        is_authorized_asset(&env, &asset)
    }

    /// Return the full list of protocol-authorized guard contracts.
    pub fn get_authorized_guards(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        get_authorized_guards(&env)
    }

    /// Return `true` if `guard` is in the protocol-authorized guard list.
    pub fn is_authorized_guard(env: Env, guard: Address) -> bool {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        is_authorized_guard(&env, &guard)
    }

    // -----------------------------------------------------------------------
    // Asset whitelist management (admin only)
    // -----------------------------------------------------------------------

    /// Add an asset to the protocol-wide authorized asset list.
    ///
    /// The asset must already be registered in the AssetHandler (with its price
    /// oracle) before it can be authorized here. The AssetHandler is the single
    /// source of truth for per-asset oracles.
    ///
    /// # Errors
    /// * [`FactoryError::NotAdmin`]
    /// * [`FactoryError::AssetAlreadyAuthorized`]
    /// * [`FactoryError::AssetHandlerNotSet`] if no AssetHandler is configured
    /// * Panics if asset is not registered in AssetHandler
    pub fn add_authorized_asset(env: Env, caller: Address, asset: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }
        if is_authorized_asset(&env, &asset) {
            panic_with_error!(&env, FactoryError::AssetAlreadyAuthorized);
        }

        // Validate that the asset has an oracle registered in AssetHandler.
        if let Some(ah) = get_asset_handler(&env) {
            let is_reg: bool = env.invoke_contract(
                &ah,
                &Symbol::new(&env, "is_registered"),
                (asset.clone(),).into_val(&env),
            );
            if !is_reg {
                panic_with_error!(&env, FactoryError::AssetNotInAssetHandler);
            }
        }
        // If no AssetHandler set, allow freely (factory may be used without one).

        let mut assets = get_authorized_assets(&env);
        assets.push_back(asset.clone());
        set_authorized_assets(&env, &assets);
        asset_authorized_event(&env, &asset);
    }

    /// Remove an asset from the protocol-wide authorized asset list.
    ///
    /// # Errors
    /// * [`FactoryError::NotAdmin`]
    /// * [`FactoryError::AssetNotAuthorized`]
    pub fn remove_authorized_asset(env: Env, caller: Address, asset: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }
        if !is_authorized_asset(&env, &asset) {
            panic_with_error!(&env, FactoryError::AssetNotAuthorized);
        }
        let assets = get_authorized_assets(&env);
        let mut updated: Vec<Address> = Vec::new(&env);
        for a in assets.iter() {
            if a != asset {
                updated.push_back(a);
            }
        }
        set_authorized_assets(&env, &updated);
        asset_deauthorized_event(&env, &asset);
    }

    // -----------------------------------------------------------------------
    // Guard whitelist management (admin only)
    // -----------------------------------------------------------------------

    /// Add a strategy guard contract to the protocol-wide authorized guard list.
    ///
    /// # Errors
    /// * [`FactoryError::NotAdmin`]
    /// * [`FactoryError::GuardAlreadyAuthorized`]
    pub fn add_authorized_guard(env: Env, caller: Address, guard: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }
        if is_authorized_guard(&env, &guard) {
            panic_with_error!(&env, FactoryError::GuardAlreadyAuthorized);
        }
        let mut guards = get_authorized_guards(&env);
        guards.push_back(guard.clone());
        set_authorized_guards(&env, &guards);
        guard_authorized_event(&env, &guard);
    }

    /// Remove a strategy guard contract from the authorized guard list.
    ///
    /// # Errors
    /// * [`FactoryError::NotAdmin`]
    /// * [`FactoryError::GuardNotAuthorized`]
    pub fn remove_authorized_guard(env: Env, caller: Address, guard: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }
        if !is_authorized_guard(&env, &guard) {
            panic_with_error!(&env, FactoryError::GuardNotAuthorized);
        }
        let guards = get_authorized_guards(&env);
        let mut updated: Vec<Address> = Vec::new(&env);
        for g in guards.iter() {
            if g != guard {
                updated.push_back(g);
            }
        }
        set_authorized_guards(&env, &updated);
        guard_deauthorized_event(&env, &guard);
    }

    // -----------------------------------------------------------------------
    // Vault manager assignment (admin only)
    // -----------------------------------------------------------------------

    /// Assign or reassign the manager for a registered vault.
    ///
    /// The admin is the only party that can change vault managers.  This
    /// function updates the factory's `VaultManager` record and also calls
    /// `vault.set_manager(new_manager)` so the vault's own state stays in sync.
    ///
    /// # Errors
    /// * [`FactoryError::NotAdmin`]
    /// * [`FactoryError::VaultNotFound`]
    pub fn set_vault_manager(env: Env, caller: Address, vault: Address, new_manager: Address) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }
        if !get_is_registered(&env, &vault) {
            panic_with_error!(&env, FactoryError::VaultNotFound);
        }
        set_vault_manager(&env, &vault, &new_manager);
        // Propagate through the factory identity configured on the vault.
        let args = (env.current_contract_address(), new_manager.clone()).into_val(&env);
        env.invoke_contract::<()>(&vault, &Symbol::new(&env, "set_manager"), args);
        vault_manager_set_event(&env, &vault, &new_manager);
    }

    // -----------------------------------------------------------------------
    // Vault creation with seed deposit (admin only)
    // -----------------------------------------------------------------------

    /// Register an already-deployed vault and perform the anti-inflation seed deposit.
    ///
    /// The admin must:
    /// 1. Deploy and initialize the vault externally (via `__constructor`).
    /// 2. Approve `seed_amount` of `base_asset` from their account to this factory.
    /// 3. Call this function.
    ///
    /// The factory will:
    /// 1. Verify the vault is not already registered.
    /// 2. Pull `seed_amount` of `base_asset` from `caller` into the vault directly.
    /// 3. Call `vault.seed_deposit(seed_amount)` to mint seed shares to the burn address.
    /// 4. Register the vault and record its manager.
    ///
    /// After this call, `total_supply > 0` which eliminates the first-depositor
    /// inflation attack.
    ///
    /// # Errors
    /// * [`FactoryError::NotAdmin`]
    /// * [`FactoryError::VaultAlreadyRegistered`]
    /// * [`FactoryError::InvalidSeedAmount`]
    pub fn create_vault(
        env: Env,
        caller: Address,
        vault: Address,
        manager: Address,
        base_asset: Address,
        seed_amount: i128,
    ) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        caller.require_auth();
        if caller != get_admin(&env) {
            panic_with_error!(&env, FactoryError::NotAdmin);
        }
        if seed_amount <= 0 {
            panic_with_error!(&env, FactoryError::InvalidSeedAmount);
        }
        Self::assert_not_registered(&env, &vault);

        // Verify manager matches vault's on-chain state.
        let onchain_manager: Address =
            env.invoke_contract(&vault, &Symbol::new(&env, "get_manager"), ().into_val(&env));
        if onchain_manager != manager {
            panic_with_error!(&env, FactoryError::ManagerMismatch);
        }

        // Transfer seed_amount of base_asset from admin into the vault.
        token::Client::new(&env, &base_asset).transfer_from(
            &env.current_contract_address(),
            &caller,
            &vault,
            &seed_amount,
        );

        // Instruct the vault to mint seed shares to the burn address.
        let args = (env.current_contract_address(), seed_amount).into_val(&env);
        env.invoke_contract::<()>(&vault, &Symbol::new(&env, "seed_deposit"), args);

        // Register the vault.
        let idx = get_vault_count(&env);
        set_vault_by_index(&env, idx, &vault);
        set_vault_position(&env, &vault, idx);
        set_registered(&env, &vault);
        set_vault_count(&env, idx + 1);
        set_vault_manager(&env, &vault, &manager);

        vault_created_event(&env, &vault, &manager, seed_amount);
    }

    /// Bump the persistent TTL for vault registry entries in index range `[start, end)`.
    ///
    /// This function is **permissionless**: any account may call it to pay for
    /// TTL renewal, preventing registry entries from expiring silently.
    ///
    /// # Arguments
    /// * `start` – First index to touch (inclusive).
    /// * `end`   – One-past-last index to touch (exclusive).
    ///
    /// If `start >= end` or `start >= vault_count` the call is a no-op.
    /// `end` is clamped to `vault_count` so callers can safely pass `u32::MAX`.
    pub fn touch_vaults(env: Env, start: u32, end: u32) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        let count = get_vault_count(&env);
        if start >= count || start >= end {
            return;
        }
        let clamped_end = end.min(count);
        let mut i = start;
        while i < clamped_end {
            let vault = get_vault_by_index(&env, i);
            // Bump all three persistent keys for this vault slot.
            let key_by_index = DataKey::VaultByIndex(i);
            let key_position = DataKey::VaultPosition(vault.clone());
            let key_registered = DataKey::IsRegistered(vault.clone());
            env.storage().persistent().extend_ttl(
                &key_by_index,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            env.storage().persistent().extend_ttl(
                &key_position,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            env.storage().persistent().extend_ttl(
                &key_registered,
                PERSISTENT_LIFETIME_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
            i += 1;
        }
    }
}

#[cfg(test)]
mod test;
