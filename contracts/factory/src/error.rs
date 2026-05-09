//! Factory contract error codes.
//!
//! Discriminants are **stable** — do not renumber existing variants.

use soroban_sdk::contracterror;

/// All errors that the Factory contract can raise.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FactoryError {
    /// `initialize` was already called on this contract instance.
    AlreadyInitialized = 1,

    /// An entry-point that requires prior initialization was called before
    /// `initialize`.
    NotInitialized = 2,

    /// The caller is not the registered admin address.
    NotAdmin = 3,

    /// The vault address passed to `remove_vault` is not present in the
    /// registry.
    VaultNotFound = 4,

    /// The vault is already present in the registry.
    VaultAlreadyRegistered = 5,

    /// The manager supplied to `register_vault` does not match
    /// `vault.get_manager()`.
    ManagerMismatch = 6,

    /// `accept_admin` was called but no pending admin transfer is in progress.
    NoPendingAdmin = 7,
}
