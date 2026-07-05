//! Maintenance mode: read-only switch with explicit client messaging.
//!
//! Maintenance mode is a softer alternative to a full pause. When active:
//! - All state-mutating operations MUST call `require_write_allowed` and return
//!   `MaintenanceModeActive` to the caller.
//! - Read-only queries remain available so clients can inspect protocol state.
//! - Admin operations that toggle maintenance mode itself are always allowed.

use crate::admin::AdminStorage;
use crate::audit::AuditStorage;
use crate::bid::BidStorage;
use crate::currency::CurrencyWhitelist;
use crate::errors::QuickLendXError;
use crate::investment::InvestmentStorage;
use crate::payments::EscrowStorage;
use crate::settlement;
use crate::storage::{extend_persistent_ttl, DataKey, InvoiceStorage};
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, String, Symbol};

/// Storage key for the maintenance mode boolean flag.
pub const MAINTENANCE_MODE_KEY: Symbol = symbol_short!("maint");

/// Storage key for the maintenance reason string.
pub const MAINTENANCE_REASON_KEY: Symbol = symbol_short!("maint_rsn");

/// Maximum allowed byte length for a maintenance reason string.
pub const MAX_REASON_LEN: u32 = 256;

/// Report summarizing the results of a TTL extension operation.
///
/// Returned by [`MaintenanceControl::extend_protocol_ttl`]. Each field counts the
/// number of persistent-storage entries whose TTL was refreshed for that kind.
/// A zero value means no entries of that kind existed (idempotent no-op).
///
/// # Fields
/// * `invoices_refreshed`  — Number of invoice records extended.
/// * `bids_refreshed`      — Number of bid records extended.
/// * `investments_refreshed` — Number of active investment records extended.
/// * `escrows_refreshed`   — Number of escrow records extended.
/// * `currencies_refreshed` — Number of whitelisted currency entries extended.
///
/// The targeted invoice path also fills payment and audit counts when those
/// record sets exist.
///
/// # Idempotency
/// Calling `extend_protocol_ttl` multiple times within the same ledger is safe:
/// `extend_ttl` is itself idempotent, and the report always reflects the current
/// state at call time. If no new entries were added between calls the report
/// will be identical.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ExtendReport {
    /// Number of invoice records whose TTL was extended.
    pub invoices_refreshed: u32,
    /// Number of bid records whose TTL was extended.
    pub bids_refreshed: u32,
    /// Number of active investment records whose TTL was extended.
    pub investments_refreshed: u32,
    /// Number of escrow records whose TTL was extended.
    pub escrows_refreshed: u32,
    /// Number of payment storage keys whose TTL was extended.
    pub payments_refreshed: u32,
    /// Number of audit storage keys covered by an instance TTL extension.
    pub audit_refreshed: u32,
    /// Number of whitelisted currency entries whose TTL was extended.
    pub currencies_refreshed: u32,
}

pub struct MaintenanceControl;

impl MaintenanceControl {
    fn empty_report() -> ExtendReport {
        ExtendReport {
            invoices_refreshed: 0,
            bids_refreshed: 0,
            investments_refreshed: 0,
            escrows_refreshed: 0,
            payments_refreshed: 0,
            audit_refreshed: 0,
            currencies_refreshed: 0,
        }
    }

    fn emit_ttl_event(env: &Env, kind: &str, count: u32) {
        if count > 0 {
            crate::events::emit_ttl_extended(env, &String::from_str(env, kind), count);
        }
    }

    fn emit_extend_report_events(env: &Env, report: &ExtendReport) {
        Self::emit_ttl_event(env, "invoice", report.invoices_refreshed);
        Self::emit_ttl_event(env, "bid", report.bids_refreshed);
        Self::emit_ttl_event(env, "investment", report.investments_refreshed);
        Self::emit_ttl_event(env, "escrow", report.escrows_refreshed);
        Self::emit_ttl_event(env, "payment", report.payments_refreshed);
        Self::emit_ttl_event(env, "audit", report.audit_refreshed);
        Self::emit_ttl_event(env, "currency", report.currencies_refreshed);
    }

    /// Return `true` if the protocol is currently in maintenance mode.
    pub fn is_maintenance_mode(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&MAINTENANCE_MODE_KEY)
            .unwrap_or(false)
    }

    /// Return the maintenance reason string, or `None` if not in maintenance.
    pub fn get_maintenance_reason(env: &Env) -> Option<String> {
        env.storage().instance().get(&MAINTENANCE_REASON_KEY)
    }

    /// Enable or disable maintenance mode (admin only).
    pub fn set_maintenance_mode(
        env: &Env,
        admin: &Address,
        enabled: bool,
        reason: &String,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;
        Self::apply_maintenance_mode(env, enabled, reason, admin)
    }

    /// Write maintenance flags without re-checking admin (caller must authorize).
    pub(crate) fn apply_maintenance_mode(
        env: &Env,
        enabled: bool,
        reason: &String,
        actor: &Address,
    ) -> Result<(), QuickLendXError> {
        if enabled && reason.len() > MAX_REASON_LEN {
            return Err(QuickLendXError::InvalidDescription);
        }

        env.storage()
            .instance()
            .set(&MAINTENANCE_MODE_KEY, &enabled);

        if enabled {
            env.storage()
                .instance()
                .set(&MAINTENANCE_REASON_KEY, reason);
            env.events().publish(
                (symbol_short!("MAINT"), symbol_short!("enabled")),
                reason.clone(),
            );
        } else {
            env.storage().instance().remove(&MAINTENANCE_REASON_KEY);
            env.events().publish(
                (symbol_short!("MAINT"), symbol_short!("disabled")),
                actor.clone(),
            );
        }

        Ok(())
    }

    /// Guard for state-mutating operations.
    pub fn require_write_allowed(env: &Env) -> Result<(), QuickLendXError> {
        if Self::is_maintenance_mode(env) {
            Err(QuickLendXError::MaintenanceModeActive)
        } else {
            Ok(())
        }
    }

    /// Admin-only: extends the TTL for all major persistent storage indexes.
    ///
    /// This iterates through invoices, bids, active investments, escrows (via invoices),
    /// and the currency whitelist, and extends the TTL for each entry.
    ///
    /// # Arguments
    /// * `env`     - The contract environment.
    /// * `admin`   - Caller address; must be the current admin.
    ///
    /// # Returns
    /// * `ExtendReport` - A summary of how many entries were refreshed per kind.
    ///
    /// # Errors
    /// * `NotAdmin` - caller is not the admin.
    pub fn extend_protocol_ttl(
        env: &Env,
        admin: &Address,
    ) -> Result<ExtendReport, QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;

        let mut report = Self::empty_report();

        // 1. Extend Invoices
        for invoice_id in InvoiceStorage::get_all_invoice_ids(env).iter() {
            extend_persistent_ttl(env, &DataKey::Invoice(invoice_id.clone()));
            report.invoices_refreshed += 1;
        }

        // 2. Extend Bids
        for bid_id in BidStorage::get_all_bids(env).iter() {
            extend_persistent_ttl(env, &bid_id);
            report.bids_refreshed += 1;
        }

        // 3. Extend Active Investments
        for investment_id in InvestmentStorage::get_active_investment_ids(env).iter() {
            extend_persistent_ttl(env, &investment_id);
            report.investments_refreshed += 1;
        }

        // 4. Extend Escrows (find them via invoices)
        for invoice_id in InvoiceStorage::get_all_invoice_ids(env).iter() {
            if let Some(escrow) = EscrowStorage::get_escrow_by_invoice(env, &invoice_id) {
                extend_persistent_ttl(env, &escrow.escrow_id);
                report.escrows_refreshed += 1;
            }
        }

        // 5. Extend Currencies
        for currency in CurrencyWhitelist::get_whitelisted_currencies(env).iter() {
            extend_persistent_ttl(env, &currency);
            report.currencies_refreshed += 1;
        }

        Self::emit_extend_report_events(env, &report);

        Ok(report)
    }

    /// Admin-only: extends TTL for one invoice and its associated storage keys.
    pub fn extend_invoice_ttl(
        env: &Env,
        admin: &Address,
        invoice_id: &BytesN<32>,
    ) -> Result<ExtendReport, QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;
        InvoiceStorage::get_invoice(env, invoice_id).ok_or(QuickLendXError::InvoiceNotFound)?;

        let mut report = Self::empty_report();

        extend_persistent_ttl(env, &DataKey::Invoice(invoice_id.clone()));
        report.invoices_refreshed = 1;

        report.bids_refreshed = BidStorage::extend_invoice_bid_ttl(env, invoice_id);
        report.investments_refreshed =
            InvestmentStorage::extend_invoice_investment_ttl(env, invoice_id);
        report.escrows_refreshed = EscrowStorage::extend_invoice_escrow_ttl(env, invoice_id);
        report.payments_refreshed = settlement::extend_invoice_payment_ttl(env, invoice_id);
        report.audit_refreshed = AuditStorage::extend_invoice_audit_ttl(env, invoice_id);

        Self::emit_extend_report_events(env, &report);

        Ok(report)
    }
}
