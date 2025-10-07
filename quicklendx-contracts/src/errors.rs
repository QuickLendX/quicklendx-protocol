use soroban_sdk::{contracterror, symbol_short, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum QuickLendXError {
    NotFound = 1000,
    AlreadyExists = 1001,
    Unauthorized = 1002,
    InvalidAmount = 1003,
    InvalidStatus = 1004,
    InsufficientFunds = 1005,
    StorageError = 1006,
    OperationNotAllowed = 1007,
    
    InvoiceNotFound = 2000,
    InvoiceAlreadyExists = 2001,
    InvoiceNotAvailable = 2002,
    InvoiceAlreadyFunded = 2003,
    InvoiceAmountInvalid = 2004,
    InvoiceDueDateInvalid = 2005,
    InvoiceNotVerified = 2006,
    InvoiceNotFunded = 2007,
    InvoiceAlreadyPaid = 2008,
    InvoiceAlreadyDefaulted = 2009,
    
    NotBusinessOwner = 3000,
    NotInvestor = 3001,
    NotAdmin = 3002,
    
    InvalidAddress = 4000,
    InvalidCurrency = 4001,
    InvalidTimestamp = 4002,
    InvalidDescription = 4003,
    StorageKeyNotFound = 4004,
    
    PaymentTooLow = 5000,
    PlatformNotConfigured = 5001,
    InvalidCoveragePercentage = 5002,
    
    InvalidRating = 6000,
    NotFunded = 6001,
    AlreadyRated = 6002,
    NotRater = 6003,
    
    BusinessNotVerified = 7000,
    KYCAlreadyPending = 7001,
    KYCAlreadyVerified = 7002,
    KYCNotFound = 7003,
    InvalidKYCStatus = 7004,
    
    AuditLogNotFound = 8000,
    AuditIntegrityError = 8001,
    AuditQueryError = 8002,
    
    InvalidTag = 9000,
    TagLimitExceeded = 9001,
    
    DisputeNotFound = 10000,
    DisputeAlreadyExists = 10001,
    DisputeNotAuthorized = 10002,
    DisputeAlreadyResolved = 10003,
    DisputeNotUnderReview = 10004,
    InvalidDisputeReason = 10005,
    InvalidDisputeEvidence = 10006,
    
    NotificationNotFound = 11000,
    NotificationBlocked = 11001,
    
    InvalidPaymentEvent = 12000,
    PaymentAlreadyProcessed = 12001,
    SettlementQueueFull = 12002,
    SettlementRetryLimit = 12003,
    InvalidPaymentSource = 12004,
    PaymentValidationFailed = 12005,
}

impl From<QuickLendXError> for Symbol {
    fn from(error: QuickLendXError) -> Self {
        match error {
            QuickLendXError::NotFound => symbol_short!("NOT_FOUND"),
            QuickLendXError::AlreadyExists => symbol_short!("ALREADY_EX"),
            QuickLendXError::Unauthorized => symbol_short!("UNAUTH"),
            QuickLendXError::InvalidAmount => symbol_short!("INV_AMT"),
            QuickLendXError::InvalidStatus => symbol_short!("INV_ST"),
            QuickLendXError::InsufficientFunds => symbol_short!("INSUF"),
            QuickLendXError::StorageError => symbol_short!("STORE"),
            QuickLendXError::OperationNotAllowed => symbol_short!("OP_NA"),
            
            QuickLendXError::InvoiceNotFound => symbol_short!("INV_NF"),
            QuickLendXError::InvoiceAlreadyExists => symbol_short!("INV_EX"),
            QuickLendXError::InvoiceNotAvailable => symbol_short!("INV_NA"),
            QuickLendXError::InvoiceAlreadyFunded => symbol_short!("INV_FD"),
            QuickLendXError::InvoiceAmountInvalid => symbol_short!("INV_AI"),
            QuickLendXError::InvoiceDueDateInvalid => symbol_short!("INV_DI"),
            QuickLendXError::InvoiceNotVerified => symbol_short!("INV_NV"),
            QuickLendXError::InvoiceNotFunded => symbol_short!("INV_NF"),
            QuickLendXError::InvoiceAlreadyPaid => symbol_short!("INV_PD"),
            QuickLendXError::InvoiceAlreadyDefaulted => symbol_short!("INV_DF"),
            
            QuickLendXError::NotBusinessOwner => symbol_short!("NOT_OWN"),
            QuickLendXError::NotInvestor => symbol_short!("NOT_INV"),
            QuickLendXError::NotAdmin => symbol_short!("NOT_ADM"),
            
            QuickLendXError::InvalidAddress => symbol_short!("INV_ADR"),
            QuickLendXError::InvalidCurrency => symbol_short!("INV_CR"),
            QuickLendXError::InvalidTimestamp => symbol_short!("INV_TM"),
            QuickLendXError::InvalidDescription => symbol_short!("INV_DS"),
            QuickLendXError::StorageKeyNotFound => symbol_short!("KEY_NF"),
            
            QuickLendXError::PaymentTooLow => symbol_short!("PAY_LOW"),
            QuickLendXError::PlatformNotConfigured => symbol_short!("PLT_NC"),
            QuickLendXError::InvalidCoveragePercentage => symbol_short!("INS_CV"),
            
            QuickLendXError::InvalidRating => symbol_short!("INV_RT"),
            QuickLendXError::NotFunded => symbol_short!("NOT_FD"),
            QuickLendXError::AlreadyRated => symbol_short!("ALR_RT"),
            QuickLendXError::NotRater => symbol_short!("NOT_RT"),
            
            QuickLendXError::BusinessNotVerified => symbol_short!("BUS_NV"),
            QuickLendXError::KYCAlreadyPending => symbol_short!("KYC_PD"),
            QuickLendXError::KYCAlreadyVerified => symbol_short!("KYC_VF"),
            QuickLendXError::KYCNotFound => symbol_short!("KYC_NF"),
            QuickLendXError::InvalidKYCStatus => symbol_short!("KYC_IS"),
            
            QuickLendXError::AuditLogNotFound => symbol_short!("AUD_NF"),
            QuickLendXError::AuditIntegrityError => symbol_short!("AUD_IE"),
            QuickLendXError::AuditQueryError => symbol_short!("AUD_QE"),
            
            QuickLendXError::InvalidTag => symbol_short!("INV_TAG"),
            QuickLendXError::TagLimitExceeded => symbol_short!("TAG_LIM"),
            
            QuickLendXError::DisputeNotFound => symbol_short!("DSP_NF"),
            QuickLendXError::DisputeAlreadyExists => symbol_short!("DSP_EX"),
            QuickLendXError::DisputeNotAuthorized => symbol_short!("DSP_NA"),
            QuickLendXError::DisputeAlreadyResolved => symbol_short!("DSP_RS"),
            QuickLendXError::DisputeNotUnderReview => symbol_short!("DSP_UR"),
            QuickLendXError::InvalidDisputeReason => symbol_short!("DSP_RN"),
            QuickLendXError::InvalidDisputeEvidence => symbol_short!("DSP_EV"),
            
            QuickLendXError::NotificationNotFound => symbol_short!("NOT_NF"),
            QuickLendXError::NotificationBlocked => symbol_short!("NOT_BL"),
            
            QuickLendXError::InvalidPaymentEvent => symbol_short!("PAY_INV"),
            QuickLendXError::PaymentAlreadyProcessed => symbol_short!("PAY_PROC"),
            QuickLendXError::SettlementQueueFull => symbol_short!("SET_Q_FUL"),
            QuickLendXError::SettlementRetryLimit => symbol_short!("SET_RETRY"),
            QuickLendXError::InvalidPaymentSource => symbol_short!("PAY_SRC"),
            QuickLendXError::PaymentValidationFailed => symbol_short!("PAY_VAL"),
        }
    }
}