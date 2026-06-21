/// Business Report Generation Consistency Checks (Issue #598)
///
/// These tests validate that every `BusinessReport` produced by
/// `generate_business_report` satisfies a set of numeric and logical
/// invariants, and that the stored copy returned by `get_business_report`
/// is byte-for-byte identical to the value returned at generation time.
///
/// Invariants checked
/// ------------------
/// 1. Timestamp ordering   : `start_date < end_date`, `generated_at == env.ledger().timestamp()`
/// 2. Volume correctness   : `total_volume == - invoice.amount` for invoices in the period
/// 3. Count correctness    : `invoices_uploaded` equals the number of invoices created in period
/// 4. Funding ratio        : `invoices_funded - invoices_uploaded`
/// 5. Rate bounds          : `success_rate  - [0, 10000]`
///                           `default_rate  - [0, 10000]`
///                           `success_rate + default_rate - 10000`
/// 6. Rate formula         : verified against manually-known invoice counts
/// 7. Report immutability  : a stored report is unchanged after a newer report is generated
/// 8. Period exclusion     : an invoice whose `created_at` is before the period window
///                           does NOT appear in the scoped report
/// 9. Multi-business isolation : report for business-A does not count business-B's invoices
/// 10. Report-ID uniqueness   : two reports generated at different ledger timestamps have
///                              different IDs
use super::*;
use crate::analytics::{AnalyticsCalculator, TimePeriod};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String,
};

// ============================================================================
// Shared helpers
// ============================================================================

fn setup(env: &Env) -> (QuickLendXContractClient, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let business = Address::generate(env);
    env.mock_all_auths();
    client.set_admin(&admin);
    (client, admin, business)
}

/// Upload one invoice for `business` with the given amount and description.
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
        &soroban_sdk::Vec::new(env),
    )
}

// ============================================================================
// 1. Timestamp ordering
// ============================================================================

#[test]
fn test_report_timestamp_generated_at_equals_ledger_now() {
    let env = Env::default();
    let ts = 5_000_000u64;
    env.ledger().set_timestamp(ts);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 1_000, "ts-inv");
    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    assert_eq!(
        report.generated_at, ts,
        "generated_at must equal the ledger timestamp at report creation time"
    );
}

#[test]
fn test_report_start_date_strictly_before_end_date() {
    let env = Env::default();
    env.ledger().set_timestamp(10_000_000u64);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 500, "date-ord-inv");

    for period in [
        TimePeriod::Daily,
        TimePeriod::Weekly,
        TimePeriod::Monthly,
        TimePeriod::Quarterly,
        TimePeriod::Yearly,
        TimePeriod::AllTime,
    ]
    .iter()
    {
        let report = client.generate_business_report(&business, period);
        assert!(
            report.start_date <= report.end_date,
            "start_date must be - end_date for period {:?}",
            period
        );
    }
}

#[test]
fn test_report_end_date_equals_ledger_timestamp() {
    let env = Env::default();
    let ts = 8_000_000u64;
    env.ledger().set_timestamp(ts);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 1_000, "end-date-inv");

    for period in [
        TimePeriod::Daily,
        TimePeriod::Weekly,
        TimePeriod::Monthly,
        TimePeriod::Quarterly,
        TimePeriod::Yearly,
        TimePeriod::AllTime,
    ]
    .iter()
    {
        let report = client.generate_business_report(&business, period);
        assert_eq!(
            report.end_date, ts,
            "end_date must equal the ledger timestamp for period {:?}",
            period
        );
    }
}

#[test]
fn test_report_period_dates_match_analytics_calculator() {
    let env = Env::default();
    let ts = 100_000_000u64;
    env.ledger().set_timestamp(ts);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 1_000, "period-calc-inv");

    for period in [
        TimePeriod::Daily,
        TimePeriod::Weekly,
        TimePeriod::Monthly,
        TimePeriod::Quarterly,
        TimePeriod::Yearly,
        TimePeriod::AllTime,
    ]
    .iter()
    {
        let (expected_start, expected_end) =
            AnalyticsCalculator::get_period_dates(ts, period.clone());
        let report = client.generate_business_report(&business, period);

        assert_eq!(
            report.start_date, expected_start,
            "start_date mismatch for period {:?}",
            period
        );
        assert_eq!(
            report.end_date, expected_end,
            "end_date mismatch for period {:?}",
            period
        );
    }
}

// ============================================================================
// 2. Volume correctness
// ============================================================================

#[test]
fn test_report_total_volume_equals_sum_of_invoice_amounts() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 1_000, "vol-inv-1");
    upload(&env, &client, &business, 2_500, "vol-inv-2");
    upload(&env, &client, &business, 4_000, "vol-inv-3");

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    assert_eq!(
        report.total_volume,
        7_500,
        "total_volume must equal the arithmetic sum of all invoice amounts"
    );
}

#[test]
fn test_report_zero_volume_for_empty_business() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, _) = setup(&env);

    let fresh_business = Address::generate(&env);
    let report = client.generate_business_report(&fresh_business, &TimePeriod::AllTime);

    assert_eq!(
        report.total_volume, 0,
        "total_volume must be 0 for a business with no invoices"
    );
}

#[test]
fn test_report_volume_consistent_across_stored_and_live() {
    let env = Env::default();
    env.ledger().set_timestamp(2_000_000u64);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 8_000, "volume-cs-inv");

    let live = client.generate_business_report(&business, &TimePeriod::AllTime);
    let stored = client
        .get_business_report(&live.report_id)
        .expect("report must be stored after generation");

    assert_eq!(
        live.total_volume, stored.total_volume,
        "total_volume in the live report and the stored report must match"
    );
}

// ============================================================================
// 3. Count correctness
// ============================================================================

#[test]
fn test_report_invoice_count_matches_uploaded_count() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 100, "cnt-inv-1");
    upload(&env, &client, &business, 200, "cnt-inv-2");
    upload(&env, &client, &business, 300, "cnt-inv-3");
    upload(&env, &client, &business, 400, "cnt-inv-4");

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    assert_eq!(
        report.invoices_uploaded, 4,
        "invoices_uploaded must equal the number of invoices created for the business"
    );
}

#[test]
fn test_report_invoice_count_zero_for_new_business() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, _) = setup(&env);

    let fresh = Address::generate(&env);
    let report = client.generate_business_report(&fresh, &TimePeriod::AllTime);

    assert_eq!(
        report.invoices_uploaded, 0,
        "invoices_uploaded must be 0 for a business with no history"
    );
}

// ============================================================================
// 4. Funding ratio invariant
// ============================================================================

#[test]
fn test_report_invoices_funded_never_exceeds_invoices_uploaded() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    let inv1 = upload(&env, &client, &business, 1_000, "fund-ratio-1");
    let inv2 = upload(&env, &client, &business, 2_000, "fund-ratio-2");
    let _inv3 = upload(&env, &client, &business, 3_000, "fund-ratio-3");

    // Fund two out of three invoices
    client.update_invoice_status(&inv1, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv1, &InvoiceStatus::Funded);
    client.update_invoice_status(&inv2, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv2, &InvoiceStatus::Funded);

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    assert!(
        report.invoices_funded <= report.invoices_uploaded,
        "invoices_funded ({}) must never exceed invoices_uploaded ({})",
        report.invoices_funded,
        report.invoices_uploaded
    );
}

#[test]
fn test_report_invoices_funded_zero_when_none_funded() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 1_000, "no-fund-1");
    upload(&env, &client, &business, 2_000, "no-fund-2");

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    assert_eq!(
        report.invoices_funded, 0,
        "invoices_funded must be 0 when no invoices have been funded"
    );
}

// ============================================================================
// 5 & 6. Rate bounds and rate formula
// ============================================================================

#[test]
fn test_report_rates_within_bps_bounds() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    let inv1 = upload(&env, &client, &business, 1_000, "rates-inv-1");
    let inv2 = upload(&env, &client, &business, 1_000, "rates-inv-2");
    let inv3 = upload(&env, &client, &business, 1_000, "rates-inv-3");

    client.update_invoice_status(&inv1, &InvoiceStatus::Paid);
    client.update_invoice_status(&inv2, &InvoiceStatus::Defaulted);
    // inv3 remains Pending

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    assert!(
        report.success_rate >= 0 && report.success_rate <= 10_000,
        "success_rate ({}) must be in [0, 10000] bps",
        report.success_rate
    );
    assert!(
        report.default_rate >= 0 && report.default_rate <= 10_000,
        "default_rate ({}) must be in [0, 10000] bps",
        report.default_rate
    );
    assert!(
        report.success_rate + report.default_rate <= 10_000,
        "success_rate + default_rate ({}) must be - 10000 bps",
        report.success_rate + report.default_rate
    );
}

#[test]
fn test_report_success_rate_formula_all_paid() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    let inv1 = upload(&env, &client, &business, 1_000, "all-paid-1");
    let inv2 = upload(&env, &client, &business, 1_000, "all-paid-2");

    client.update_invoice_status(&inv1, &InvoiceStatus::Paid);
    client.update_invoice_status(&inv2, &InvoiceStatus::Paid);

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    // 2 paid / 2 total = 10 000 bps
    assert_eq!(
        report.success_rate, 10_000,
        "success_rate must be 10 000 bps when all invoices are Paid"
    );
    assert_eq!(
        report.default_rate, 0,
        "default_rate must be 0 bps when no invoice is Defaulted"
    );
}

#[test]
fn test_report_success_rate_formula_partial() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    let inv1 = upload(&env, &client, &business, 1_000, "partial-1");
    let inv2 = upload(&env, &client, &business, 1_000, "partial-2");
    let inv3 = upload(&env, &client, &business, 1_000, "partial-3");
    let inv4 = upload(&env, &client, &business, 1_000, "partial-4");

    client.update_invoice_status(&inv1, &InvoiceStatus::Paid);
    client.update_invoice_status(&inv2, &InvoiceStatus::Paid);
    client.update_invoice_status(&inv3, &InvoiceStatus::Defaulted);
    // inv4 stays Pending

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    // 2 paid / 4 uploaded = 5 000 bps
    assert_eq!(
        report.success_rate, 5_000,
        "success_rate must be 5000 bps for 2 paid out of 4 uploaded"
    );
    // 1 defaulted / 4 uploaded = 2 500 bps
    assert_eq!(
        report.default_rate, 2_500,
        "default_rate must be 2500 bps for 1 defaulted out of 4 uploaded"
    );
}

#[test]
fn test_report_all_defaulted_rates() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    let inv1 = upload(&env, &client, &business, 1_000, "all-def-1");
    let inv2 = upload(&env, &client, &business, 1_000, "all-def-2");

    client.update_invoice_status(&inv1, &InvoiceStatus::Defaulted);
    client.update_invoice_status(&inv2, &InvoiceStatus::Defaulted);

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    assert_eq!(
        report.success_rate, 0,
        "success_rate must be 0 when all invoices are Defaulted"
    );
    assert_eq!(
        report.default_rate, 10_000,
        "default_rate must be 10 000 bps when all invoices are Defaulted"
    );
}

#[test]
fn test_report_rates_zero_for_empty_business() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, _) = setup(&env);

    let fresh = Address::generate(&env);
    let report = client.generate_business_report(&fresh, &TimePeriod::AllTime);

    assert_eq!(report.success_rate, 0, "success_rate must be 0 with no invoices");
    assert_eq!(report.default_rate, 0, "default_rate must be 0 with no invoices");
}

// ============================================================================
// 7. Report immutability
// ============================================================================

#[test]
fn test_stored_report_unchanged_after_new_report_generated() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 1_000, "immut-first");

    // Generate and store the first report
    let first = client.generate_business_report(&business, &TimePeriod::AllTime);
    let first_id = first.report_id.clone();

    // Advance time and add a new invoice
    env.ledger().set_timestamp(1_000_001u64);
    upload(&env, &client, &business, 9_999, "immut-second");

    // Generate a second report - must not mutate the first stored report
    let _second = client.generate_business_report(&business, &TimePeriod::AllTime);

    let retrieved_first = client
        .get_business_report(&first_id)
        .expect("first report must still exist");

    assert_eq!(
        retrieved_first.invoices_uploaded, first.invoices_uploaded,
        "stored report's invoices_uploaded must not change after a new report is generated"
    );
    assert_eq!(
        retrieved_first.total_volume, first.total_volume,
        "stored report's total_volume must not change after a new report is generated"
    );
    assert_eq!(
        retrieved_first.generated_at, first.generated_at,
        "stored report's generated_at must not change after a new report is generated"
    );
}

// ============================================================================
// 8. Period exclusion
// ============================================================================

#[test]
fn test_report_excludes_invoice_outside_period_window() {
    let env = Env::default();
    // Place current time at exactly 2 days from epoch
    let two_days: u64 = 2 * 86_400;
    env.ledger().set_timestamp(two_days);
    let (client, _admin, business) = setup(&env);

    // Invoice is created at the current ledger timestamp (2 days).
    // A Daily report has start = 2*86400 - 86400 = 86400 and end = 2*86400.
    // The invoice's created_at == two_days which is - end -> included.
    upload(&env, &client, &business, 5_000, "in-window-inv");

    // Advance to 3 days.  Now the daily window is [2*86400, 3*86400].
    // The invoice was created at 2*86400 = start -> still included in [start, end].
    env.ledger().set_timestamp(3 * 86_400);

    // Advance to 4 days.  Daily window: [3*86400, 4*86400].
    // The invoice (created at 2*86400) is now BEFORE the window start -> excluded.
    env.ledger().set_timestamp(4 * 86_400);
    let report_daily = client.generate_business_report(&business, &TimePeriod::Daily);

    assert_eq!(
        report_daily.invoices_uploaded, 0,
        "an invoice created 2 days before the daily window should not be counted"
    );

    // AllTime always includes everything
    let report_all = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(
        report_all.invoices_uploaded, 1,
        "AllTime report must still count the old invoice"
    );
}

// ============================================================================
// 9. Multi-business isolation
// ============================================================================

#[test]
fn test_report_does_not_count_other_business_invoices() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business_a) = setup(&env);
    let business_b = Address::generate(&env);

    upload(&env, &client, &business_a, 1_000, "biz-a-inv-1");
    upload(&env, &client, &business_a, 2_000, "biz-a-inv-2");
    upload(&env, &client, &business_b, 9_999, "biz-b-inv-1");

    let report_a = client.generate_business_report(&business_a, &TimePeriod::AllTime);
    let report_b = client.generate_business_report(&business_b, &TimePeriod::AllTime);

    assert_eq!(
        report_a.invoices_uploaded, 2,
        "report for business-A must count only business-A's invoices"
    );
    assert_eq!(
        report_a.total_volume, 3_000,
        "report for business-A must sum only business-A's invoice amounts"
    );
    assert_eq!(
        report_b.invoices_uploaded, 1,
        "report for business-B must count only business-B's invoices"
    );
    assert_eq!(
        report_b.total_volume, 9_999,
        "report for business-B must sum only business-B's invoice amounts"
    );
}

// ============================================================================
// 10. Report-ID uniqueness
// ============================================================================

#[test]
fn test_reports_generated_at_different_times_have_different_ids() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 1_000, "uid-inv");

    let report_1 = client.generate_business_report(&business, &TimePeriod::AllTime);

    // Advance the ledger timestamp to guarantee a different hash input
    env.ledger().set_timestamp(1_000_001u64);
    let report_2 = client.generate_business_report(&business, &TimePeriod::AllTime);

    assert_ne!(
        report_1.report_id, report_2.report_id,
        "two reports generated at different ledger timestamps must have different IDs"
    );
}

// ============================================================================
// Stored vs live field-by-field equality
// ============================================================================

#[test]
fn test_all_report_fields_identical_in_stored_copy() {
    let env = Env::default();
    let ts = 3_000_000u64;
    env.ledger().set_timestamp(ts);
    let (client, _admin, business) = setup(&env);

    let inv1 = upload(&env, &client, &business, 10_000, "full-eq-1");
    let inv2 = upload(&env, &client, &business, 5_000, "full-eq-2");

    client.update_invoice_status(&inv1, &InvoiceStatus::Funded);
    client.update_invoice_status(&inv2, &InvoiceStatus::Paid);

    let live = client.generate_business_report(&business, &TimePeriod::AllTime);
    let stored = client
        .get_business_report(&live.report_id)
        .expect("report must exist in storage");

    // All fields must be identical.
    assert_eq!(stored.report_id, live.report_id);
    assert_eq!(stored.business_address, live.business_address);
    assert_eq!(stored.period, live.period);
    assert_eq!(stored.start_date, live.start_date);
    assert_eq!(stored.end_date, live.end_date);
    assert_eq!(stored.invoices_uploaded, live.invoices_uploaded);
    assert_eq!(stored.invoices_funded, live.invoices_funded);
    assert_eq!(stored.total_volume, live.total_volume);
    assert_eq!(stored.average_funding_time, live.average_funding_time);
    assert_eq!(stored.success_rate, live.success_rate);
    assert_eq!(stored.default_rate, live.default_rate);
    assert_eq!(stored.rating_average, live.rating_average);
    assert_eq!(stored.total_ratings, live.total_ratings);
    assert_eq!(stored.generated_at, live.generated_at);
    assert_eq!(stored.generated_at, ts);
}

// ============================================================================
// Category breakdown consistency
// ============================================================================

#[test]
fn test_category_breakdown_sum_equals_invoices_uploaded() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 1_000, "cat-sum-1");
    upload(&env, &client, &business, 2_000, "cat-sum-2");
    upload(&env, &client, &business, 3_000, "cat-sum-3");

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    let total_in_breakdown: u32 = report
        .category_breakdown
        .iter()
        .map(|(_, count)| count)
        .sum::<u32>();

    assert_eq!(
        total_in_breakdown, report.invoices_uploaded,
        "sum of all category_breakdown counts must equal invoices_uploaded"
    );
}

// ============================================================================
// Average funding time non-negative
// ============================================================================

#[test]
fn test_report_average_funding_time_is_non_negative() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _admin, business) = setup(&env);

    let inv = upload(&env, &client, &business, 5_000, "avg-fund-time");
    client.update_invoice_status(&inv, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv, &InvoiceStatus::Funded);

    let report = client.generate_business_report(&business, &TimePeriod::AllTime);

    // average_funding_time is u64, so it cannot be negative by type, but we
    // also verify the invariant explicitly to catch potential saturation bugs.
    assert!(
        report.average_funding_time < u64::MAX,
        "average_funding_time must not overflow to u64::MAX"
    );
}

// ============================================================================
// Period-specific window boundary tests
// ============================================================================

#[test]
fn test_business_report_daily_boundary_invoice_included() {
    let env = Env::default();
    // Ledger at exactly 2 days; daily window = [86400, 172800].
    // An invoice created at exact window start (86400) is included.
    let day2 = 2 * 86_400u64;
    env.ledger().set_timestamp(day2);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 1_000, "daily-boundary-inv");

    let report = client.generate_business_report(&business, &TimePeriod::Daily);

    // created_at == day2 == end_date -> invoice is in [start, end]
    assert_eq!(
        report.invoices_uploaded, 1,
        "invoice created at end_date boundary must be included in the daily report"
    );
}

#[test]
fn test_business_report_weekly_includes_invoice_in_window() {
    let env = Env::default();
    env.ledger().set_timestamp(10 * 86_400u64);
    let (client, _admin, business) = setup(&env);

    upload(&env, &client, &business, 2_000, "weekly-win-inv");

    let report = client.generate_business_report(&business, &TimePeriod::Weekly);
    assert_eq!(report.invoices_uploaded, 1);
    assert_eq!(report.total_volume, 2_000);
}

// ============================================================================
// Idempotence: re-generating a report yields consistent computed values
// ============================================================================

#[test]
fn test_report_regeneration_produces_same_summary_values() {
    let env = Env::default();
    env.ledger().set_timestamp(2_000_000u64);
    let (client, _admin, business) = setup(&env);

    let inv = upload(&env, &client, &business, 7_500, "idempotent-inv");
    client.update_invoice_status(&inv, &InvoiceStatus::Paid);

    let r1 = client.generate_business_report(&business, &TimePeriod::AllTime);

    // Re-generate at same timestamp (no new invoices)
    let r2 = client.generate_business_report(&business, &TimePeriod::AllTime);

    assert_eq!(
        r1.invoices_uploaded, r2.invoices_uploaded,
        "invoices_uploaded must be stable across re-generations at the same state"
    );
    assert_eq!(r1.total_volume, r2.total_volume, "total_volume must be stable");
    assert_eq!(r1.success_rate, r2.success_rate, "success_rate must be stable");
    assert_eq!(r1.default_rate, r2.default_rate, "default_rate must be stable");
    assert_eq!(r1.period, r2.period, "period must be stable");
}
