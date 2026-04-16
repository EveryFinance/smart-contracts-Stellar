use soroban_sdk::contracterror;

/// Errors emitted by the Oracle contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum OracleError {
    /// Contract has not been initialized.
    NotInitialized = 1,
    /// Contract has already been initialized.
    AlreadyInitialized = 2,
    /// Caller is not the admin.
    NotAuthorized = 3,
    /// No price has been set for the requested asset.
    PriceNotFound = 4,
    /// Prices must be strictly positive.
    NonPositivePrice = 5,
    /// Price exists but is older than the allowed freshness threshold.
    StalePrice = 6,
}
