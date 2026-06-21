/// Unified error type for the QuickLendX protocol contract.
use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ContractError {
    /// Contract has not been initialized.
    NotInitialized = 1,
    /// Contract has already been initialized.
    AlreadyInitialized = 2,
    /// Caller is not the admin.
    NotAdmin = 3,
    /// The requested operation is not allowed in the current state.
    OperationNotAllowed = 4,
    /// A provided amount is invalid (e.g. zero or negative).
    InvalidAmount = 5,
    /// A provided fee value is invalid.
    InvalidFee = 6,
    /// A generic parameter is out of the accepted range.
    InvalidParameter = 7,
}
