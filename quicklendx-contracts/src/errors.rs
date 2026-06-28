use soroban_sdk::{contracterror, symbol_short, Symbol};

/// Typed error enum for the QuickLendX contract. See docs/contracts/errors.md.
///
/// The Soroban XDR spec allows a maximum of 50 error variants per contract.
/// All 50 slots are used; new variants require replacing an existing one.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum QuickLendXError {
    // Invoice lifecycle (1000-1006)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvoiceNotFound = 1000,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvoiceNotAvailableForFunding = 1001,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvoiceAlreadyFunded = 1002,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvoiceAmountInvalid = 1003,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvoiceDueDateInvalid = 1004,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvoiceNotFunded = 1005,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvoiceAlreadyDefaulted = 1006,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvoiceFrozen = 1007,

    // Authorization (1100-1104)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    Unauthorized = 1100,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    NotBusinessOwner = 1101,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    NotInvestor = 1102,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    NotAdmin = 1103,
    /// Caller address equals the contract's own address (confused-deputy prevention).
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    SelfCallNotAllowed = 1104,

    // Input validation (1200-1205)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidAmount = 1200,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidAddress = 1201,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidCurrency = 1202,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidTimestamp = 1203,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidDescription = 1204,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    SelfTransfer = 1205,

    // Storage (1300-1301)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    StorageError = 1300,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    StorageKeyNotFound = 1301,

    // Business logic (1400-1405)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InsufficientFunds = 1400,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidStatus = 1401,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    OperationNotAllowed = 1402,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    PaymentTooLow = 1403,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    PlatformAccountNotConfigured = 1404,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidCoveragePercentage = 1405,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    MaxBidsPerInvoiceExceeded = 1406,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    MaxActiveBidsPerInvestorExceeded = 1407,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    MaxInvoicesPerBusinessExceeded = 1408,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidBidTtl = 1409,

    // Rating (1500-1503)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidRating = 1500,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    NotFunded = 1501,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    AlreadyRated = 1502,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    NotRater = 1503,

    // KYC / verification (1600-1604)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    BusinessNotVerified = 1600,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    KYCAlreadyPending = 1601,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    KYCAlreadyVerified = 1602,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    KYCNotFound = 1603,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidKYCStatus = 1604,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvestorNotVerified = 1605,
    BusinessDeleted = 1660,

    // Audit (1700-1702)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    AuditLogNotFound = 1700,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    AuditIntegrityError = 1701,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    AuditQueryError = 1702,

    // Category / tag (1800-1801)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidTag = 1800,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    TagLimitExceeded = 1801,

    // Fee configuration (1850-1855)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidFeeConfiguration = 1850,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    TreasuryNotConfigured = 1851,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidFeeBasisPoints = 1852,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    ArithmeticOverflow = 1856,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    RotationAlreadyPending = 1853,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    RotationNotFound = 1854,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    RotationExpired = 1855,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    RotationTimelockNotElapsed = 1857,

    // Dispute (1900-1906)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    DisputeNotFound = 1900,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    DisputeAlreadyExists = 1901,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    DisputeNotAuthorized = 1902,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    DisputeAlreadyResolved = 1903,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    DisputeNotUnderReview = 1904,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidDisputeReason = 1905,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    InvalidDisputeEvidence = 1906,

    // Notification (2000-2002)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    NotificationNotFound = 2000,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    NotificationBlocked = 2001,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    NotificationDuplicate = 2002,

    // Emergency withdraw (2100-2106)
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    ContractPaused = 2100,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    EmergencyWithdrawNotFound = 2101,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    EmergencyWithdrawTimelockNotElapsed = 2102,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    EmergencyWithdrawExpired = 2103,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    EmergencyWithdrawCancelled = 2104,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    EmergencyWithdrawAlreadyExists = 2105,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    EmergencyWithdrawInsufficientBalance = 2106,

    /// BREAKING: Do not renumber this variant. public ABI consumption.
    TokenTransferFailed = 2200,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    MaintenanceModeActive = 2201,
    /// BREAKING: Do not renumber this variant. public ABI consumption.
    DuplicateDefaultTransition = 2202,
    BackupVersionUnsupported = 2203,
    DuplicateBid = 2204,
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
            QuickLendXError::SelfCallNotAllowed => symbol_short!("SELF_NA"),
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
            QuickLendXError::InvestorNotVerified => symbol_short!("INV_NV"),
            QuickLendXError::BusinessDeleted => symbol_short!("BUS_DEL"),
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
            QuickLendXError::RotationAlreadyPending => symbol_short!("ROT_PND"),
            QuickLendXError::RotationNotFound => symbol_short!("ROT_NF"),
            QuickLendXError::RotationExpired => symbol_short!("ROT_EXP"),
            QuickLendXError::RotationTimelockNotElapsed => symbol_short!("ROT_TLK"),
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
            QuickLendXError::NotificationDuplicate => symbol_short!("NOT_DUP"),
            QuickLendXError::MaxBidsPerInvoiceExceeded => symbol_short!("MAX_BIDS"),
            QuickLendXError::MaxActiveBidsPerInvestorExceeded => symbol_short!("MAX_ACT"),
            QuickLendXError::MaxInvoicesPerBusinessExceeded => symbol_short!("MAX_INV"),
            QuickLendXError::InvalidBidTtl => symbol_short!("INV_TTL"),
            QuickLendXError::ContractPaused => symbol_short!("PAUSED"),
            QuickLendXError::EmergencyWithdrawNotFound => symbol_short!("EMG_NF"),
            QuickLendXError::EmergencyWithdrawTimelockNotElapsed => symbol_short!("EMG_TLK"),
            QuickLendXError::EmergencyWithdrawExpired => symbol_short!("EMG_EXP"),
            QuickLendXError::EmergencyWithdrawCancelled => symbol_short!("EMG_CNL"),
            QuickLendXError::EmergencyWithdrawAlreadyExists => symbol_short!("EMG_EX"),
            QuickLendXError::EmergencyWithdrawInsufficientBalance => symbol_short!("EMG_BAL"),
            QuickLendXError::TokenTransferFailed => symbol_short!("TKN_FAIL"),
            QuickLendXError::MaintenanceModeActive => symbol_short!("MAINT"),
            QuickLendXError::ArithmeticOverflow => symbol_short!("ARITH_OF"),
            QuickLendXError::DuplicateDefaultTransition => symbol_short!("DEF_DUP"),
            QuickLendXError::BackupVersionUnsupported => symbol_short!("BKP_VER"),
            QuickLendXError::InvoiceFrozen => symbol_short!("INV_FRZ"),
            QuickLendXError::SelfTransfer => symbol_short!("SELF_TR"),
            QuickLendXError::DuplicateBid => symbol_short!("BID_DUP"),
        }
    }
}
