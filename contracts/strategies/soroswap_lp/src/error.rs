//! SoroswapLpStrategy error codes.
//!
//! Discriminants are **stable** — do not renumber existing variants.

use soroban_sdk::contracterror;

/// All errors that the SoroswapLpStrategy contract can raise.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SoroswapLpError {
    /// `initialize` was already called on this contract instance.
    AlreadyInitialized = 1,

    /// An entry-point that requires prior initialization was called before
    /// `initialize`.
    NotInitialized = 2,

    /// The caller of `deposit_liquidity` or `withdraw` is not the registered
    /// vault address.
    NotVault = 3,

    /// The caller of `pause`, `unpause`, or `set_oracle` is not the registered
    /// manager address.
    NotManager = 4,

    /// The strategy is currently paused; `deposit_liquidity` and `withdraw`
    /// are blocked.
    Paused = 5,

    /// A zero or negative amount was supplied where a strictly-positive value
    /// is required.
    InvalidAmount = 6,

    /// The requested LP withdrawal amount exceeds the strategy's tracked LP
    /// token balance.
    InsufficientLpBalance = 7,

    /// An arithmetic operation overflowed `i128`.
    Overflow = 8,

    /// An oracle returned a non-positive price, which would corrupt NAV math.
    InvalidOraclePrice = 9,
}
