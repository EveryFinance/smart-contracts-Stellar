use soroban_sdk::contracterror;

/// Errors that can be returned by the ShareToken contract.
///
/// Each variant maps to a unique u32 discriminant so that the Soroban host
/// can surface them as typed contract errors to callers.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ShareTokenError {
    /// The contract has not yet been initialized via `initialize`.
    NotInitialized = 1,

    /// `initialize` was called on a contract that is already initialized.
    AlreadyInitialized = 2,

    /// The caller does not have the required authority (e.g. non-admin tried
    /// to mint).
    NotAuthorized = 3,

    /// The source account does not hold enough tokens for the requested
    /// transfer or burn.
    InsufficientBalance = 4,

    /// The spender's allowance is smaller than the requested `transfer_from`
    /// or `burn_from` amount.
    InsufficientAllowance = 5,

    /// A negative amount was supplied where only non-negative values are
    /// accepted.
    NegativeAmount = 6,

    /// A zero amount was supplied where a strictly-positive value is required.
    ZeroAmount = 7,

    /// An arithmetic operation would overflow i128.
    Overflow = 8,
}
