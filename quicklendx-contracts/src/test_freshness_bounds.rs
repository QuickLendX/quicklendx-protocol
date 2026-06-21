//! Freshness drift-bound regressions for issue #1331.
//!
//! The freshness indicator is monotonic with elapsed ledger time: once the
//! indexed ledger timestamp is past the accepted drift bound, additional
//! elapsed time must remain stale.

use crate::freshness::DEFAULT_MAX_FRESHNESS_DRIFT_SECS;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Ledger, Env, String};

fn setup() -> (Env, QuickLendXContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    (env, client)
}

fn is_stale_at_lag(env: &Env, client: &QuickLendXContractClient, lag_seconds: u64) -> String {
    let indexed_timestamp = 1_700_000_000u64;
    env.ledger()
        .set_timestamp(indexed_timestamp.saturating_add(lag_seconds));
    client
        .get_freshness(&500u32, &indexed_timestamp, &0u32)
        .get(String::from_str(env, "is_stale"))
        .unwrap()
}

#[test]
fn freshness_flips_one_second_after_drift_bound() {
    let (env, client) = setup();
    let bound = DEFAULT_MAX_FRESHNESS_DRIFT_SECS;

    assert_eq!(
        is_stale_at_lag(&env, &client, bound.saturating_sub(1)),
        String::from_str(&env, "false"),
        "lag below the drift bound must be fresh"
    );
    assert_eq!(
        is_stale_at_lag(&env, &client, bound),
        String::from_str(&env, "false"),
        "lag exactly at the drift bound must still be fresh"
    );
    assert_eq!(
        is_stale_at_lag(&env, &client, bound + 1),
        String::from_str(&env, "true"),
        "lag one second past the drift bound must be stale"
    );
}

#[test]
fn freshness_is_monotonic_with_elapsed_time() {
    let (env, client) = setup();
    let bound = DEFAULT_MAX_FRESHNESS_DRIFT_SECS;

    let samples = [0u64, bound / 2, bound, bound + 1, bound + 60, bound + 600];
    let mut saw_stale = false;
    for lag in samples {
        let stale = is_stale_at_lag(&env, &client, lag) == String::from_str(&env, "true");
        if saw_stale {
            assert!(
                stale,
                "freshness must not report fresher after a stale sample at lag {lag}"
            );
        }
        saw_stale = saw_stale || stale;
    }
}

#[test]
fn freshness_response_documents_active_drift_bound() {
    let (env, client) = setup();
    let indexed_timestamp = 1_700_000_000u64;
    env.ledger().set_timestamp(indexed_timestamp + DEFAULT_MAX_FRESHNESS_DRIFT_SECS);

    let result = client.get_freshness(&500u32, &indexed_timestamp, &0u32);
    assert_eq!(
        result
            .get(String::from_str(&env, "max_freshness_drift_seconds"))
            .unwrap(),
        String::from_str(&env, "300")
    );
    assert_eq!(
        result.get(String::from_str(&env, "index_lag_seconds")).unwrap(),
        String::from_str(&env, "300")
    );
}

