//! PhoenixTradeGuard error codes.
//!
//! Discriminants are **stable** — do not renumber existing variants.

use soroban_sdk::contracterror;

/// All errors that the PhoenixTradeGuard contract can raise.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum PhoenixGuardError {
    /// `initialize` was already called on this contract instance.
    AlreadyInitialized = 1,

    /// An entry-point that requires prior initialization was called before
    /// `initialize`.
    NotInitialized = 2,

    /// The caller of `validate_swap` or `validate_swap_exact_in` is not the
    /// registered vault address.
    NotVault = 3,

    /// The caller of `set_whitelist` is not the registered manager address.
    NotManager = 4,

    /// A token referenced in a swap operation (`offer_asset` or `ask_asset`)
    /// is not present in the manager-controlled whitelist.
    TokenNotWhitelisted = 5,

    /// The implied slippage `(amount_in - min_out) / amount_in` exceeds
    /// [`MAX_SLIPPAGE_BPS`](crate::storage::MAX_SLIPPAGE_BPS).
    SlippageTooHigh = 6,

    /// A zero or negative `amount_in` was supplied.
    InvalidAmount = 7,

    /// The `operations` list is empty — at least one swap operation is required.
    OperationsEmpty = 8,

    /// The `operations` list contains more than
    /// [`MAX_OPERATIONS`](crate::storage::MAX_OPERATIONS) hops.
    OperationsTooMany = 9,

    /// Adjacent swap operations are not contiguous: `operations[i].ask_asset ≠
    /// operations[i+1].offer_asset`.  A non-contiguous path could pass token
    /// whitelist checks while routing through a different (unvalidated) asset.
    NonContiguousHops = 10,
}
