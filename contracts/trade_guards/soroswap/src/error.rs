//! SoroswapTradeGuard error codes.
//!
//! Discriminants are **stable** — do not renumber existing variants.

use soroban_sdk::contracterror;

/// All errors that the SoroswapTradeGuard contract can raise.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SoroswapGuardError {
    /// `initialize` was already called on this contract instance.
    AlreadyInitialized = 1,

    /// An entry-point that requires prior initialization was called before
    /// `initialize`.
    NotInitialized = 2,

    /// The caller of `validate_swap_exact_in` or `validate_swap_exact_out` is
    /// not the registered vault address.
    NotVault = 3,

    /// The caller of `set_whitelist` is not the registered manager address.
    NotManager = 4,

    /// A token in the swap path is not present in the manager-controlled
    /// whitelist.
    TokenNotWhitelisted = 5,

    /// The implied slippage `(amount_in - min_out) / amount_in` exceeds
    /// [`MAX_SLIPPAGE_BPS`](crate::storage::MAX_SLIPPAGE_BPS).
    SlippageTooHigh = 6,

    /// A zero or negative amount was supplied where a strictly-positive value
    /// is required.
    InvalidAmount = 7,

    /// The swap path contains more than [`MAX_PATH_LEN`](crate::storage::MAX_PATH_LEN)
    /// token addresses.
    PathTooLong = 8,

    /// The swap path contains fewer than 2 token addresses (no valid swap
    /// direction can be inferred).
    PathTooShort = 9,
}
