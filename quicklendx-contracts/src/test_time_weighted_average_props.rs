//! Property coverage for the incremental time-weighted-average accumulator.

use crate::time_weighted_average::{TimeWeightedAverage, TimeWeightedAverageError};
use proptest::prelude::*;

const MAX_VALUE: i128 = 1_000_000_000_000;
const MAX_LEDGER_DELTA: u64 = 100_000;

fn reference_average(initial_value: i128, updates: &[(u64, i128)]) -> Option<i128> {
    let mut current_value = initial_value;
    let mut weighted_sum = 0i128;
    let mut total_ledger_delta = 0u64;

    for &(ledger_delta, next_value) in updates {
        weighted_sum += current_value * i128::from(ledger_delta);
        total_ledger_delta += ledger_delta;
        current_value = next_value;
    }

    (total_ledger_delta != 0).then(|| weighted_sum / i128::from(total_ledger_delta))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn roll_forward_random_ledger_deltas_matches_reference_implementation(
        initial_value in -MAX_VALUE..=MAX_VALUE,
        updates in prop::collection::vec(
            (0u64..=MAX_LEDGER_DELTA, -MAX_VALUE..=MAX_VALUE),
            1..64,
        ),
    ) {
        let expected = reference_average(initial_value, &updates);
        let mut actual = TimeWeightedAverage::new(initial_value);

        for &(ledger_delta, next_value) in &updates {
            actual
                .roll_forward(ledger_delta, next_value)
                .expect("generated values stay inside i128 bounds");
        }

        prop_assert_eq!(actual.average(), expected);
    }
}

#[test]
fn returns_none_when_no_ledger_time_has_elapsed() {
    let mut average = TimeWeightedAverage::new(10);
    average.roll_forward(0, 20).unwrap();
    average.roll_forward(0, 30).unwrap();

    assert_eq!(average.average(), None);
}

#[test]
fn leaves_state_unchanged_when_roll_forward_overflows() {
    let mut average = TimeWeightedAverage::new(i128::MAX);
    let before = average.clone();

    assert_eq!(
        average.roll_forward(2, 0),
        Err(TimeWeightedAverageError::ArithmeticOverflow)
    );
    assert_eq!(average, before);
}
