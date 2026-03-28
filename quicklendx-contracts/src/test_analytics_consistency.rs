/// Business Report Generation Consistency Checks (Issue #598)
///
/// Validates that every `BusinessReport` produced by `generate_business_report`
/// satisfies the invariants documented in docs/contracts/analytics.md.
///
/// Test inventory (15 invariants):
///  1. generated_at == ledger timestamp at generation time
///  2. start_date ≤ end_date for every TimePeriod variant
///  3. end_date == ledger timestamp
///  4. start_date / end_date match AnalyticsCalculator::get_period_dates
///  5. total_volume == Σ invoice.amount for invoices in the period
///  6. total_volume == 0 for a business with no invoices
///  7. total_volume consistent between the live return and the stored copy
///  8. invoices_uploaded == number of invoices created in the period
///  9. invoices_uploaded == 0 for a new business
/// 10. invoices_funded ≤ invoices_uploaded
/// 11. invoices_funded == 0 when no invoice is funded
/// 12. success_rate ∈ [0, 10000] bps, default_rate ∈ [0, 10000] bps, sum ≤ 10000
/// 13. success_rate formula — all-paid case: 10 000 bps
/// 14. success_rate formula — partial case: exact bps
/// 15. all-defaulted case: success_rate == 0, default_rate == 10 000
/// 16. rates == 0 for an empty business
/// 17. stored report unchanged after a new report is generated (immutability)
/// 18. invoices outside the period window are excluded
/// 19. a report for business-A does not count business-B's invoices (isolation)
/// 20. two reports at different ledger timestamps have different IDs (uniqueness)
/// 21. all fields in the stored copy match the live return value
/// 22. Σ category_breakdown counts == invoices_uploaded
/// 23. average_funding_time does not overflow u64::MAX
/// 24. daily boundary — invoice created at end_date is included
/// 25. re-generating a report yields identical computed summaries (idempotence)
use super::*;
use crate::analytics::{AnalyticsCalculator, TimePeriod};
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

// ─── helpers ────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (QuickLendXContractClient<'_>, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let business = Address::generate(env);
    env.mock_all_auths();
    client.set_admin(&admin);
    (client, admin, business)
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
    )
}

// ─── 1. generated_at correctness ────────────────────────────────────────────

#[test]
fn test_report_generated_at_equals_ledger_timestamp() {
    let env = Env::default();
    let ts = 5_000_000u64;
    env.ledger().set_timestamp(ts);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 1_000, "ts-test");
    let report = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(report.generated_at, ts);
}

// ─── 2. start_date ≤ end_date ───────────────────────────────────────────────

#[test]
fn test_report_start_date_le_end_date_all_periods() {
    let env = Env::default();
    env.ledger().set_timestamp(10_000_000u64);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 500, "bounds-test");
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
        let r = client.generate_business_report(&business, period);
        assert!(r.start_date <= r.end_date);
    }
}

// ─── 3. end_date == ledger ts ────────────────────────────────────────────────

#[test]
fn test_report_end_date_equals_ledger_ts() {
    let env = Env::default();
    let ts = 8_000_000u64;
    env.ledger().set_timestamp(ts);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 1_000, "end-test");
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
        assert_eq!(
            client.generate_business_report(&business, period).end_date,
            ts
        );
    }
}

// ─── 4. period dates match calculator ───────────────────────────────────────

#[test]
fn test_report_period_dates_match_calculator() {
    let env = Env::default();
    let ts = 100_000_000u64;
    env.ledger().set_timestamp(ts);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 1_000, "calc-match");
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
        let (exp_start, exp_end) = AnalyticsCalculator::get_period_dates(ts, period.clone());
        let r = client.generate_business_report(&business, period);
        assert_eq!(r.start_date, exp_start);
        assert_eq!(r.end_date, exp_end);
    }
}

// ─── 5. total_volume == Σ invoice amounts ───────────────────────────────────

#[test]
fn test_report_total_volume_equals_sum_of_amounts() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 1_000, "vol-1");
    upload(&env, &client, &business, 2_500, "vol-2");
    upload(&env, &client, &business, 4_000, "vol-3");
    let r = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(r.total_volume, 7_500);
}

// ─── 6. zero volume for empty business ──────────────────────────────────────

#[test]
fn test_report_zero_volume_for_empty_business() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, _) = setup(&env);
    let fresh = Address::generate(&env);
    let r = client.generate_business_report(&fresh, &TimePeriod::AllTime);
    assert_eq!(r.total_volume, 0);
}

// ─── 7. volume consistent between live and stored ───────────────────────────

#[test]
fn test_report_volume_live_equals_stored() {
    let env = Env::default();
    env.ledger().set_timestamp(2_000_000u64);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 8_000, "vol-cs");
    let live = client.generate_business_report(&business, &TimePeriod::AllTime);
    let stored = client.get_business_report(&live.report_id).unwrap();
    assert_eq!(live.total_volume, stored.total_volume);
}

// ─── 8. invoices_uploaded count ─────────────────────────────────────────────

#[test]
fn test_report_uploaded_count_correct() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 100, "cnt-1");
    upload(&env, &client, &business, 200, "cnt-2");
    upload(&env, &client, &business, 300, "cnt-3");
    upload(&env, &client, &business, 400, "cnt-4");
    let r = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(r.invoices_uploaded, 4);
}

// ─── 9. zero count for new business ─────────────────────────────────────────

#[test]
fn test_report_zero_count_for_new_business() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, _) = setup(&env);
    let fresh = Address::generate(&env);
    assert_eq!(
        client
            .generate_business_report(&fresh, &TimePeriod::AllTime)
            .invoices_uploaded,
        0
    );
}

// ─── 10. invoices_funded ≤ invoices_uploaded ────────────────────────────────

#[test]
fn test_report_funded_le_uploaded() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    let inv1 = upload(&env, &client, &business, 1_000, "fund-1");
    let inv2 = upload(&env, &client, &business, 2_000, "fund-2");
    let _inv3 = upload(&env, &client, &business, 3_000, "fund-3");
    client.update_invoice_status(&inv1, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv1, &InvoiceStatus::Funded);
    client.update_invoice_status(&inv2, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv2, &InvoiceStatus::Funded);
    let r = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert!(r.invoices_funded <= r.invoices_uploaded);
}

// ─── 11. invoices_funded == 0 when nothing funded ───────────────────────────

#[test]
fn test_report_funded_zero_when_none_funded() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 1_000, "no-fund-1");
    upload(&env, &client, &business, 2_000, "no-fund-2");
    let r = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(r.invoices_funded, 0);
}

// ─── 12. rate bounds ────────────────────────────────────────────────────────

#[test]
fn test_report_rates_within_bps_bounds() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    let inv1 = upload(&env, &client, &business, 1_000, "rb-1");
    let inv2 = upload(&env, &client, &business, 1_000, "rb-2");
    let _inv3 = upload(&env, &client, &business, 1_000, "rb-3");
    client.update_invoice_status(&inv1, &InvoiceStatus::Paid);
    client.update_invoice_status(&inv2, &InvoiceStatus::Defaulted);
    let r = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert!(r.success_rate >= 0 && r.success_rate <= 10_000);
    assert!(r.default_rate >= 0 && r.default_rate <= 10_000);
    assert!(r.success_rate + r.default_rate <= 10_000);
}

// ─── 13. all-paid → 10 000 bps ──────────────────────────────────────────────

#[test]
fn test_report_success_rate_formula_all_paid() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    let inv1 = upload(&env, &client, &business, 1_000, "ap-1");
    let inv2 = upload(&env, &client, &business, 1_000, "ap-2");
    client.update_invoice_status(&inv1, &InvoiceStatus::Paid);
    client.update_invoice_status(&inv2, &InvoiceStatus::Paid);
    let r = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(r.success_rate, 10_000);
    assert_eq!(r.default_rate, 0);
}

// ─── 14. partial case ───────────────────────────────────────────────────────

#[test]
fn test_report_success_rate_formula_partial() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    let inv1 = upload(&env, &client, &business, 1_000, "pa-1");
    let inv2 = upload(&env, &client, &business, 1_000, "pa-2");
    let inv3 = upload(&env, &client, &business, 1_000, "pa-3");
    let _inv4 = upload(&env, &client, &business, 1_000, "pa-4");
    client.update_invoice_status(&inv1, &InvoiceStatus::Paid);
    client.update_invoice_status(&inv2, &InvoiceStatus::Paid);
    client.update_invoice_status(&inv3, &InvoiceStatus::Defaulted);
    let r = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(r.success_rate, 5_000); // 2/4
    assert_eq!(r.default_rate, 2_500); // 1/4
}

// ─── 15. all-defaulted ──────────────────────────────────────────────────────

#[test]
fn test_report_all_defaulted() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    let inv1 = upload(&env, &client, &business, 1_000, "ad-1");
    let inv2 = upload(&env, &client, &business, 1_000, "ad-2");
    client.update_invoice_status(&inv1, &InvoiceStatus::Defaulted);
    client.update_invoice_status(&inv2, &InvoiceStatus::Defaulted);
    let r = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(r.success_rate, 0);
    assert_eq!(r.default_rate, 10_000);
}

// ─── 16. rates zero for empty business ──────────────────────────────────────

#[test]
fn test_report_rates_zero_for_empty_business() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, _) = setup(&env);
    let fresh = Address::generate(&env);
    let r = client.generate_business_report(&fresh, &TimePeriod::AllTime);
    assert_eq!(r.success_rate, 0);
    assert_eq!(r.default_rate, 0);
}

// ─── 17. report immutability ─────────────────────────────────────────────────

#[test]
fn test_stored_report_unchanged_after_newer_report() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 1_000, "immut-1");
    let first = client.generate_business_report(&business, &TimePeriod::AllTime);
    let first_id = first.report_id.clone();
    env.ledger().set_timestamp(1_000_001u64);
    upload(&env, &client, &business, 9_999, "immut-2");
    let _second = client.generate_business_report(&business, &TimePeriod::AllTime);
    let stored = client.get_business_report(&first_id).unwrap();
    assert_eq!(stored.invoices_uploaded, first.invoices_uploaded);
    assert_eq!(stored.total_volume, first.total_volume);
    assert_eq!(stored.generated_at, first.generated_at);
}

// ─── 18. period exclusion ────────────────────────────────────────────────────

#[test]
fn test_report_excludes_out_of_window_invoice() {
    let env = Env::default();
    // Create an invoice at 2*day; then advance to 4*day so it falls
    // outside the daily window [3*day, 4*day].
    let day = 86_400u64;
    env.ledger().set_timestamp(2 * day);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 5_000, "old-inv");

    env.ledger().set_timestamp(4 * day);
    let daily = client.generate_business_report(&business, &TimePeriod::Daily);
    assert_eq!(daily.invoices_uploaded, 0);

    let all_time = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(all_time.invoices_uploaded, 1);
}

// ─── 19. multi-business isolation ────────────────────────────────────────────

#[test]
fn test_report_business_isolation() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, biz_a) = setup(&env);
    let biz_b = Address::generate(&env);
    upload(&env, &client, &biz_a, 1_000, "a-1");
    upload(&env, &client, &biz_a, 2_000, "a-2");
    upload(&env, &client, &biz_b, 9_999, "b-1");
    let ra = client.generate_business_report(&biz_a, &TimePeriod::AllTime);
    let rb = client.generate_business_report(&biz_b, &TimePeriod::AllTime);
    assert_eq!(ra.invoices_uploaded, 2);
    assert_eq!(ra.total_volume, 3_000);
    assert_eq!(rb.invoices_uploaded, 1);
    assert_eq!(rb.total_volume, 9_999);
}

// ─── 20. report-ID uniqueness ────────────────────────────────────────────────

#[test]
fn test_report_ids_unique_across_ledger_timestamps() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 1_000, "uid-inv");
    let r1 = client.generate_business_report(&business, &TimePeriod::AllTime);
    env.ledger().set_timestamp(1_000_001u64);
    let r2 = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_ne!(r1.report_id, r2.report_id);
}

// ─── 21. all stored fields equal live ────────────────────────────────────────

#[test]
fn test_all_stored_fields_match_live() {
    let env = Env::default();
    let ts = 3_000_000u64;
    env.ledger().set_timestamp(ts);
    let (client, _, business) = setup(&env);
    let inv1 = upload(&env, &client, &business, 10_000, "fe-1");
    let inv2 = upload(&env, &client, &business, 5_000, "fe-2");
    client.update_invoice_status(&inv1, &InvoiceStatus::Funded);
    client.update_invoice_status(&inv2, &InvoiceStatus::Paid);
    let live = client.generate_business_report(&business, &TimePeriod::AllTime);
    let stored = client.get_business_report(&live.report_id).unwrap();
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
    assert_eq!(stored.generated_at, ts);
}

// ─── 22. category breakdown sum == invoices_uploaded ────────────────────────

#[test]
fn test_category_breakdown_sum_equals_uploaded() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 1_000, "cat-1");
    upload(&env, &client, &business, 2_000, "cat-2");
    upload(&env, &client, &business, 3_000, "cat-3");
    let r = client.generate_business_report(&business, &TimePeriod::AllTime);
    let breakdown_sum: u32 = r.category_breakdown.iter().map(|(_, c)| c).sum::<u32>();
    assert_eq!(breakdown_sum, r.invoices_uploaded);
}

// ─── 23. average_funding_time not overflowed ────────────────────────────────

#[test]
fn test_report_avg_funding_time_not_overflowed() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    let inv = upload(&env, &client, &business, 5_000, "aft-inv");
    client.update_invoice_status(&inv, &InvoiceStatus::Verified);
    client.update_invoice_status(&inv, &InvoiceStatus::Funded);
    let r = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert!(r.average_funding_time < u64::MAX);
}

// ─── 24. invoice at end_date boundary is included ────────────────────────────

#[test]
fn test_report_daily_boundary_invoice_included() {
    let env = Env::default();
    let day2 = 2 * 86_400u64;
    env.ledger().set_timestamp(day2);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 1_000, "boundary-inv");
    // created_at == end_date → included in the daily window
    let r = client.generate_business_report(&business, &TimePeriod::Daily);
    assert_eq!(r.invoices_uploaded, 1);
}

// ─── 25. idempotence ─────────────────────────────────────────────────────────

#[test]
fn test_report_regeneration_idempotent() {
    let env = Env::default();
    env.ledger().set_timestamp(2_000_000u64);
    let (client, _, business) = setup(&env);
    let inv = upload(&env, &client, &business, 7_500, "idemp-inv");
    client.update_invoice_status(&inv, &InvoiceStatus::Paid);
    let r1 = client.generate_business_report(&business, &TimePeriod::AllTime);
    let r2 = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert_eq!(r1.invoices_uploaded, r2.invoices_uploaded);
    assert_eq!(r1.total_volume, r2.total_volume);
    assert_eq!(r1.success_rate, r2.success_rate);
    assert_eq!(r1.default_rate, r2.default_rate);
    assert_eq!(r1.period, r2.period);
}

// ─── get_business_report returns None for unknown ID ─────────────────────────

#[test]
fn test_get_business_report_none_for_unknown_id() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    let fake_id = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
    assert!(client.get_business_report(&fake_id).is_none());
}

// ─── get_business_report returns Some after generate ─────────────────────────

#[test]
fn test_get_business_report_some_after_generate() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, business) = setup(&env);
    upload(&env, &client, &business, 5_000, "ga-inv");
    let report = client.generate_business_report(&business, &TimePeriod::AllTime);
    assert!(client.get_business_report(&report.report_id).is_some());
}

// ─── get_investor_report returns None for unknown ID ─────────────────────────

#[test]
fn test_get_investor_report_none_for_unknown_id() {
    let env = Env::default();
    let (client, _, _) = setup(&env);
    let fake_id = soroban_sdk::BytesN::from_array(&env, &[0u8; 32]);
    assert!(client.get_investor_report(&fake_id).is_none());
}

// ─── get_investor_report returns Some after generate ─────────────────────────

#[test]
fn test_get_investor_report_some_after_generate() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, _, _) = setup(&env);
    let investor = Address::generate(&env);
    let report = client.generate_investor_report(&investor, &TimePeriod::AllTime);
    assert!(client.get_investor_report(&report.report_id).is_some());
}

// ─── investor report fields match when stored ────────────────────────────────

#[test]
fn test_investor_report_stored_matches_live() {
    let env = Env::default();
    let ts = 2_000_000u64;
    env.ledger().set_timestamp(ts);
    let (client, _, _) = setup(&env);
    let investor = Address::generate(&env);
    let live = client.generate_investor_report(&investor, &TimePeriod::AllTime);
    let stored = client.get_investor_report(&live.report_id).unwrap();
    assert_eq!(stored.report_id, live.report_id);
    assert_eq!(stored.investor_address, investor);
    assert_eq!(stored.period, live.period);
    assert_eq!(stored.start_date, live.start_date);
    assert_eq!(stored.end_date, live.end_date);
    assert_eq!(stored.investments_made, live.investments_made);
    assert_eq!(stored.total_invested, live.total_invested);
    assert_eq!(stored.generated_at, ts);
}
