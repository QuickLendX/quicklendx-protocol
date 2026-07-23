//! Contract-side backpressure read-through queries.
//!
//! On Soroban, adaptive load-shedding is surfaced through maintenance mode
//! because contracts cannot observe off-chain queue depth or database latency.
//! This module exposes the backpressure query surface by composing
//! [`MaintenanceControl::is_maintenance_mode`](crate::maintenance::MaintenanceControl::is_maintenance_mode)
//! without duplicating storage.

use crate::maintenance::MaintenanceControl;
use soroban_sdk::Env;

/// Read-through backpressure controller for contract-side load shedding.
pub struct BackpressureControl;

impl BackpressureControl {
    /// Returns `true` when mutating entrypoints are being shed due to load.
    pub fn is_active(env: &Env) -> bool {
        MaintenanceControl::is_maintenance_mode(env)
    }
}
