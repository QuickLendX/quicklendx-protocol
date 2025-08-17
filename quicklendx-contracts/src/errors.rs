use soroban_sdk::{contracterror, symbol_short, Symbol};

/// Custom error types for the QuickLendX contract
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum QuickLendXError {
 // Invoice errors (1000-1099)
 InvoiceNotFound = 1000,
 InvoiceAlreadyExists = 1001,
 InvoiceNotAvailableForFunding = 1002,
 InvoiceAlreadyFunded = 1003,
 InvoiceAmountInvalid = 1004,
 InvoiceDueDateInvalid = 1005,
 InvoiceNotVerified = 1006,
 InvoiceNotFunded = 1007,
 InvoiceAlreadyPaid = 1008,
 InvoiceAlreadyDefaulted = 1009,

 // Authorization errors (1100-1199)
 Unauthorized = 1100,
 NotBusinessOwner = 1101,
 NotInvestor = 1102,
 NotAdmin = 1103,

 // Validation errors (1200-1299)
 InvalidAmount = 1200,
 InvalidAddress = 1201,
 InvalidCurrency = 1202,
 InvalidTimestamp = 1203,
 InvalidDescription = 1204,

 // Storage errors (1300-1399)
 StorageError = 1300,
 StorageKeyNotFound = 1301,

 // Business logic errors (1400-1499)
 InsufficientFunds = 1400,
 InvalidStatus = 1401,
 OperationNotAllowed = 1402,

 // Rating errors (1500-1599, from feat-invoice_rating_system)
 InvalidRating = 1500,
 NotFunded = 1501,
 AlreadyRated = 1502,
 NotRater = 1503,

 // KYC/Verification errors (1600-1699, from main)
 BusinessNotVerified = 1600,
 KYCAlreadyPending = 1601,
 KYCAlreadyVerified = 1602,
 KYCNotFound = 1603,
 InvalidKYCStatus = 1604,

 // Audit errors (1700-1799)
 AuditLogNotFound = 1700,
 AuditValidationFailed = 1701,
 AuditIntegrityError = 1702,
 AuditQueryError = 1703,

 // Category and Tag errors (1800-1899)
 InvalidCategory = 1800,
 CategoryNotFound = 1801,
 InvalidTag = 1802,
 TagLimitExceeded = 1803,
}

impl From<QuickLendXError> for Symbol {
 fn from(error: QuickLendXError) -> Self {
 match error {
 QuickLendXError::InvoiceNotFound => symbol_short!("INV_NF"),
 QuickLendXError::InvoiceAlreadyExists => symbol_short!("INV_EX"),
 QuickLendXError::InvoiceNotAvailableForFunding => symbol_short!("INV_NA"),
 QuickLendXError::InvoiceAlreadyFunded => symbol_short!("INV_FD"),
 QuickLendXError::InvoiceAmountInvalid => symbol_short!("INV_AI"),
 QuickLendXError::InvoiceDueDateInvalid => symbol_short!("INV_DI"),
 QuickLendXError::InvoiceNotVerified => symbol_short!("INV_NV"),
 QuickLendXError::InvoiceNotFunded => symbol_short!("INV_NF"),
 QuickLendXError::InvoiceAlreadyPaid => symbol_short!("INV_PD"),
 QuickLendXError::InvoiceAlreadyDefaulted => symbol_short!("INV_DF"),
 QuickLendXError::Unauthorized => symbol_short!("UNAUTH"),
 QuickLendXError::NotBusinessOwner => symbol_short!("NOT_OWN"),
 QuickLendXError::NotInvestor => symbol_short!("NOT_INV"),
 QuickLendXError::NotAdmin => symbol_short!("NOT_ADM"),
 QuickLendXError::InvalidAmount => symbol_short!("INV_AMT"),
 QuickLendXError::InvalidAddress => symbol_short!("INV_ADR"),
 QuickLendXError::InvalidCurrency => symbol_short!("INV_CR"),
 QuickLendXError::InvalidTimestamp => symbol_short!("INV_TM"),
 QuickLendXError::InvalidDescription => symbol_short!("INV_DS"),
 QuickLendXError::StorageError => symbol_short!("STORE"),
 QuickLendXError::StorageKeyNotFound => symbol_short!("KEY_NF"),
 QuickLendXError::InsufficientFunds => symbol_short!("INSUF"),
 QuickLendXError::InvalidStatus => symbol_short!("INV_ST"),
 QuickLendXError::OperationNotAllowed => symbol_short!("OP_NA"),
 QuickLendXError::InvalidRating => symbol_short!("INV_RT"),
 QuickLendXError::NotFunded => symbol_short!("NOT_FD"),
 QuickLendXError::AlreadyRated => symbol_short!("ALR_RT"),
 QuickLendXError::NotRater => symbol_short!("NOT_RT"),
 QuickLendXError::BusinessNotVerified => symbol_short!("BUS_NV"),
 QuickLendXError::KYCAlreadyPending => symbol_short!("KYC_PD"),
 QuickLendXError::KYCAlreadyVerified => symbol_short!("KYC_VF"),
 QuickLendXError::KYCNotFound => symbol_short!("KYC_NF"),
 QuickLendXError::InvalidKYCStatus => symbol_short!("KYC_IS"),
 // Add to Symbol conversion
 QuickLendXError::AuditLogNotFound => symbol_short!("AUD_NF"),
 QuickLendXError::AuditValidationFailed => symbol_short!("AUD_VF"),
 QuickLendXError::AuditIntegrityError => symbol_short!("AUD_IE"),
 QuickLendXError::AuditQueryError => symbol_short!("AUD_QE"),
 // Category and Tag errors
 QuickLendXError::InvalidCategory => symbol_short!("INV_CAT"),
 QuickLendXError::CategoryNotFound => symbol_short!("CAT_NF"),
 QuickLendXError::InvalidTag => symbol_short!("INV_TAG"),
 QuickLendXError::TagLimitExceeded => symbol_short!("TAG_LIM"),
 }
 }
}
