use crate::errors::QuickLendXError;
use crate::events::{emit_insurance_claimed, emit_invoice_defaulted, emit_invoice_expired};
use crate::init::ProtocolInitializer;
use crate::investment::{InvestmentStatus, InvestmentStorage};
use crate::invoice::{InvoiceStatus, InvoiceStorage};
use soroban_sdk::{contracttype, symbol_short, BytesN, Env, Vec};

/// Default grace period in seconds (7 days)
pub const DEFAULT_GRACE_PERIOD: u64 = 7 * 24 * 60 * 60;
/// Default number of funded invoices processed per overdue scan call.
pub const DEFAULT_OVERDUE_SCAN_BATCH_LIMIT: u32 = 25;
/// Hard cap for caller-provided overdue scan limits.
pub const MAX_OVERDUE_SCAN_BATCH_LIMIT: u32 = 100;

const OVERDUE_SCAN_CURSOR_KEY: soroban_sdk::Symbol = symbol_short!("ovd_scan");

/// Result metadata returned by the bounded overdue invoice scanner.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverdueScanResult {
    pub overdue_count: u32,
    pub scanned_count: u32,
    pub total_funded: u32,
    pub next_cursor: u32,
}

/// Maximum allowed grace period in seconds (30 days)
/// This prevents excessively long grace periods that could lock funds indefinitely
const MAX_GRACE_PERIOD: u64 = 30 * 24 * 60 * 60;

/// Resolve grace period using per-call override, protocol config, or default.
///
/// # Fallback Resolution Order
/// 1. If `grace_period` is provided and valid → use it (after validation)
/// 2. If `grace_period` is None → try protocol config
/// 3. If protocol config not available → use hardcoded DEFAULT_GRACE_PERIOD
///
/// # Validation Rules
/// - Override values must be <= MAX_GRACE_PERIOD (30 days)
/// - Invalid overrides are rejected with QuickLendXError::InvalidTimestamp
/// - Zero grace period is allowed (immediate default after due date)
///
/// # Security Considerations
/// - Prevents denial-of-service via extremely large grace periods
/// - Ensures deterministic behavior across all code paths
/// - Maintains consistency with protocol-limits configuration
///
/// # Arguments
/// * `env` - The Soroban environment
/// * `grace_period` - Optional grace period override in seconds
///
/// # Returns
/// * `Ok(u64)` - Resolved grace period value
/// * `Err(QuickLendXError::InvalidTimestamp)` - If override exceeds maximum allowed value
pub fn resolve_grace_period(env: &Env, grace_period: Option<u64>) -> Result<u64, QuickLendXError> {
    match grace_period {
        Some(value) => {
            if value > MAX_GRACE_PERIOD {
                return Err(QuickLendXError::InvalidTimestamp);
            }
            Ok(value)
        }
        None => Ok(ProtocolInitializer::get_protocol_config(env)
            .map(|config| config.grace_period_seconds)
            .unwrap_or(DEFAULT_GRACE_PERIOD)),
    }
}

/// @notice Marks a funded invoice as defaulted after its grace window has strictly elapsed.
/// @dev Defaulting is allowed only when `ledger.timestamp() > due_date + resolved_grace_period`.
/// Calls using a timestamp equal to the grace deadline must fail to avoid early liquidation.
/// Grace resolution order is: explicit override, protocol config, then `DEFAULT_GRACE_PERIOD`.
///
/// # Arguments
/// * `env` - The environment
/// * `invoice_id` - The invoice ID to mark as defaulted
/// * `grace_period` - Optional grace period in seconds. If `None`, uses protocol config or
///   `DEFAULT_GRACE_PERIOD` when not configured.
///
/// # Returns
/// * `Ok(())` if the invoice was successfully marked as defaulted
/// * `Err(QuickLendXError)` if the operation fails
pub fn mark_invoice_defaulted(
    env: &Env,
    invoice_id: &BytesN<32>,
    grace_period: Option<u64>,
) -> Result<(), QuickLendXError> {
    let invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.status == InvoiceStatus::Defaulted {
        return Err(QuickLendXError::InvoiceAlreadyDefaulted);
    }

    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvoiceNotAvailableForFunding);
    }

    let current_timestamp = env.ledger().timestamp();
    let grace = resolve_grace_period(env, grace_period)?;
    let grace_deadline = invoice.grace_deadline(grace);

    if current_timestamp <= grace_deadline {
        return Err(QuickLendXError::OperationNotAllowed);
    }

    handle_default(env, invoice_id)
}

/// @notice Returns the funded-invoice scan cursor used by bounded overdue scans.
/// @dev The cursor is normalized against the current funded-invoice count before use.
/// @param env The contract environment.
/// @return Zero-based index of the next funded invoice to inspect.
pub fn get_overdue_scan_cursor(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&OVERDUE_SCAN_CURSOR_KEY)
        .unwrap_or(0)
}

/// @notice Returns the batch size used when callers do not provide an explicit scan limit.
/// @return Default funded-invoice batch size for overdue scanning.
pub fn default_overdue_scan_batch_limit() -> u32 {
    DEFAULT_OVERDUE_SCAN_BATCH_LIMIT
}

/// @notice Returns the maximum funded-invoice batch size accepted by bounded overdue scans.
/// @return Hard cap applied to caller-provided scan limits.
pub fn max_overdue_scan_batch_limit() -> u32 {
    MAX_OVERDUE_SCAN_BATCH_LIMIT
}

fn set_overdue_scan_cursor(env: &Env, cursor: u32) {
    env.storage()
        .instance()
        .set(&OVERDUE_SCAN_CURSOR_KEY, &cursor);
}

fn normalize_cursor(cursor: u32, funded_count: u32) -> u32 {
    if funded_count == 0 || cursor >= funded_count {
        0
    } else {
        cursor
    }
}

fn resolve_scan_limit(limit: Option<u32>) -> u32 {
    limit
        .unwrap_or(DEFAULT_OVERDUE_SCAN_BATCH_LIMIT)
        .max(1)
        .min(MAX_OVERDUE_SCAN_BATCH_LIMIT)
}

/// @notice Scans funded invoices in a deterministic bounded window for overdue/default handling.
/// @dev Uses a rotating cursor stored in instance storage so repeated calls eventually inspect
///      the full funded set without any single call walking every invoice. The function reads a
///      snapshot of the funded index once, then processes at most `limit` entries from that snapshot.
/// @param env The contract environment.
/// @param grace_period Grace period in seconds used to determine default eligibility.
/// @param limit Optional funded-invoice batch size. Values are clamped to `1..=100`.
/// @return Scan result containing overdue count, scanned count, funded snapshot size, and next cursor.
/// @security Bounded loops protect against excessive per-call work. Callers that need full coverage
///           must invoke the scan repeatedly until `next_cursor` wraps to `0`.
pub fn scan_funded_invoice_expirations(
    env: &Env,
    grace_period: u64,
    limit: Option<u32>,
) -> Result<OverdueScanResult, QuickLendXError> {
    let funded_invoices = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Funded);
    let total_funded = funded_invoices.len();

    if total_funded == 0 {
        set_overdue_scan_cursor(env, 0);
        return Ok(OverdueScanResult {
            overdue_count: 0,
            scanned_count: 0,
            total_funded: 0,
            next_cursor: 0,
        });
    }

    let scan_limit = resolve_scan_limit(limit).min(total_funded);
    let current_timestamp = env.ledger().timestamp();
    let mut cursor = normalize_cursor(get_overdue_scan_cursor(env), total_funded);
    let mut overdue_count = 0u32;
    let mut scanned_count = 0u32;

    while scanned_count < scan_limit {
        if let Some(invoice_id) = funded_invoices.get(cursor) {
            if let Some(invoice) = InvoiceStorage::get_invoice(env, &invoice_id) {
                if invoice.is_overdue(current_timestamp) {
                    overdue_count = overdue_count.saturating_add(1);
                    let _ = crate::notifications::NotificationSystem::notify_payment_overdue(
                        env, &invoice,
                    );
                }

                if current_timestamp > invoice.grace_deadline(grace_period) {
                    let _ = invoice.check_and_handle_expiration(env, grace_period)?;
                }
            }
        }

        scanned_count = scanned_count.saturating_add(1);
        cursor = if cursor + 1 >= total_funded {
            0
        } else {
            cursor + 1
        };
    }

    let next_cursor = if scan_limit >= total_funded {
        0
    } else {
        cursor
    };
    set_overdue_scan_cursor(env, next_cursor);

    Ok(OverdueScanResult {
        overdue_count,
        scanned_count,
        total_funded,
        next_cursor,
    })
}

/// @notice Applies the default transition after all time and status checks have passed.
/// @dev This helper does not re-check the grace-period cutoff and must only be reached from
/// validated call sites such as `mark_invoice_defaulted` or `check_and_handle_expiration`.
pub fn handle_default(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    if invoice.status == InvoiceStatus::Defaulted {
        return Err(QuickLendXError::InvoiceAlreadyDefaulted);
    }

    if invoice.status != InvoiceStatus::Funded {
        return Err(QuickLendXError::InvalidStatus);
    }

    InvoiceStorage::remove_from_status_invoices(env, &InvoiceStatus::Funded, invoice_id);

    invoice.mark_as_defaulted();
    InvoiceStorage::update_invoice(env, &invoice);

    InvoiceStorage::add_to_status_invoices(env, &InvoiceStatus::Defaulted, invoice_id);

    emit_invoice_expired(env, &invoice);

    if let Some(mut investment) = InvestmentStorage::get_investment_by_invoice(env, invoice_id) {
        investment.status = InvestmentStatus::Defaulted;

        let claim_details = investment.process_all_insurance_claims(env);

        InvestmentStorage::update_investment(env, &investment);

        for (provider, coverage_amount) in claim_details.iter() {
            if coverage_amount > 0 {
                emit_insurance_claimed(
                    env,
                    &investment.investment_id,
                    &investment.invoice_id,
                    &provider,
                    coverage_amount,
                );
            }
        }
    }

    emit_invoice_defaulted(env, &invoice);

    Ok(())
}

/// Get all invoice IDs that have active or resolved disputes
pub fn get_invoices_with_disputes(env: &Env) -> Vec<BytesN<32>> {
    Vec::new(env)
}

/// Get details for a dispute on a specific invoice
pub fn get_dispute_details(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<Option<crate::invoice::Dispute>, QuickLendXError> {
    let _invoice =
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

    Ok(None)
}
