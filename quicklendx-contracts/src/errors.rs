use soroban_sdk::{contracterror, symbol_short, Symbol};

/// Typed error enum for the QuickLendX contract. See docs/contracts/errors.md.
///
/// The Soroban XDR spec allows a maximum of 50 error variants per contract.
/// All 50 slots are used; new variants require replacing an existing one.
#[contracterror]
#[derive(Copy, Clone, Eq, PartialEq, PartialOrd, Ord)]
#[cfg_attr(test, derive(Debug))]
#[repr(u32)]
pub enum QuickLendXError {
    // Invoice lifecycle (1000–1006)
    InvoiceNotFound = 1000,
    InvoiceNotAvailableForFunding = 1001,
    InvoiceAlreadyFunded = 1002,
    InvoiceAmountInvalid = 1003,
    InvoiceDueDateInvalid = 1004,
    InvoiceNotFunded = 1005,
    InvoiceAlreadyDefaulted = 1006,

    // Authorization (1100–1103)
    Unauthorized = 1100,
    NotBusinessOwner = 1101,
    NotInvestor = 1102,
    NotAdmin = 1103,

    // Input validation (1200–1204)
    InvalidAmount = 1200,
    InvalidAddress = 1201,
    InvalidCurrency = 1202,
    InvalidTimestamp = 1203,
    InvalidDescription = 1204,

    // Storage (1300–1301)
    StorageError = 1300,
    StorageKeyNotFound = 1301,

    // Business logic (1400–1405)
    InsufficientFunds = 1400,
    InvalidStatus = 1401,
    OperationNotAllowed = 1402,
    PaymentTooLow = 1403,
    PlatformAccountNotConfigured = 1404,
    InvalidCoveragePercentage = 1405,
    MaxBidsPerInvoiceExceeded = 1406,

    // Rating (1500–1503)
    InvalidRating = 1500,
    NotFunded = 1501,
    AlreadyRated = 1502,
    NotRater = 1503,

    // KYC / verification (1600–1604)
    BusinessNotVerified = 1600,
    KYCAlreadyPending = 1601,
    KYCAlreadyVerified = 1602,
    KYCNotFound = 1603,
    InvalidKYCStatus = 1604,

    // Audit (1700–1702)
    AuditLogNotFound = 1700,
    AuditIntegrityError = 1701,
    AuditQueryError = 1702,

    // Category / tag (1800–1801)
    InvalidTag = 1800,
    TagLimitExceeded = 1801,

    // Fee configuration (1850–1852)
    InvalidFeeConfiguration = 1850,
    TreasuryNotConfigured = 1851,
    InvalidFeeBasisPoints = 1852,

    // Dispute (1900–1906)
    DisputeNotFound = 1900,
    DisputeAlreadyExists = 1901,
    DisputeNotAuthorized = 1902,
    DisputeAlreadyResolved = 1903,
    DisputeNotUnderReview = 1904,
    InvalidDisputeReason = 1905,
    InvalidDisputeEvidence = 1906,

    // Notification (2000–2001)
    NotificationNotFound = 2000,
    NotificationBlocked = 2001,

    // Emergency (2100)
    ContractPaused = 2100,
}

impl From<QuickLendXError> for Symbol {
    fn from(error: QuickLendXError) -> Self {
        match error {
            // Invoice lifecycle
            QuickLendXError::InvoiceNotFound => symbol_short!("INV_NF"),
            QuickLendXError::InvoiceNotAvailableForFunding => symbol_short!("INV_NAF"),
            QuickLendXError::InvoiceAlreadyFunded => symbol_short!("INV_AF"),
            QuickLendXError::InvoiceAmountInvalid => symbol_short!("INV_AI"),
            QuickLendXError::InvoiceDueDateInvalid => symbol_short!("INV_DI"),
            QuickLendXError::InvoiceNotFunded => symbol_short!("INV_NFD"),
            QuickLendXError::InvoiceAlreadyDefaulted => symbol_short!("INV_AD"),
            // Authorization
            QuickLendXError::Unauthorized => symbol_short!("UNAUTH"),
            QuickLendXError::NotBusinessOwner => symbol_short!("NOT_OWN"),
            QuickLendXError::NotInvestor => symbol_short!("NOT_INV"),
            QuickLendXError::NotAdmin => symbol_short!("NOT_ADM"),
            // Input validation
            QuickLendXError::InvalidAmount => symbol_short!("INV_AMT"),
            QuickLendXError::InvalidAddress => symbol_short!("INV_ADR"),
            QuickLendXError::InvalidCurrency => symbol_short!("INV_CR"),
            QuickLendXError::InvalidTimestamp => symbol_short!("INV_TM"),
            QuickLendXError::InvalidDescription => symbol_short!("INV_DS"),
            // Storage
            QuickLendXError::StorageError => symbol_short!("STORE"),
            QuickLendXError::StorageKeyNotFound => symbol_short!("KEY_NF"),
            // Business logic
            QuickLendXError::InsufficientFunds => symbol_short!("INSUF"),
            QuickLendXError::InvalidStatus => symbol_short!("INV_ST"),
            QuickLendXError::OperationNotAllowed => symbol_short!("OP_NA"),
            QuickLendXError::PaymentTooLow => symbol_short!("PAY_LOW"),
            QuickLendXError::PlatformAccountNotConfigured => symbol_short!("PLT_NC"),
            QuickLendXError::InvalidCoveragePercentage => symbol_short!("INS_CV"),
            // Rating
            QuickLendXError::InvalidRating => symbol_short!("INV_RT"),
            QuickLendXError::NotFunded => symbol_short!("NOT_FD"),
            QuickLendXError::AlreadyRated => symbol_short!("ALR_RT"),
            QuickLendXError::NotRater => symbol_short!("NOT_RT"),
            // KYC / verification
            QuickLendXError::BusinessNotVerified => symbol_short!("BUS_NV"),
            QuickLendXError::KYCAlreadyPending => symbol_short!("KYC_PD"),
            QuickLendXError::KYCAlreadyVerified => symbol_short!("KYC_VF"),
            QuickLendXError::KYCNotFound => symbol_short!("KYC_NF"),
            QuickLendXError::InvalidKYCStatus => symbol_short!("KYC_IS"),
            // Audit
            QuickLendXError::AuditLogNotFound => symbol_short!("AUD_NF"),
            QuickLendXError::AuditIntegrityError => symbol_short!("AUD_IE"),
            QuickLendXError::AuditQueryError => symbol_short!("AUD_QE"),
            // Category / tag
            QuickLendXError::InvalidTag => symbol_short!("INV_TAG"),
            QuickLendXError::TagLimitExceeded => symbol_short!("TAG_LIM"),
            // Fee configuration
            QuickLendXError::InvalidFeeConfiguration => symbol_short!("FEE_CFG"),
            QuickLendXError::TreasuryNotConfigured => symbol_short!("TRS_NC"),
            QuickLendXError::InvalidFeeBasisPoints => symbol_short!("FEE_BPS"),
            // Dispute
            QuickLendXError::DisputeNotFound => symbol_short!("DSP_NF"),
            QuickLendXError::DisputeAlreadyExists => symbol_short!("DSP_EX"),
            QuickLendXError::DisputeNotAuthorized => symbol_short!("DSP_NA"),
            QuickLendXError::DisputeAlreadyResolved => symbol_short!("DSP_RS"),
            QuickLendXError::DisputeNotUnderReview => symbol_short!("DSP_UR"),
            QuickLendXError::InvalidDisputeReason => symbol_short!("DSP_RN"),
            QuickLendXError::InvalidDisputeEvidence => symbol_short!("DSP_EV"),
            // Notification
            QuickLendXError::NotificationNotFound => symbol_short!("NOT_NF"),
            QuickLendXError::NotificationBlocked => symbol_short!("NOT_BL"),
            QuickLendXError::MaxBidsPerInvoiceExceeded => symbol_short!("MAX_BIDS"),
            QuickLendXError::ContractPaused => symbol_short!("PAUSED"),
        }
    }
}
