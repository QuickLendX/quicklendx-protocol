//! Coordinated incident mode: atomically pauses the protocol and enters maintenance mode.
//!
//! QuickLendX has two independent circuit breakers — the hard `PauseControl::set_paused`
//! and the softer `MaintenanceControl::set_maintenance_mode`. During an incident,
//! operators need a single atomic action that engages both and returns an auditable
//! snapshot for the runbook.

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::maintenance::{MaintenanceControl, MAX_REASON_LEN};
use crate::pause::PauseControl;
use soroban_sdk::{contracttype, Address, Env, String};

/// Health snapshot returned when entering or exiting incident mode.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IncidentSnapshot {
    /// Whether the hard pause flag is set.
    pub is_paused: bool,
    /// Whether maintenance mode is active.
    pub is_maintenance: bool,
    /// Stored maintenance reason (empty when not in maintenance).
    pub reason: String,
    /// Ledger timestamp at the moment the snapshot was captured.
    pub timestamp: u64,
}

pub struct IncidentControl;

impl IncidentControl {
    /// Atomically engage incident mode: hard pause plus maintenance with reason.
    ///
    /// # Atomicity
    /// Admin credentials and `reason` length are validated before any storage
    /// write. On Soroban the entire invocation reverts when this function returns
    /// `Err`, so callers never observe a half-applied pause without maintenance
    /// (or vice versa) from a failed `enter_incident_mode`.
    ///
    /// Writes are ordered maintenance-first, then pause. Both flags commit
    /// together on success.
    ///
    /// # Recovery
    /// If pause and maintenance were toggled separately and drifted out of sync,
    /// call [`Self::exit_incident_mode`] to clear both, or re-call this function
    /// to realign them and refresh the reason.
    ///
    /// Re-entering while already in incident mode is idempotent and updates the
    /// stored reason.
    pub fn enter_incident_mode(
        env: &Env,
        admin: &Address,
        reason: &String,
    ) -> Result<IncidentSnapshot, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        if reason.len() > MAX_REASON_LEN {
            return Err(QuickLendXError::InvalidDescription);
        }

        MaintenanceControl::apply_maintenance_mode(env, true, reason, admin)?;
        PauseControl::apply_paused(env, true);

        Ok(Self::snapshot_from_state(env))
    }

    /// Atomically clear incident mode: unpause and disable maintenance.
    ///
    /// Idempotent when neither flag is set — both are cleared to `false` and an
    /// empty-reason snapshot is returned.
    pub fn exit_incident_mode(
        env: &Env,
        admin: &Address,
    ) -> Result<IncidentSnapshot, QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;

        PauseControl::apply_paused(env, false);
        MaintenanceControl::apply_maintenance_mode(env, false, &String::from_str(env, ""), admin)?;

        Ok(Self::snapshot_from_state(env))
    }

    fn snapshot_from_state(env: &Env) -> IncidentSnapshot {
        IncidentSnapshot {
            is_paused: PauseControl::is_paused(env),
            is_maintenance: MaintenanceControl::is_maintenance_mode(env),
            reason: MaintenanceControl::get_maintenance_reason(env)
                .unwrap_or_else(|| String::from_str(env, "")),
            timestamp: env.ledger().timestamp(),
        }
    }
}
