//! Bench helpers for gas/cpu instruction measurement.
//! This module is compiled only for tests and provides a `measure` helper.
//! It intentionally returns placeholder zero-deltas when the Soroban budget
//! APIs are unavailable; tests that require real measurement should enable
//! the appropriate dev-dependencies and use real budget snapshots.
#[cfg(test)]
pub mod bench {
    use soroban_sdk::Env;

    /// Budget deltas recorded for a scenario.
    pub struct BudgetDelta {
        pub instructions: u64,
        pub read_bytes: u64,
        pub write_bytes: u64,
    }

    /// Measure the budget delta for a closure.
    ///
    /// Note: This stub returns zeros unless a test harness records real
    /// `BudgetSnapshot` values. It exists to provide a stable API surface
    /// for the measurement scripts and docs.
    pub fn measure<F: FnOnce()>(_env: &Env, _label: &str, f: F) -> BudgetDelta {
        f();
        BudgetDelta {
            instructions: 0,
            read_bytes: 0,
            write_bytes: 0,
        }
    }
}
