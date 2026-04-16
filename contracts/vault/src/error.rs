//! Vault contract error codes.
//!
//! Every variant maps to a unique `u32` discriminant that the Soroban host
//! surfaces as `Error(Contract, #N)` to callers and to off-chain indexers.
//! Discriminants are **stable** — do not renumber existing variants.

use soroban_sdk::contracterror;

/// All errors that the Vault contract can raise.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VaultError {
    /// `initialize` was already called on this contract instance.
    AlreadyInitialized = 1,

    /// An entry-point that requires prior initialization was called before
    /// `initialize`.
    NotInitialized = 2,

    /// The caller is not the registered manager address.
    NotManager = 3,

    /// The caller is not the registered trader address.
    NotTrader = 4,

    /// The vault is currently paused; deposits and withdrawals are blocked.
    Paused = 5,

    /// A zero or negative amount was supplied where a strictly-positive value
    /// is required (e.g. deposit or withdrawal amount).
    InvalidAmount = 6,

    /// The caller does not hold enough share tokens to satisfy the withdrawal.
    InsufficientShares = 7,

    /// The requested strategy address is not in the vault's whitelist.
    StrategyNotWhitelisted = 8,

    /// The trade guard rejected the proposed trade parameters.
    TradeGuardRejected = 9,

    /// An arithmetic operation overflowed `i128`.
    Overflow = 10,

    /// The share total supply is zero when a non-zero value is required
    /// (e.g. computing share price before any deposits).
    ZeroTotalSupply = 11,

    /// A strategy asset does not match the vault's configured base asset.
    AssetMismatch = 12,

    /// An operation requires a trade guard but none has been configured for the
    /// specified strategy.
    GuardNotSet = 13,

    /// Attempt to remove a strategy that still holds an active (non-zero)
    /// position.  Unwind the position before removing the strategy.
    StrategyHasActivePosition = 14,

    /// The deposit would push the vault's total NAV above the configured
    /// deposit cap.  Set `deposit_cap` to 0 to disable the cap.
    DepositCapExceeded = 15,

    /// The proposed invest would push a single strategy's NAV share above the
    /// configured `max_concentration_bps` limit.
    ConcentrationLimitExceeded = 16,

    /// A manager operation (invest / unwind / execute_trade) caused the vault's
    /// NAV to fall below `nav_before × (1 − max_loss_bps / 10_000)`.
    /// Indicates potential exploitation via slippage or an oracle attack.
    TvlGuardTripped = 17,

    /// Oracle token pricing is not supported for LP strategies; LP valuation
    /// must be handled inside the strategy contract.
    LpStrategyOracleUnsupported = 18,

    /// An LP strategy has a non-zero position but no internal oracle
    /// configured, so its value cannot be safely used for NAV.
    LpStrategyOracleRequired = 19,

    /// Requested withdrawal cannot be funded from vault cash plus auto-unwound
    /// single-asset strategies.
    InsufficientLiquidity = 20,

    /// Once a strategy is marked LP, the LP flag cannot be cleared.
    LpStrategyFlagImmutable = 21,

    /// The provided `share_token_admin` does not match the share token's
    /// current admin during vault initialization.
    ShareTokenAdminMismatch = 22,

    /// A withdrawal was attempted before cooldown elapsed since last deposit.
    CooldownActive = 23,

    /// Deposit attempted into a private pool by a non-allowlisted address.
    NotMember = 24,

    /// Attempted to commit an announced fee increase before delay elapsed.
    FeeIncreaseDelayActive = 25,

    /// No pending announced fee increase exists to commit.
    NoFeeIncreaseAnnounced = 26,

    /// Same-ledger operation-type mismatch detected for value-manipulation guard.
    OperationTypeMismatch = 27,

    /// Same-ledger NAV checkpoint mismatch detected for value-manipulation guard.
    ValueManipulationDetected = 28,
}
