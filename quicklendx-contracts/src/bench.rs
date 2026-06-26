//! Bench helpers for gas/cpu instruction measurement.
//! This module is compiled only for tests and provides a `measure` helper.

#[cfg(any(test, feature = "testutils"))]
pub mod bench {
    use soroban_sdk::Env;

/// @notice Budget deltas recorded for a scenario.
/// @field instructions The number of CPU instructions executed.
/// @field read_bytes The number of bytes read from storage.
/// @field write_bytes The number of bytes written to storage.
pub struct BudgetDelta {
    pub instructions: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
}

    /// @notice Measure the budget delta for a closure.
    /// @param env The Soroban execution environment.
    /// @param label The label of the scenario being measured.
    /// @param f The closure executing the contract invocation.
    /// @return The recorded BudgetDelta.
    pub fn measure<F: FnOnce()>(env: &Env, _label: &str, f: F) -> BudgetDelta {
        f();
        let estimate = env.cost_estimate();
        let resources = estimate.resources();
        BudgetDelta {
            instructions: resources.instructions as u64,
            read_bytes: resources.disk_read_bytes as u64,
            write_bytes: resources.write_bytes as u64,
        }
    }
}
