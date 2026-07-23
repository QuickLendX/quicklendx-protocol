//! # Freshness lag calculation tests (Issue #1541)
//!
//! Locks in the three core lag scenarios for `get_freshness`:
//!
//! 1. **Lag at zero** — indexed timestamp matches current ledger time; lag = 0.
//! 2. **Lag at positive** — current ledger time is ahead of indexed timestamp;
//!    lag = current - indexed (positive seconds).
//! 3. **Lag during pause** — contract is paused but `get_freshness` remains
//!    callable (it is a read-only diagnostic endpoint) and still returns the
//!    correct lag for the time elapsed since indexing.
//!
//! These tests use plain `#[cfg(test)]` — no feature gate — so they run on
//! every CI matrix entry.

#[cfg(test)]
mod test_freshness_lag {
    use crate::freshness::DEFAULT_MAX_FRESHNESS_DRIFT_SECS;
    use crate::{QuickLendXContract, QuickLendXContractClient};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, Env, String,
    };

    // =========================================================================
    // Helpers
    // =========================================================================

    /// Register the contract, mock all auths, and initialize an admin so that
    /// pause/unpause calls have a valid admin to authorize against.
    fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.initialize_admin(&admin);
        (env, client, admin)
    }

    /// Extract `index_lag_seconds` from a `get_freshness` response as `i64`.
    fn lag_from_response(env: &Env, result: &soroban_sdk::Map<String, String>) -> i64 {
        let raw = result
            .get(String::from_str(env, "index_lag_seconds"))
            .expect("index_lag_seconds key must be present");
        let len = raw.len() as usize;
        let mut buf = [0u8; 24];
        raw.copy_into_slice(&mut buf[..len]);
        core::str::from_utf8(&buf[..len])
            .expect("lag must be valid UTF-8")
            .parse::<i64>()
            .expect("lag must parse as i64")
    }

    // =========================================================================
    // Happy path — lag at zero
    // =========================================================================

    /// When the indexed timestamp exactly equals the current ledger timestamp,
    /// `index_lag_seconds` must be zero.
    #[test]
    fn lag_is_zero_when_indexed_timestamp_matches_current_ledger_time() {
        let (env, client, _) = setup();
        let now = 1_700_000_000u64;
        env.ledger().set_timestamp(now);

        let result = client.get_freshness(&500u32, &now, &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert_eq!(lag, 0, "lag must be exactly 0 when indexed == current");
    }

    /// Zero lag must not be considered stale regardless of the drift threshold.
    #[test]
    fn lag_zero_is_not_stale() {
        let (env, client, _) = setup();
        let now = 1_700_000_000u64;
        env.ledger().set_timestamp(now);

        let result = client.get_freshness(&500u32, &now, &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert!(
            lag <= DEFAULT_MAX_FRESHNESS_DRIFT_SECS,
            "zero lag must be within the freshness drift bound"
        );
    }

    /// Zero lag holds at the Unix epoch boundary (timestamp = 0, ledger_seq = 1).
    ///
    /// Note: `indexed_ledger_seq = 0` is now rejected with
    /// `InvalidLedgerSequence` (see issue #1485). This test verifies that
    /// supplying the minimum valid sequence (1) at timestamp 0 still produces
    /// zero lag, and that supplying 0 is correctly rejected.
    #[test]
    fn lag_is_zero_at_unix_epoch_when_indexed_equals_current() {
        let (env, client, _) = setup();
        env.ledger().set_timestamp(0);
        env.ledger().set_sequence_number(1);

        // Minimum valid ledger sequence (1) at epoch boundary => zero lag.
        let result = client.get_freshness(&1u32, &0u64, &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert_eq!(
            lag, 0,
            "lag must be 0 at epoch boundary when timestamps match"
        );
    }

    /// Ledger sequence 0 is rejected: it is not a valid Soroban sequence number
    /// and would allow callers to inject a sentinel that bypasses freshness
    /// checks (see issue #1485).
    #[test]
    fn ledger_seq_zero_is_rejected() {
        let (env, client, _) = setup();
        env.ledger().set_timestamp(0);

        let err = client.try_get_freshness(&0u32, &0u64, &0u32)
            .expect_err("ledger_seq 0 must be rejected");
        assert_eq!(
            err,
            Ok(crate::errors::QuickLendXError::InvalidLedgerSequence),
            "expected InvalidLedgerSequence error for ledger_seq = 0"
        );
    }

    // =========================================================================
    // Happy path — lag at positive
    // =========================================================================

    /// When the current ledger time is ahead of the indexed timestamp, lag is a
    /// positive number of seconds equal to (current − indexed).
    #[test]
    fn lag_equals_elapsed_seconds_when_current_is_ahead_of_indexed() {
        let (env, client, _) = setup();
        let indexed = 1_700_000_000u64;
        let elapsed = 60u64;
        env.ledger().set_timestamp(indexed + elapsed);

        let result = client.get_freshness(&500u32, &indexed, &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert_eq!(lag, elapsed as i64, "lag must equal elapsed seconds");
    }

    /// Lag of exactly one second is positive and fresh.
    #[test]
    fn lag_of_one_second_is_positive_and_fresh() {
        let (env, client, _) = setup();
        let indexed = 1_700_000_000u64;
        env.ledger().set_timestamp(indexed + 1);

        let result = client.get_freshness(&500u32, &indexed, &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert_eq!(lag, 1);
        assert!(
            lag <= DEFAULT_MAX_FRESHNESS_DRIFT_SECS,
            "1 s lag is well within the drift bound"
        );
    }

    /// Lag exactly at the drift bound is accepted as fresh.
    #[test]
    fn lag_exactly_at_drift_bound_is_accepted_as_fresh() {
        let (env, client, _) = setup();
        let bound = DEFAULT_MAX_FRESHNESS_DRIFT_SECS as u64;
        let indexed = 1_700_000_000u64;
        env.ledger().set_timestamp(indexed + bound);

        let result = client.get_freshness(&500u32, &indexed, &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert_eq!(lag, DEFAULT_MAX_FRESHNESS_DRIFT_SECS);
        assert!(
            lag <= DEFAULT_MAX_FRESHNESS_DRIFT_SECS,
            "lag at the bound is not yet stale"
        );
    }

    /// Lag one second past the drift bound is stale.
    #[test]
    fn lag_one_second_past_drift_bound_is_stale() {
        let (env, client, _) = setup();
        let bound = DEFAULT_MAX_FRESHNESS_DRIFT_SECS as u64;
        let indexed = 1_700_000_000u64;
        env.ledger().set_timestamp(indexed + bound + 1);

        let result = client.get_freshness(&500u32, &indexed, &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert_eq!(lag, DEFAULT_MAX_FRESHNESS_DRIFT_SECS + 1);
        assert!(
            lag > DEFAULT_MAX_FRESHNESS_DRIFT_SECS,
            "lag one second past the bound must be considered stale"
        );
    }

    /// Lag grows monotonically as elapsed time increases.
    #[test]
    fn lag_grows_monotonically_with_elapsed_time() {
        let (env, client, _) = setup();
        let indexed = 1_700_000_000u64;
        let steps = [0u64, 1, 30, 60, 120, 121, 300, 600];
        let mut previous = i64::MIN;

        for &elapsed in &steps {
            env.ledger().set_timestamp(indexed + elapsed);
            let result = client.get_freshness(&500u32, &indexed, &0u32).unwrap();
            let lag = lag_from_response(&env, &result);

            assert!(
                lag >= previous,
                "lag {lag} at elapsed {elapsed}s must not be less than previous lag {previous}"
            );
            assert_eq!(
                lag, elapsed as i64,
                "lag must equal elapsed seconds ({elapsed}s)"
            );
            previous = lag;
        }
    }

    // =========================================================================
    // Sad path — future indexed timestamp produces negative lag (clock skew)
    // =========================================================================

    /// When the indexed timestamp is in the future relative to the current ledger
    /// time (clock skew), lag is negative — this is not stale, it is a skew signal.
    #[test]
    fn lag_is_negative_when_indexed_timestamp_is_in_the_future() {
        let (env, client, _) = setup();
        let now = 1_700_000_000u64;
        let skew = 5u64;
        env.ledger().set_timestamp(now);

        let result = client.get_freshness(&500u32, &(now + skew), &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert_eq!(
            lag,
            -(skew as i64),
            "future indexed timestamp gives negative lag"
        );
        assert!(
            lag <= DEFAULT_MAX_FRESHNESS_DRIFT_SECS,
            "negative lag (clock skew) must not be treated as stale"
        );
    }

    // =========================================================================
    // Lag during pause
    // =========================================================================

    /// `get_freshness` is a diagnostic read-only endpoint; it must remain callable
    /// and return the correct lag even when the contract is paused.
    #[test]
    fn lag_is_correct_when_contract_is_paused() {
        let (env, client, admin) = setup();
        let indexed = 1_700_000_000u64;
        let elapsed = 45u64;
        env.ledger().set_timestamp(indexed + elapsed);

        // Pause the contract.
        client.pause(&admin);

        // get_freshness must still succeed — it is not gated by require_not_paused.
        let result = client.get_freshness(&500u32, &indexed, &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert_eq!(
            lag, elapsed as i64,
            "lag must equal elapsed seconds even while contract is paused"
        );
    }

    /// Zero lag is reported correctly when the contract is paused and indexed
    /// timestamp matches current time.
    #[test]
    fn lag_is_zero_when_paused_and_timestamps_match() {
        let (env, client, admin) = setup();
        let now = 1_700_000_000u64;
        env.ledger().set_timestamp(now);

        client.pause(&admin);

        let result = client.get_freshness(&500u32, &now, &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert_eq!(lag, 0, "zero lag must be reported correctly while paused");
    }

    /// Positive lag accumulates during a pause just as it would when the contract
    /// is live: the pause does not freeze or reset the lag calculation.
    #[test]
    fn lag_accumulates_correctly_during_pause() {
        let (env, client, admin) = setup();
        let indexed = 1_700_000_000u64;

        // Pause at t = indexed + 10.
        env.ledger().set_timestamp(indexed + 10);
        client.pause(&admin);

        // Advance time further while paused.
        let elapsed_while_paused = 90u64;
        env.ledger()
            .set_timestamp(indexed + 10 + elapsed_while_paused);

        let result = client.get_freshness(&500u32, &indexed, &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert_eq!(
            lag,
            (10 + elapsed_while_paused) as i64,
            "lag must reflect total elapsed time since indexing, not just time since pause"
        );
    }

    /// After unpausing, lag continues to be reported correctly — the unpause
    /// does not affect the lag calculation.
    #[test]
    fn lag_is_correct_after_unpause() {
        let (env, client, admin) = setup();
        let indexed = 1_700_000_000u64;

        env.ledger().set_timestamp(indexed + 30);
        client.pause(&admin);

        env.ledger().set_timestamp(indexed + 90);
        client.unpause(&admin);

        let result = client.get_freshness(&500u32, &indexed, &0u32).unwrap();
        let lag = lag_from_response(&env, &result);

        assert_eq!(lag, 90, "lag must equal total elapsed time after unpause");
    }
}
