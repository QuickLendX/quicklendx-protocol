//! Boundary tests for the documented freshness drift threshold.

use super::*;
use soroban_sdk::{testutils::Ledger, Env, String};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

fn index_lag_seconds(
    env: &Env,
    client: &QuickLendXContractClient,
    current: u64,
    indexed: u64,
) -> i64 {
    env.ledger().set_timestamp(current);

    let result = client.get_freshness(&100u32, &indexed, &0u32).unwrap();
    let lag = result
        .get(String::from_str(env, "index_lag_seconds"))
        .unwrap();
    let len = lag.len() as usize;
    let mut buf = [0u8; 22];
    lag.copy_into_slice(&mut buf[..len]);

    core::str::from_utf8(&buf[..len])
        .unwrap()
        .parse::<i64>()
        .unwrap()
}

fn is_stale(lag_seconds: i64) -> bool {
    lag_seconds > freshness::DEFAULT_MAX_FRESHNESS_DRIFT_SECS
}

#[test]
fn test_freshness_bound_is_inclusive_and_one_second_past_is_stale() {
    let (env, client) = setup();
    let current = 1_700_000_120u64;
    let bound = freshness::DEFAULT_MAX_FRESHNESS_DRIFT_SECS as u64;

    let exactly_at_bound = index_lag_seconds(&env, &client, current, current - bound);
    let one_second_past_bound = index_lag_seconds(&env, &client, current, current - bound - 1);

    assert_eq!(
        exactly_at_bound,
        freshness::DEFAULT_MAX_FRESHNESS_DRIFT_SECS
    );
    assert!(!is_stale(exactly_at_bound));
    assert_eq!(
        one_second_past_bound,
        freshness::DEFAULT_MAX_FRESHNESS_DRIFT_SECS + 1
    );
    assert!(is_stale(one_second_past_bound));
}

#[test]
fn test_freshness_lag_is_monotonic_with_elapsed_time() {
    let (env, client) = setup();
    let current = 1_700_001_000u64;
    let elapsed_times = [0u64, 1, 30, 120, 121, 600];
    let mut previous = i64::MIN;

    for elapsed in elapsed_times {
        let lag = index_lag_seconds(&env, &client, current, current - elapsed);

        assert!(
            lag >= previous,
            "lag {lag} should not be fresher than prior lag {previous}"
        );
        previous = lag;
    }
}

#[test]
fn test_freshness_at_write_time_and_epoch_start_is_fresh() {
    let (env, client) = setup();

    let epoch_start_lag = index_lag_seconds(&env, &client, 0, 0);
    let current_write_lag = index_lag_seconds(&env, &client, 1_700_000_000, 1_700_000_000);

    assert_eq!(epoch_start_lag, 0);
    assert!(!is_stale(epoch_start_lag));
    assert_eq!(current_write_lag, 0);
    assert!(!is_stale(current_write_lag));
}

#[test]
fn test_future_indexed_timestamp_is_clock_skew_not_stale() {
    let (env, client) = setup();

    let lag = index_lag_seconds(&env, &client, 1_700_000_000, 1_700_000_005);

    assert_eq!(lag, -5);
    assert!(!is_stale(lag));
}
