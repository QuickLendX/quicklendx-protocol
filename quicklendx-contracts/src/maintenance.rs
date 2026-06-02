//! Maintenance mode: read-only switch with explicit client messaging.
//!
//! Maintenance mode is a softer alternative to a full pause. When active:
//! - All state-mutating operations MUST call `require_write_allowed` and return
//!   `MaintenanceModeActive` to the caller.
//! - Read-only queries remain available so clients can inspect protocol state.
//! - Admin operations that toggle maintenance mode itself are always allowed.

use crate::admin::AdminStorage;
use crate::bid::BidStorage;
use crate::currency::CurrencyWhitelist;
use crate::errors::QuickLendXError;
use crate::investment::InvestmentStorage;
use crate::payments::EscrowStorage;
use crate::storage::{extend_persistent_ttl, DataKey, InvoiceStorage};
use soroban_sdk::{symbol_short, Address, Env, String, Symbol, contracttype};

/// Storage key for the maintenance mode boolean flag.
const MAINTENANCE_KEY: Symbol = symbol_short!("maint");

/// Storage key for the maintenance reason string.
const MAINTENANCE_REASON_KEY: Symbol = symbol_short!("maint_rsn");

/// Maximum allowed byte length for a maintenance reason string.
pub const MAX_REASON_LEN: u32 = 256;

/// Report summarizing the results of a TTL extension operation.
#[contracttype]
#[derive(Clone)]
pub struct ExtendReport {
    pub invoices_refreshed: u32,
    pub bids_refreshed: u32,
    pub investments_refreshed: u32,
    pub escrows_refreshed: u32,
    pub currencies_refreshed: u32,
}

pub struct MaintenanceControl;

impl MaintenanceControl {
    /// Return `true` if the protocol is currently in maintenance mode.
    pub fn is_maintenance_mode(env: &Env) -> bool {
        env.storage()
            .instance()
            .get(&MAINTENANCE_KEY)
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

        if enabled && reason.len() > MAX_REASON_LEN {
            return Err(QuickLendXError::InvalidDescription);
        }

        env.storage().instance().set(&MAINTENANCE_KEY, &enabled);

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
                admin.clone(),
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
    pub fn extend_protocol_ttl(
        env: &Env,
        admin: &Address,
    ) -> Result<ExtendReport, QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;

        let mut report = ExtendReport {
            invoices_refreshed: 0,
            bids_refreshed: 0,
            investments_refreshed: 0,
            escrows_refreshed: 0,
            currencies_refreshed: 0,
        };

        for invoice_id in InvoiceStorage::get_all_invoice_ids(env).iter() {
            extend_persistent_ttl(env, &DataKey::Invoice(invoice_id.clone()));
            report.invoices_refreshed += 1;
        }

        for bid_id in BidStorage::get_all_bids(env).iter() {
            extend_persistent_ttl(env, &bid_id);
            report.bids_refreshed += 1;
        }

        for investment_id in InvestmentStorage::get_active_investment_ids(env).iter() {
            extend_persistent_ttl(env, &investment_id);
            report.investments_refreshed += 1;
        }

        for invoice_id in InvoiceStorage::get_all_invoice_ids(env).iter() {
            if let Some(escrow) = EscrowStorage::get_escrow_by_invoice(env, &invoice_id) {
                extend_persistent_ttl(env, &escrow.escrow_id);
                report.escrows_refreshed += 1;
            }
        }

        for currency in CurrencyWhitelist::get_whitelisted_currencies(env).iter() {
            extend_persistent_ttl(env, &currency);
            report.currencies_refreshed += 1;
        }

        Self::emit_ttl_extended(env, "invoice", report.invoices_refreshed);
        Self::emit_ttl_extended(env, "bid", report.bids_refreshed);
        Self::emit_ttl_extended(env, "investment", report.investments_refreshed);
        Self::emit_ttl_extended(env, "escrow", report.escrows_refreshed);
        Self::emit_ttl_extended(env, "currency", report.currencies_refreshed);
    }
    
    fn emit_ttl_extended(env: &Env, entity_type: &str, count: u32) {
        env.events().publish(
            (Symbol::new(env, "ttl_extended"),),
            (String::from_str(env, entity_type), count)
        );

        Ok(report)
    }
}