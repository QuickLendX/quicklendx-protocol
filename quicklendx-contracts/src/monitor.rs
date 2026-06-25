//! Operational health aggregation for write-gating and degraded-state banners.
//!
//! [`HealthStatus`] composes pause, maintenance, backpressure, and freshness
//! signals into a single ledger-consistent snapshot via [`get_health_status`].

use crate::backpressure::BackpressureControl;
use crate::freshness::{FreshnessMetadata, DEFAULT_MAX_FRESHNESS_DRIFT_SECS};
use crate::maintenance::MaintenanceControl;
use crate::pause::PauseControl;
use soroban_sdk::{contracttype, Env, String};

/// Single-read operational snapshot for clients, indexers, and monitoring.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HealthStatus {
    /// Emergency pause flag from [`PauseControl::is_paused`].
    pub is_paused: bool,
    /// Planned maintenance / read-only mode from [`MaintenanceControl::is_maintenance_mode`].
    pub is_maintenance: bool,
    /// Human-readable maintenance reason, if maintenance is active.
    pub maintenance_reason: Option<String>,
    /// Load-shedding flag from [`BackpressureControl::is_active`].
    pub backpressure_active: bool,
    /// Freshness lag in seconds using the current ledger as the indexed point.
    pub index_lag_seconds: i64,
    /// `true` when `index_lag_seconds` exceeds [`DEFAULT_MAX_FRESHNESS_DRIFT_SECS`].
    pub data_is_stale: bool,
    /// Derived write gate for clients. See [`derive_writes_allowed`].
    pub writes_allowed: bool,
}

/// Build a fresh [`HealthStatus`] snapshot from existing public query surfaces.
pub fn get_health_status(env: &Env) -> HealthStatus {
    let is_paused = PauseControl::is_paused(env);
    let is_maintenance = MaintenanceControl::is_maintenance_mode(env);
    let maintenance_reason = MaintenanceControl::get_maintenance_reason(env);
    let backpressure_active = BackpressureControl::is_active(env);

    let freshness = FreshnessMetadata::from_env(
        env,
        env.ledger().sequence(),
        env.ledger().timestamp(),
        0,
    );
    let data_is_stale = freshness.index_lag_seconds > DEFAULT_MAX_FRESHNESS_DRIFT_SECS;

    HealthStatus {
        is_paused,
        is_maintenance,
        maintenance_reason,
        backpressure_active,
        index_lag_seconds: freshness.index_lag_seconds,
        data_is_stale,
        writes_allowed: derive_writes_allowed(is_paused, is_maintenance, backpressure_active),
    }
}

/// Returns `true` only when pause, maintenance, and backpressure all permit writes.
///
/// `data_is_stale` is intentionally excluded: freshness is advisory for indexed
/// off-chain reads and does not gate on-ledger mutating entrypoints.
pub fn derive_writes_allowed(
    is_paused: bool,
    is_maintenance: bool,
    backpressure_active: bool,
) -> bool {
    !is_paused && !is_maintenance && !backpressure_active
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_writes_allowed_all_clear() {
        assert!(derive_writes_allowed(false, false, false));
    }

    #[test]
    fn test_writes_allowed_blocked_when_paused() {
        assert!(!derive_writes_allowed(true, false, false));
    }

    #[test]
    fn test_writes_allowed_blocked_when_maintenance() {
        assert!(!derive_writes_allowed(false, true, false));
    }

    #[test]
    fn test_writes_allowed_blocked_when_backpressure() {
        assert!(!derive_writes_allowed(false, false, true));
    }

    #[test]
    fn test_writes_allowed_blocked_when_paused_and_maintenance() {
        assert!(!derive_writes_allowed(true, true, false));
    }

    #[test]
    fn test_writes_allowed_blocked_when_all_flags() {
        assert!(!derive_writes_allowed(true, true, true));
    }
}
