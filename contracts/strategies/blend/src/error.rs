use soroban_sdk::contracterror;

/// All errors that the Blend strategy contract can raise.
///
/// Each variant maps to a unique `u32` discriminant so that the Soroban host
/// surfaces them as typed contract errors to external callers and to the vault.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BlendStrategyError {
    /// `initialize` was already called on this contract instance.
    AlreadyInitialized = 1,

    /// An entry-point that requires prior initialization was called before
    /// `initialize`.
    NotInitialized = 2,

    /// The caller of `deposit` or `withdraw` is not the registered vault
    /// address.
    NotVault = 3,

    /// The caller of `pause` or `unpause` is not the registered manager
    /// address.
    NotManager = 4,

    /// The strategy is currently paused; no deposits or withdrawals are
    /// accepted.
    Paused = 5,

    /// A zero or negative amount was supplied where a strictly-positive value
    /// is required.
    InvalidAmount = 6,

    /// The requested withdrawal amount exceeds the strategy's current tracked
    /// position.
    InsufficientPosition = 7,

    /// An arithmetic operation overflowed `i128`.
    Overflow = 8,
}
