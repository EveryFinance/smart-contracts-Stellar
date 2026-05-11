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

    /// The asset is not in the factory's authorized asset list.
    AssetNotAuthorized = 8,

    /// The asset is already in the factory's authorized asset list.
    AssetAlreadyAuthorized = 9,

    /// The guard contract is not in the factory's authorized guard list.
    GuardNotAuthorized = 10,

    /// The guard contract is already in the factory's authorized guard list.
    GuardAlreadyAuthorized = 11,

    /// The vault manager lookup failed — vault is not tracked by this factory.
    VaultManagerNotFound = 12,

    /// Seed deposit amount must be greater than zero.
    InvalidSeedAmount = 13,

    /// Asset is not registered in the AssetHandler contract — cannot authorize it.
    AssetNotInAssetHandler = 14,
}
