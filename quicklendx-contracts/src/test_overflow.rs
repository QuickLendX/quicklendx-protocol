//! # Overflow and Arithmetic Safety Tests
//!
//! This module provides tests to ensure all arithmetic in the QuickLendX protocol
//! is overflow- and underflow-safe. It targets:
//!
//! - **Volume accumulation**: `UserVolumeData.total_volume` and `transaction_count`
//!   use `saturating_add`; tests verify correct accumulation and saturation at limits.
//! - **Revenue accumulation**: `RevenueData.total_collected`, `pending_distribution`,
//!   and `transaction_count` use `saturating_add`; tests verify no overflow on large amounts.
//! - **Fee calculation at limit**: Platform fee at 0% and 10% (MAX), and with large i128
//!   amounts; `calculate_profit` uses `saturating_sub`/`saturating_mul`/`checked_div`.
//! - **Bid comparison overflow safety**: `BidStorage::compare_bids` uses `saturating_sub`
//!   for profit; tests cover extreme values and underflow-safe cases (expected_return < bid_amount).
//! - **Timestamp boundaries**: `saturating_add` where used (e.g. `Bid::default_expiration`,
//!   invoice grace_deadline, pagination `start + limit`, dispute query range).
//!
//! All tests are designed to run without panic and to assert expected saturated or
//! correct results. See `docs/contracts/arithmetic-safety.md` for the full safety design.

#![cfg(test)]
use core::cmp::Ordering;

use crate::bid::{Bid, BidStatus, BidStorage};
use crate::fees::FeeType;
use crate::invoice::{Invoice, InvoiceCategory};
use crate::profits::{self, calculate_treasury_split, verify_no_dust};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, Map, String, Vec};

fn setup_test() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    client.initialize_admin(&admin);
    client.initialize_fee_system(&admin);

    (env, client, admin)
}

// =============================================================================
// Volume accumulation overflow
// =============================================================================

/// Volume accumulation uses `saturating_add`; adding large amounts twice must not overflow.
#[test]
fn test_volume_accumulation_overflow() {
    let (env, client, _admin) = setup_test();
    let user = Address::generate(&env);

    let large_val = 1_000_000_000_000_000_000i128;
    let _ = client.update_user_transaction_volume(&user, &large_val);
    let _ = client.update_user_transaction_volume(&user, &large_val);

    let volume_data = client.get_user_volume_data(&user);
    assert!(volume_data.total_volume > 0);
    assert_eq!(volume_data.total_volume, large_val * 2);
}

/// Volume saturates at i128::MAX: adding more does not panic and total stays at or below MAX.
#[test]
fn test_volume_accumulation_saturates_at_max() {
    let (env, client, _admin) = setup_test();
    let user = Address::generate(&env);

    let near_max = i128::MAX - 100;
    let _ = client.update_user_transaction_volume(&user, &near_max);
    let _ = client.update_user_transaction_volume(&user, &1000); // would exceed MAX

    let volume_data = client.get_user_volume_data(&user);
    assert_eq!(volume_data.total_volume, i128::MAX);
    assert_eq!(volume_data.transaction_count, 2);
}

// =============================================================================
// Revenue accumulation overflow
// =============================================================================

/// Revenue accumulation uses `saturating_add`; large fees collected twice must not overflow.
#[test]
fn test_revenue_accumulation_overflow() {
    let (env, client, _admin) = setup_test();
    let user = Address::generate(&env);

    let mut fees = Map::new(&env);
    let large_val = 1_000_000_000_000_000_000i128;
    fees.set(FeeType::Platform, large_val);

    let _ = client.collect_transaction_fees(&user, &fees, &large_val);
    let _ = client.collect_transaction_fees(&user, &fees, &large_val);

    let period = env.ledger().timestamp() / 2_592_000;
    let analytics = client.get_fee_analytics(&period);

    assert_eq!(analytics.total_fees, large_val * 2);
}

/// Revenue total_collected saturates at i128::MAX when adding would overflow.
#[test]
fn test_revenue_accumulation_saturates_at_max() {
    let (env, client, _admin) = setup_test();
    let user = Address::generate(&env);

    let near_max = i128::MAX - 50;
    let mut fees = Map::new(&env);
    fees.set(FeeType::Platform, near_max);

    let _ = client.collect_transaction_fees(&user, &fees, &near_max);
    let _ = client.collect_transaction_fees(&user, &fees, &100); // would exceed MAX

    let period = env.ledger().timestamp() / 2_592_000;
    let analytics = client.get_fee_analytics(&period);
    assert_eq!(analytics.total_fees, i128::MAX);
}

// =============================================================================
// Fee calculation at limit
// =============================================================================

/// Fee at 1000 bps (10%): profit * 10% and investor_return + fee == payment (no dust).
#[test]
fn test_fee_calculation_at_limit() {
    let (_env, client, _admin) = setup_test();

    let _ = client.set_platform_fee(&1000);

    let investment = 1_000_000_000;
    let payment = 2_000_000_000;

    let (investor_return, fee) = client.calculate_profit(&investment, &payment);

    assert_eq!(fee, 100_000_000);
    assert_eq!(investor_return, 1_900_000_000);
    assert!(verify_no_dust(investor_return, fee, payment));
}

/// Fee at 0 bps: no fee, investor gets full payment.
#[test]
fn test_fee_calculation_at_zero_bps() {
    let (_env, client, _admin) = setup_test();

    let _ = client.set_platform_fee(&0);

    let investment = 1_000_000_000;
    let payment = 2_000_000_000;

    let (investor_return, fee) = client.calculate_profit(&investment, &payment);

    assert_eq!(fee, 0);
    assert_eq!(investor_return, payment);
    assert!(verify_no_dust(investor_return, fee, payment));
}

/// Fee calculation with very large i128 amounts must not overflow or panic.
#[test]
fn test_fee_calculation_large_amounts_no_overflow() {
    let (env, client, _admin) = setup_test();

    let _ = client.set_platform_fee(&1000);

    let investment = i128::MAX / 2;
    let payment = i128::MAX / 2 + 1_000_000_000;

    let (investor_return, fee) = client.calculate_profit(&investment, &payment);

    assert!(fee >= 0);
    assert!(investor_return >= 0);
    assert!(verify_no_dust(investor_return, fee, payment));
}

/// Payment <= investment: no profit, zero fee, investor gets full payment.
#[test]
fn test_fee_calculation_no_profit() {
    let (_env, client, _admin) = setup_test();

    let _ = client.set_platform_fee(&1000);

    let (investor_return, fee) = client.calculate_profit(&1000, &1000);
    assert_eq!(fee, 0);
    assert_eq!(investor_return, 1000);

    let (investor_return, fee) = client.calculate_profit(&2000, &1000);
    assert_eq!(fee, 0);
    assert_eq!(investor_return, 1000);
}

// =============================================================================
// Bid comparison overflow safety
// =============================================================================

/// compare_bids uses saturating_sub for profit; extreme expected_return must not panic.
#[test]
fn test_compare_bids_safe_overflow() {
    let env = Env::default();
    let bid_id = BytesN::from_array(&env, &[0; 32]);
    let invoice_id = BytesN::from_array(&env, &[0; 32]);
    let investor = Address::generate(&env);

    let bid_amount = 1000;
    let max_return = i128::MAX;

    let bid1 = Bid {
        bid_id: bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount,
        expected_return: max_return,
        timestamp: 100,
        status: BidStatus::Placed,
        expiration_timestamp: 1000,
    };

    let bid2 = Bid {
        expected_return: max_return - 1000,
        ..bid1.clone()
    };

    let result = BidStorage::compare_bids(&bid1, &bid2);
    assert_eq!(result, Ordering::Greater);
}

/// When expected_return < bid_amount, profit = saturating_sub(return, amount) <= 0; no underflow.
#[test]
fn test_compare_bids_underflow_safe() {
    let env = Env::default();
    let bid_id = BytesN::from_array(&env, &[0; 32]);
    let invoice_id = BytesN::from_array(&env, &[0; 32]);
    let investor = Address::generate(&env);

    let bid_a = Bid {
        bid_id: bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount: 10_000,
        expected_return: 5_000,
        timestamp: 100,
        status: BidStatus::Placed,
        expiration_timestamp: 1000,
    };

    let bid_b = Bid {
        bid_amount: 1_000,
        expected_return: 2_000,
        ..bid_a.clone()
    };

    let result = BidStorage::compare_bids(&bid_a, &bid_b);
    assert_eq!(result, Ordering::Less);
}

/// Equal profit: tie-break by expected_return, then bid_amount, then timestamp.
#[test]
fn test_compare_bids_equal_profit_ordering() {
    let env = Env::default();
    let bid_id = BytesN::from_array(&env, &[0; 32]);
    let invoice_id = BytesN::from_array(&env, &[0; 32]);
    let investor = Address::generate(&env);

    let base = Bid {
        bid_id: bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount: 1000,
        expected_return: 2000,
        timestamp: 100,
        status: BidStatus::Placed,
        expiration_timestamp: 1000,
    };

    let same = Bid { ..base.clone() };
    assert_eq!(BidStorage::compare_bids(&base, &same), Ordering::Equal);

    let higher_return = Bid {
        expected_return: 3000,
        bid_amount: 2000,
        ..base.clone()
    };
    assert_eq!(
        BidStorage::compare_bids(&base, &higher_return),
        Ordering::Less
    );
}

// =============================================================================
// Timestamp boundaries (saturating_add where used)
// =============================================================================

/// Bid::default_expiration(now) uses now.saturating_add(DEFAULT_BID_TTL); u64::MAX saturates.
#[test]
fn test_timestamp_bid_default_expiration_saturates() {
    let env = Env::default();
    let result = Bid::default_expiration(u64::MAX);
    assert_eq!(result, u64::MAX);
}

/// Invoice grace_deadline uses due_date.saturating_add(grace_period); boundary test.
#[test]
fn test_timestamp_invoice_grace_deadline_saturates() {
    let env = Env::default();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let due_date = u64::MAX - 100;
    let grace_period = 200u64;

    let inv = Invoice::new(
        &env,
        business,
        10_000,
        currency,
        due_date,
        String::from_str(&env, "Test"),
        InvoiceCategory::Services,
        Vec::new(&env),
    );
    let deadline = inv.unwrap().grace_deadline(grace_period);
    assert_eq!(deadline, u64::MAX);
}

/// Pagination uses start.saturating_add(limit).min(len); large offset + limit must not panic.
#[test]
fn test_timestamp_pagination_overflow_safe() {
    let (env, client, _admin) = setup_test();
    let business = Address::generate(&env);

    let _ = client.get_business_invoices_paged(&business, &None, &(u32::MAX - 5), &10);
}

/// get_total_invoice_count sums status counts with saturating_add; must not panic.
#[test]
fn test_total_invoice_count_saturating() {
    let (env, client, _admin) = setup_test();
    let _ = client.get_total_invoice_count();
}

/// try_store_invoice with max u64 due_date must not panic (may return Err on validation).
#[test]
fn test_timestamp_boundaries() {
    let (env, client, _admin) = setup_test();
    let business = Address::generate(&env);

    let result = client.try_store_invoice(
        &business,
        &10_000,
        &Address::generate(&env),
        &u64::MAX,
        &String::from_str(&env, "Max Time"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );

    let _ = result;
}

// =============================================================================
// Profits module: verify_no_dust and calculate_with_fee_bps at boundaries
// =============================================================================

/// verify_no_dust uses saturating_add; large values must not overflow.
#[test]
fn test_verify_no_dust_large_amounts() {
    let investor_return = i128::MAX / 2;
    let platform_fee = i128::MAX / 2;
    let payment = i128::MAX;
    assert!(verify_no_dust(investor_return, platform_fee, payment));
}

/// calculate_profit (via calculate_with_fee_bps) with fee_bps at 10000 (100%) saturates fee.
#[test]
fn test_profit_fee_bps_max() {
    let (inv, fee) = profits::PlatformFee::calculate_with_fee_bps(1000, 2000, 10_000);
    assert_eq!(fee, 1000);
    assert_eq!(inv, 1000);
    assert!(verify_no_dust(inv, fee, 2000));
}

/// calculate_treasury_split uses saturating_mul and saturating_sub; large amounts must not overflow.
#[test]
fn test_calculate_treasury_split_large_amounts() {
    let platform_fee = i128::MAX / 2;
    let treasury_share_bps = 5000i128; // 50%
    let (treasury, remaining) = calculate_treasury_split(platform_fee, treasury_share_bps);
    assert!(treasury >= 0);
    assert!(remaining >= 0);
    assert_eq!(treasury.saturating_add(remaining), platform_fee);
}

/// Pagination for investor investments: large offset + limit must not panic.
#[test]
fn test_investor_investments_pagination_overflow_safe() {
    let (env, client, _admin) = setup_test();
    let investor = Address::generate(&env);

    let _ = client.get_investor_investments_paged(&investor, &None, &(u32::MAX - 1), &10);
}
