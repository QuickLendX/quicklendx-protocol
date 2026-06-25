//! Bench helpers for gas/cpu instruction measurement.
//! This module is compiled only for tests and provides a `measure` helper.

#[cfg(test)]
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
        // Reset the budget tracker so we get a clean delta for this closure.
        env.budget().reset_unlimited();
        f();
        let budget = env.budget();
        BudgetDelta {
            instructions: budget.cpu_instruction_cost(),
            read_bytes: budget.memory_bytes_cost(),
            write_bytes: 0,
        }
    }
}
