/// Analytics boundary and empty-range behavior tests (Issue #785).
///
/// Covers all `TimePeriod` edge cases, zero-length period ranges,
/// empty-window queries, and stability under data growth.
/// Assertions already covered by `test_analytics_consistency.rs` are omitted.
use crate::analytics::{AnalyticsCalculator, AnalyticsStorage, TimePeriod};
use crate::invoice::InvoiceCategory;
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

// ─── helpers ──────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (QuickLendXContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let business = Address::generate(env);
    env.mock_all_auths();
    client.set_admin(&admin);
    (client, contract_id, admin, business)
}

fn upload(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    desc: &str,
) -> soroban_sdk::BytesN<32> {
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400;
    client.store_invoice(
        business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, desc),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
    )
}

// ─── 1–9: TimePeriod edge arithmetic (pure — no contract needed) ──────────────

/// 1. Every period at ts=0 saturates via saturating_sub to (start=0, end=0).
#[test]
fn test_period_all_variants_at_zero_timestamp() {
    for period in [
        TimePeriod::Daily,
        TimePeriod::Weekly,
        TimePeriod::Monthly,
        TimePeriod::Quarterly,
        TimePeriod::Yearly,
        TimePeriod::AllTime,
    ] {
        let (start, end) = AnalyticsCalculator::get_period_dates(0, period.clone());
        assert_eq!(start, 0, "{period:?} start at ts=0 must be 0");
        assert_eq!(end, 0, "{period:?} end at ts=0 must be 0");
    }
}

/// 2. Daily at ts == 86_400 yields exactly a 24-hour window starting at 0.
#[test]
fn test_period_daily_at_exactly_one_day() {
    let ts = 86_400u64;
    let (start, end) = AnalyticsCalculator::get_period_dates(ts, TimePeriod::Daily);
    assert_eq!(start, 0);
    assert_eq!(end, ts);
    assert_eq!(end - start, 86_400);
}

/// 3. Weekly at ts == 7×86_400 yields exactly a 7-day window.
#[test]
fn test_period_weekly_at_exactly_one_week() {
    let ts = 7 * 86_400u64;
    let (start, end) = AnalyticsCalculator::get_period_dates(ts, TimePeriod::Weekly);
    assert_eq!(start, 0);
    assert_eq!(end, ts);
    assert_eq!(end - start, 7 * 86_400);
}

/// 4. Monthly at ts == 30×86_400 yields exactly a 30-day window.
#[test]
fn test_period_monthly_at_exactly_thirty_days() {
    let ts = 30 * 86_400u64;
    let (start, end) = AnalyticsCalculator::get_period_dates(ts, TimePeriod::Monthly);
    assert_eq!(start, 0);
    assert_eq!(end, ts);
    assert_eq!(end - start, 30 * 86_400);
}

/// 5. Quarterly at ts == 90×86_400 yields exactly a 90-day window.
#[test]
fn test_period_quarterly_at_exactly_ninety_days() {
    let ts = 90 * 86_400u64;
    let (start, end) = AnalyticsCalculator::get_period_dates(ts, TimePeriod::Quarterly);
    assert_eq!(start, 0);
    assert_eq!(end, ts);
    assert_eq!(end - start, 90 * 86_400);
}

/// 6. Yearly at ts == 365×86_400 yields exactly a 365-day window.
#[test]
fn test_period_yearly_at_exactly_three_sixty_five_days() {
    let ts = 365 * 86_400u64;
    let (start, end) = AnalyticsCalculator::get_period_dates(ts, TimePeriod::Yearly);
    assert_eq!(start, 0);
    assert_eq!(end, ts);
    assert_eq!(end - start, 365 * 86_400);
}

/// 7. AllTime always returns (0, ts) regardless of the current timestamp.
#[test]
fn test_period_alltime_start_always_zero_end_always_ts() {
    for ts in [0u64, 1, 86_400, 1_000_000, u64::MAX / 2] {
        let (start, end) = AnalyticsCalculator::get_period_dates(ts, TimePeriod::AllTime);
        assert_eq!(start, 0, "AllTime start must be 0 at ts={ts}");
        assert_eq!(end, ts, "AllTime end must equal ts={ts}");
    }
}

/// 8. Invariant: start ≤ end for every period at every representative timestamp.
#[test]
fn test_period_start_never_exceeds_end_invariant() {
    let timestamps = [0u64, 1, 86_399, 86_400, 86_401, 7 * 86_400, 1_000_000, u64::MAX / 2];
    for ts in timestamps {
        for period in [
            TimePeriod::Daily,
            TimePeriod::Weekly,
            TimePeriod::Monthly,
            TimePeriod::Quarterly,
            TimePeriod::Yearly,
            TimePeriod::AllTime,
        ] {
            let (start, end) = AnalyticsCalculator::get_period_dates(ts, period.clone());
            assert!(
                start <= end,
                "{period:?} at ts={ts}: start({start}) > end({end})"
            );
        }
    }
}

/// 9. A large timestamp (mid-u64) never causes overflow on any period.
#[test]
fn test_period_large_timestamp_no_overflow() {
    let ts = u64::MAX / 2;
    for period in [
        TimePeriod::Daily,
        TimePeriod::Weekly,
        TimePeriod::Monthly,
        TimePeriod::Quarterly,
        TimePeriod::Yearly,
        TimePeriod::AllTime,
    ] {
        let (start, end) = AnalyticsCalculator::get_period_dates(ts, period.clone());
        assert!(start <= end, "{period:?} at ts={ts}: start > end");
        assert_eq!(end, ts, "{period:?} end must equal ts");
    }
}

// ─── 10–11: Zero-length and near-zero ranges ──────────────────────────────────

/// 10. At genesis (ts=0), AllTime yields a degenerate window where start == end == 0.
#[test]
fn test_zero_length_alltime_range_at_genesis() {
    let (start, end) = AnalyticsCalculator::get_period_dates(0, TimePeriod::AllTime);
    assert_eq!(start, 0);
    assert_eq!(end, 0);
    assert_eq!(start, end);
}

/// 11. At ts=1 every timed period's start saturates to 0 (window smaller than duration).
#[test]
fn test_period_all_timed_near_zero_saturate_start_to_zero() {
    for period in [
        TimePeriod::Daily,
        TimePeriod::Weekly,
        TimePeriod::Monthly,
        TimePeriod::Quarterly,
        TimePeriod::Yearly,
    ] {
        let (start, end) = AnalyticsCalculator::get_period_dates(1, period.clone());
        assert_eq!(start, 0, "{period:?} at ts=1 must saturate to start=0");
        assert_eq!(end, 1);
    }
}

// ─── 12–13: Period boundary inclusion / exclusion ─────────────────────────────

/// 12. An invoice created at exactly start_date (≥ boundary) is included.
#[test]
fn test_invoice_at_exact_start_date_is_included() {
    let env = Env::default();
    let day = 86_400u64;
    env.ledger().set_timestamp(day);
    let (client, contract_id, _, business) = setup(&env);
    upload(&env, &client, &business, 1_000, "start-boundary");

    env.ledger().set_timestamp(2 * day);
    let report = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::generate_business_report(&env, &business, TimePeriod::Daily)
        })
        .unwrap();
    assert_eq!(
        report.invoices_uploaded, 1,
        "invoice at exact start_date must be included"
    );
}

/// 13. An invoice created one second before start_date falls outside the window.
#[test]
fn test_invoice_one_second_before_start_date_is_excluded() {
    let env = Env::default();
    let day = 86_400u64;
    env.ledger().set_timestamp(day - 1);
    let (client, contract_id, _, business) = setup(&env);
    upload(&env, &client, &business, 1_000, "before-boundary");

    env.ledger().set_timestamp(2 * day);
    let report = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::generate_business_report(&env, &business, TimePeriod::Daily)
        })
        .unwrap();
    assert_eq!(
        report.invoices_uploaded, 0,
        "invoice one second before start_date must be excluded"
    );
}

// ─── 14–18: Empty-range behavior ──────────────────────────────────────────────

/// 14. A business report over an empty daily window returns all-zero counts.
#[test]
fn test_business_report_empty_daily_window_all_zeros() {
    let env = Env::default();
    env.ledger().set_timestamp(10_000_000);
    let (_client, contract_id, _, business) = setup(&env);

    let report = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::generate_business_report(&env, &business, TimePeriod::Daily)
        })
        .unwrap();
    assert_eq!(report.invoices_uploaded, 0);
    assert_eq!(report.invoices_funded, 0);
    assert_eq!(report.total_volume, 0);
    assert_eq!(report.success_rate, 0);
    assert_eq!(report.default_rate, 0);
}

/// 15. Financial metrics for an empty daily window return zero volume.
#[test]
fn test_financial_metrics_empty_daily_window_zero_volume() {
    let env = Env::default();
    let day = 86_400u64;
    env.ledger().set_timestamp(2 * day);
    let (client, contract_id, _, business) = setup(&env);
    upload(&env, &client, &business, 9_000, "outside-daily");

    env.ledger().set_timestamp(4 * day);
    let metrics = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::calculate_financial_metrics(&env, TimePeriod::Daily)
        })
        .unwrap();
    assert_eq!(metrics.total_volume, 0);
    assert_eq!(metrics.total_fees, 0);
    assert_eq!(metrics.total_profits, 0);
}

/// 16. Financial metrics AllTime returns zero when no invoices have been created.
#[test]
fn test_financial_metrics_alltime_zero_with_no_invoices() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (_client, contract_id, _, _) = setup(&env);

    let metrics = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::calculate_financial_metrics(&env, TimePeriod::AllTime)
        })
        .unwrap();
    assert_eq!(metrics.total_volume, 0);
    assert_eq!(metrics.total_fees, 0);
    assert_eq!(metrics.total_profits, 0);
    assert_eq!(metrics.average_return_rate, 0);
}

/// 17. An investor analytics report over an empty period has zero investments.
#[test]
fn test_investor_analytics_empty_period_zero_investments() {
    let env = Env::default();
    env.ledger().set_timestamp(5_000_000);
    let (_client, contract_id, _, _) = setup(&env);
    let investor = Address::generate(&env);

    let report = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::generate_investor_report(&env, &investor, TimePeriod::Monthly)
        })
        .expect("empty investor report must not error");

    assert_eq!(report.investments_made, 0);
    assert_eq!(report.total_invested, 0);
    assert_eq!(report.success_rate, 0);
    assert_eq!(report.default_rate, 0);

    let stored = env
        .as_contract(&contract_id, || {
            AnalyticsStorage::get_investor_report(&env, &report.report_id)
        })
        .expect("generated report must be persisted");
    assert_eq!(stored.investments_made, 0);
    assert_eq!(stored.report_id, report.report_id);
}

/// 18. Platform metrics on a brand-new contract are all zero.
#[test]
fn test_platform_metrics_empty_state_all_zeros() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _, _, _) = setup(&env);

    let m = client.get_platform_metrics();
    assert_eq!(m.total_invoices, 0);
    assert_eq!(m.total_investments, 0);
    assert_eq!(m.total_volume, 0);
    assert_eq!(m.total_fees_collected, 0);
    assert_eq!(m.average_invoice_amount, 0);
    assert_eq!(m.average_investment_amount, 0);
    assert_eq!(m.success_rate, 0);
    assert_eq!(m.default_rate, 0);
}

// ─── 19–22: Growth stability & financial windowing ────────────────────────────

/// 19. total_invoices grows by exactly 1 for every successful store_invoice call.
#[test]
fn test_platform_metrics_total_invoices_grows_per_upload() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, _, _, business) = setup(&env);

    assert_eq!(client.get_platform_metrics().total_invoices, 0);

    upload(&env, &client, &business, 1_000, "g1");
    assert_eq!(client.get_platform_metrics().total_invoices, 1);

    upload(&env, &client, &business, 2_000, "g2");
    assert_eq!(client.get_platform_metrics().total_invoices, 2);

    upload(&env, &client, &business, 3_000, "g3");
    assert_eq!(client.get_platform_metrics().total_invoices, 3);
}

/// 20. AllTime always reflects every invoice in the dataset as it grows.
#[test]
fn test_alltime_captures_all_invoices_under_growth() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000);
    let (client, contract_id, _, business) = setup(&env);

    upload(&env, &client, &business, 1_000, "g-1");
    upload(&env, &client, &business, 2_000, "g-2");
    upload(&env, &client, &business, 3_000, "g-3");

    let r1 = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::generate_business_report(&env, &business, TimePeriod::AllTime)
        })
        .unwrap();
    assert_eq!(r1.invoices_uploaded, 3);
    assert_eq!(r1.total_volume, 6_000);

    upload(&env, &client, &business, 4_000, "g-4");
    let r2 = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::generate_business_report(&env, &business, TimePeriod::AllTime)
        })
        .unwrap();
    assert_eq!(r2.invoices_uploaded, 4);
    assert_eq!(r2.total_volume, 10_000);
}

/// 21. Two reports generated at the same ledger timestamp have identical computed
///     fields but distinct report IDs (ID derives from sequence number too).
#[test]
fn test_two_reports_same_timestamp_identical_data_different_ids() {
    let env = Env::default();
    env.ledger().set_timestamp(2_000_000);
    let (client, contract_id, _, business) = setup(&env);
    upload(&env, &client, &business, 7_500, "idemp");

    let r1 = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::generate_business_report(&env, &business, TimePeriod::AllTime)
        })
        .unwrap();
    let r2 = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::generate_business_report(&env, &business, TimePeriod::AllTime)
        })
        .unwrap();

    assert_ne!(
        r1.report_id, r2.report_id,
        "IDs must differ even at the same timestamp"
    );
    assert_eq!(r1.invoices_uploaded, r2.invoices_uploaded);
    assert_eq!(r1.total_volume, r2.total_volume);
    assert_eq!(r1.success_rate, r2.success_rate);
    assert_eq!(r1.default_rate, r2.default_rate);
}

/// 22. Financial metrics daily window excludes invoices older than 24 hours;
///     AllTime correctly includes those same invoices.
#[test]
fn test_financial_metrics_daily_excludes_invoices_older_than_24h() {
    let env = Env::default();
    let day = 86_400u64;
    env.ledger().set_timestamp(2 * day);
    let (client, contract_id, _, business) = setup(&env);
    upload(&env, &client, &business, 5_000, "old-fin");

    env.ledger().set_timestamp(4 * day);
    let daily = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::calculate_financial_metrics(&env, TimePeriod::Daily)
        })
        .unwrap();
    assert_eq!(
        daily.total_volume, 0,
        "daily must not include a 2-day-old invoice"
    );

    let all_time = env
        .as_contract(&contract_id, || {
            AnalyticsCalculator::calculate_financial_metrics(&env, TimePeriod::AllTime)
        })
        .unwrap();
    assert_eq!(
        all_time.total_volume, 5_000,
        "AllTime must include all invoices"
    );
}
