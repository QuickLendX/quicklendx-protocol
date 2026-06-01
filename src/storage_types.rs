/// Shared storage keys and configuration types for the QuickLendX protocol.
use soroban_sdk::contracttype;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// Enumeration of all on-chain storage keys used by the contract.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataKey {
    /// The current admin [`Address`].
    Admin,
    /// Whether the contract has been initialized.
    Initialized,
    /// The protocol configuration record.
    ProtocolConfig,
    /// The fee configuration record.
    FeeConfig,
}

// ---------------------------------------------------------------------------
// Configuration types
// ---------------------------------------------------------------------------

/// Protocol-level configuration parameters.
///
/// Stored under [`DataKey::ProtocolConfig`].
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolConfig {
    /// Minimum invoice amount in the protocol's base unit (must be > 0).
    pub min_invoice_amount: i128,
    /// Maximum number of days until an invoice is due (1–730).
    pub max_due_date_days: u32,
    /// Grace period in seconds after the due date before default is triggered
    /// (0–2_592_000, i.e. up to 30 days).
    pub grace_period_seconds: u64,
}

/// Fee configuration parameters.
///
/// Stored under [`DataKey::FeeConfig`].
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeeConfig {
    /// Protocol fee in basis points (0–1000, i.e. 0 %–10 %).
    pub fee_bps: u32,
    /// Treasury address that receives collected fees.
    pub treasury: soroban_sdk::Address,
}
