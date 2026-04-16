//! Factory event publishers.
//!
//! Off-chain indexers can subscribe to these events to track the lifecycle of
//! all vaults in the protocol without polling on-chain state.
//!
//! ## Topic layout
//! Each event uses a two-element topic `(event_symbol, factory_address)` so
//! subscribers can filter by event type or by factory instance.

use soroban_sdk::{Address, Env, Symbol};

/// Emitted when a vault is successfully added to the registry.
///
/// # Data
/// `(vault: Address, manager: Address)`
/// * `vault`   — The newly registered vault contract address.
/// * `manager` — The vault's manager address (from `vault.get_manager()`).
pub fn vault_registered_event(env: &Env, vault: &Address, manager: &Address) {
    env.events().publish(
        (
            Symbol::new(env, "vault_registered"),
            env.current_contract_address(),
        ),
        (vault.clone(), manager.clone()),
    );
}

/// Emitted when a vault is removed from the registry.
///
/// # Data
/// `vault: Address` — The de-listed vault contract address.
/// The vault contract itself is not destroyed; it is only removed from the
/// registry's tracked list.
pub fn vault_removed_event(env: &Env, vault: &Address) {
    env.events().publish(
        (
            Symbol::new(env, "vault_removed"),
            env.current_contract_address(),
        ),
        vault.clone(),
    );
}

/// Emitted when the admin role is transferred via `set_admin`.
///
/// # Data
/// `new_admin: Address` — The address that is now the factory admin.
pub fn admin_changed_event(env: &Env, new_admin: &Address) {
    env.events().publish(
        (
            Symbol::new(env, "admin_changed"),
            env.current_contract_address(),
        ),
        new_admin.clone(),
    );
}
