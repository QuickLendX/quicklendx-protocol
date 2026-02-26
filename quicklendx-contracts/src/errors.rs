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
    PaymentTooLow = 1403,
    PlatformAccountNotConfigured = 1404,
    InvalidCoveragePercentage = 1405,
    MaxBidsPerInvoiceExceeded = 1406,

    // Rating errors (1500-1599)
    InvalidRating = 1500,
    NotFunded = 1501,
    AlreadyRated = 1502,
    NotRater = 1503,

    // KYC/Verification errors (1600-1699)
    BusinessNotVerified = 1600,
    KYCAlreadyPending = 1601,
    KYCAlreadyVerified = 1602,
    KYCNotFound = 1603,
    InvalidKYCStatus = 1604,

    // Audit errors (1700-1799)
    AuditLogNotFound = 1700,
    AuditIntegrityError = 1701,
    AuditQueryError = 1702,

    // Category and Tag errors (1800-1899)
    InvalidTag = 1802,
    TagLimitExceeded = 1803,

    // Dispute errors (1900-1999)
    DisputeNotFound = 1900,
    DisputeAlreadyExists = 1901,
    DisputeNotAuthorized = 1902,
    DisputeAlreadyResolved = 1903,
    DisputeNotUnderReview = 1904,
    InvalidDisputeReason = 1905,
    InvalidDisputeEvidence = 1906,

    // Notification errors
    NotificationNotFound = 2000,
    NotificationBlocked = 2001,
}
