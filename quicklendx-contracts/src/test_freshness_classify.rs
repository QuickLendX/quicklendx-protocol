use crate::freshness::{FreshnessError, FreshnessMetadata, FreshnessTier};
use soroban_sdk::{Env, String};

fn metadata_with_lag(env: &Env, index_lag_seconds: i64) -> FreshnessMetadata {
    FreshnessMetadata {
        last_indexed_ledger: 100,
        index_lag_seconds,
        last_updated_at: String::from_str(env, "2026-07-07T00:00:00Z"),
        cursor: String::from_str(env, "100_0"),
    }
}

#[test]
fn freshness_classify_uses_inclusive_boundaries() {
    let env = Env::default();

    assert_eq!(
        metadata_with_lag(&env, 30).classify(30, 120),
        FreshnessTier::Fresh
    );
    assert_eq!(
        metadata_with_lag(&env, 31).classify(30, 120),
        FreshnessTier::Stale
    );
    assert_eq!(
        metadata_with_lag(&env, 120).classify(30, 120),
        FreshnessTier::Stale
    );
    assert_eq!(
        metadata_with_lag(&env, 121).classify(30, 120),
        FreshnessTier::Critical
    );
}

#[test]
fn freshness_classify_future_skew_is_fresh_when_within_threshold() {
    let env = Env::default();

    assert_eq!(
        metadata_with_lag(&env, -5).classify(30, 120),
        FreshnessTier::Fresh
    );
}

#[test]
fn freshness_classify_invalid_thresholds_fail_closed() {
    let env = Env::default();

    assert_eq!(
        metadata_with_lag(&env, 10).classify(120, 30),
        FreshnessTier::Critical
    );
}

#[test]
fn freshness_classify_try_rejects_invalid_threshold_order() {
    let env = Env::default();

    assert_eq!(
        metadata_with_lag(&env, 10).try_classify(120, 30),
        Err(FreshnessError::InvalidConfigValue)
    );
}

#[test]
fn freshness_classify_try_returns_tier_for_valid_thresholds() {
    let env = Env::default();

    assert_eq!(
        metadata_with_lag(&env, 90).try_classify(30, 120),
        Ok(FreshnessTier::Stale)
    );
}
