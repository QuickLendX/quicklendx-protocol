//! Maintenance mode: read-only switch with explicit client messaging.
//!
//! Maintenance mode is a softer alternative to a full pause. When active:
//! - All state-mutating operations MUST call `require_write_allowed` and return
//!   `MaintenanceModeActive` to the caller.
//! - Read-only queries (`get_invoice`, `get_bid`, `get_escrow_details`, etc.)
//!   remain available so clients can inspect protocol state.
//! - Admin operations that toggle maintenance mode itself are always allowed.
//!
//! # Distinction from Pause
//!
//! | Mechanism | Reads | Writes | Use case |
//! |-----------|-------|--------|----------|
//! | Maintenance mode | - | - | Planned upgrades, routine ops |
//! | Full pause | - | - | Emergency incident response |
//!
//! Both block writes, but maintenance mode stores a human-readable `reason`
//! string returned to callers so clients can display an explicit message.
//!
//! # Security Model
//!
//! - Only the current admin can enable or disable maintenance mode.
//! - The toggle itself is exempt from its own guard (admin can always exit).
//! - All transitions are recorded as contract events for observability.
//! - The reason string is bounded to 256 bytes to prevent storage abuse.
//!
//! # Invariants
//!
//! 1. `is_maintenance_mode` reflects the current flag atomically.
//! 2. `get_maintenance_reason` is `Some` iff maintenance mode is active.
//! 3. On disable, the reason string is cleared from storage.
//! 4. Only admin can call `set_maintenance_mode`.

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::storage::{extend_persistent_ttl, DataKey, InvoiceStorage};
use crate::bid::BidStorage;
use crate::investment::InvestmentStorage;
use crate::payments::EscrowStorage;
use crate::currency::CurrencyWhitelist;
use soroban_sdk::{symbol_short, Address, Env, String, Symbol};

/// Storage key for the maintenance mode boolean flag.
const MAINTENANCE_KEY: Symbol = symbol_short!("maint");

/// Storage key for the maintenance reason string.
const MAINTENANCE_REASON_KEY: Symbol = symbol_short!("maint_rsn");

/// Maximum allowed byte length for a maintenance reason string.
pub const MAX_REASON_LEN: u32 = 256;

/// Report summarizing the results of a TTL extension operation.
#[derive(Clone, contracttype)]
pub struct ExtendReport {
    pub invoices_refreshed: u32,
    pub bids_refreshed: u32,
    pub investments_refreshed: u32,
    pub escrows_refreshed: u32,
    pub currencies_refreshed: u32,
}

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
    ///
    /// When `enabled` is `true`, `reason` is stored and emitted in the event so
    /// that clients can display explicit messaging (e.g. "Scheduled upgrade -
    /// back in 30 min"). When `enabled` is `false`, the reason is cleared.
    ///
    /// This function is intentionally exempt from `require_write_allowed` so
    /// that an admin can exit maintenance mode while writes are frozen.
    ///
    /// # Arguments
    /// * `env`     - The contract environment.
    /// * `admin`   - Caller address; must be the current admin.
    /// * `enabled` - `true` to enter maintenance, `false` to exit.
    /// * `reason`  - Human-readable message (required when `enabled`; ignored
    ///               on disable but must still be supplied by the caller).
    ///
    /// # Errors
    /// * `NotAdmin`           - caller is not the admin.
    /// * `InvalidDescription` - reason exceeds `MAX_REASON_LEN` bytes.
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
    ///
    /// Call this at the top of every write entrypoint. Returns
    /// `Err(MaintenanceModeActive)` when maintenance mode is on, so callers
    /// receive an explicit error code rather than a generic rejection.
    ///
    /// Read-only entrypoints must NOT call this - they remain available during
    /// maintenance so clients can inspect protocol state.
    ///
    /// # Errors
    /// * `MaintenanceModeActive` - protocol is in maintenance (read-only) mode.
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
    pub fn extend_protocol_ttl(env: &Env, admin: &Address) -> Result<ExtendReport, QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;

        let mut report = ExtendReport {
            invoices_refreshed: 0,
            bids_refreshed: 0,
            investments_refreshed: 0,
            escrows_refreshed: 0,
            currencies_refreshed: 0,
        };

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
            if let Some(escrow) = EscrowStorage::get_escrow_by_invoice(env, invoice_id) {
                extend_persistent_ttl(env, &escrow.escrow_id);
                report.escrows_refreshed += 1;
            }
        }

        // 5. Extend Currencies
        for currency in CurrencyWhitelist::get_whitelisted_currencies(env).iter() {
            extend_persistent_ttl(env, &currency);
            report.currencies_refreshed += 1;
        }

        // Emit events for each kind that was refreshed
        if report.invoices_refreshed > 0 {
            crate::events::emit_ttl_extended(env, &String::from_str(env, "invoice"), report.invoices_refreshed);
        }
        if report.bids_refreshed > 0 {
            crate::events::emit_ttl_extended(env, &String::from_str(env, "bid"), report.bids_refreshed);
        }
        if report.investments_refreshed > 0 {
            crate::events::emit_ttl_extended(env, &String::from_str(env, "investment"), report.investments_refreshed);
        }
        if report.escrows_refreshed > 0 {
            crate::events::emit_ttl_extended(env, &String::from_str(env, "escrow"), report.escrows_refreshed);
        }
        if report.currencies_refreshed > 0 {
            crate::events::emit_ttl_extended(env, &String::from_str(env, "currency"), report.currencies_refreshed);
        }

        Ok(report)
    }
}

        if report.bids_refreshed > 0 {
            env.events().publish(
                (symbol_short!("MAINT"), symbol_short!("ttl_ext")),
                (String::from_str(env, "bid"), report.bids_refreshed.into()),
            );
        }
        if report.investments_refreshed > 0 {
            env.events().publish(
                (symbol_short!("MAINT"), symbol_short!("ttl_ext")),
                (String::from_str(env, "investment"), report.investments_refreshed.into()),
            );
        }
        if report.escrows_refreshed > 0 {
            env.events().publish(
                (symbol_short!("MAINT"), symbol_short!("ttl_ext")),
                (String::from_str(env, "escrow"), report.escrows_refreshed.into()),
            );
        }
        if report.currencies_refreshed > 0 {
            env.events().publish(
                (symbol_short!("MAINT"), symbol_short!("ttl_ext")),
                (String::from_str(env, "currency"), report.currencies_refreshed.into()),
            );
        }

        Ok(report)
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
    pub fn extend_protocol_ttl(env: &Env, admin: &Address) -> Result<ExtendReport, QuickLendXError> {
        AdminStorage::require_admin(env, admin)?;

        let mut report = ExtendReport {
            invoices_refreshed: 0,
            bids_refreshed: 0,
            investments_refreshed: 0,
            escrows_refreshed: 0,
            currencies_refreshed: 0,
        };

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
            if let Some(escrow) = EscrowStorage::get_escrow_by_invoice(env, invoice_id) {
                extend_persistent_ttl(env, &escrow.escrow_id);
                report.escrows_refreshed += 1;
            }
        }

        // 5. Extend Currencies
        for currency in CurrencyWhitelist::get_whitelisted_currencies(env).iter() {
            extend_persistent_ttl(env, &currency);
            report.currencies_refreshed += 1;
        }

        // Emit event for each kind to facilitate granular monitoring
        if report.invoices_refreshed > 0 {
            env.events().publish(
                (symbol_short!("MAINT"), symbol_short!("ttl_ext")),
                (String::from_str(env, "invoice"), report.invoices_refheshed.into()),
            );
        }
        // ... actually I'll just emit one event with the whole report if possible, 
        // but TtlExtended event is defined as (String, u32).
        // Let's just emit one event for the whole operation.
        
        // Actually, the requirement says "emits TtlExtended event for off-chain monitoring."
        // It doesn't specify how many. Emitting one for each kind is more flexible.
        // But I already added TtlExtended as (String, u32).

        // Let's go with one per kind.
        
        Ok(report)
    }
}
