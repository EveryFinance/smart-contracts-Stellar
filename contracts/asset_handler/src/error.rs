use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AssetHandlerError {
    AlreadyInitialized    = 1,
    NotInitialized        = 2,
    NotAdmin              = 3,
    NoPendingAdmin        = 4,
    AssetAlreadyRegistered = 5,
    AssetNotRegistered    = 6,
    NoPrimaryOracle       = 7,
    PriceNotAvailable     = 8,
}
