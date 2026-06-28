//! Incremental time-weighted averages over ledger intervals.
//!
//! Each observed value is held constant until the next roll-forward. A value
//! therefore contributes `value * ledger_delta` to the running integral.

/// Errors produced while advancing a [`TimeWeightedAverage`].
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TimeWeightedAverageError {
    ArithmeticOverflow,
}

/// A checked, incremental time-weighted-average accumulator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeWeightedAverage {
    current_value: i128,
    weighted_sum: i128,
    total_ledger_delta: u64,
}

impl TimeWeightedAverage {
    /// Start an accumulator with the value held during the first interval.
    pub const fn new(initial_value: i128) -> Self {
        Self {
            current_value: initial_value,
            weighted_sum: 0,
            total_ledger_delta: 0,
        }
    }

    /// Advance by `ledger_delta`, then make `next_value` current.
    ///
    /// A zero delta changes the current value without adding elapsed time. If
    /// either accumulator would overflow, no state is changed.
    pub fn roll_forward(
        &mut self,
        ledger_delta: u64,
        next_value: i128,
    ) -> Result<(), TimeWeightedAverageError> {
        let contribution = self
            .current_value
            .checked_mul(i128::from(ledger_delta))
            .ok_or(TimeWeightedAverageError::ArithmeticOverflow)?;
        let weighted_sum = self
            .weighted_sum
            .checked_add(contribution)
            .ok_or(TimeWeightedAverageError::ArithmeticOverflow)?;
        let total_ledger_delta = self
            .total_ledger_delta
            .checked_add(ledger_delta)
            .ok_or(TimeWeightedAverageError::ArithmeticOverflow)?;

        self.current_value = next_value;
        self.weighted_sum = weighted_sum;
        self.total_ledger_delta = total_ledger_delta;
        Ok(())
    }

    /// Return the time-weighted average, or `None` before any time has elapsed.
    ///
    /// Division follows Rust's integer semantics and truncates toward zero.
    pub fn average(&self) -> Option<i128> {
        if self.total_ledger_delta == 0 {
            None
        } else {
            Some(self.weighted_sum / i128::from(self.total_ledger_delta))
        }
    }
}
