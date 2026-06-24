//! Reconciliation tests for PlatformMetrics.
//!
//! These tests independently recompute platform aggregates from the
//! underlying invoice and investment records and assert equality with
//! AnalyticsCalculator::calculate_platform_metrics.

#![cfg(test)]

extern crate alloc;

use crate::analytics::{AnalyticsCalculator, PlatformMetrics};
use crate::{QuickLendXContract, QuickLendXContractClient};
use crate::investment::InvestmentStorage;
use crate::storage::InvoiceStorage;
use crate::types::{Investment, InvestmentStatus, InvoiceCategory, InvoiceStatus};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec, BytesN,
};

fn setup(env: &Env) -> (QuickLendXContractClient<'_>, Address, Address, Address) {
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let business = Address::generate(env);

    env.mock_all_auths();

    env.as_contract(&contract_id, || {
        crate::admin::AdminStorage::initialize(env, &admin).unwrap();
    });
    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(env, "kyc"));
    client.verify_business(&admin, &business);

    (client, contract_id, admin, business)
}

fn upload(
    env: &Env,
    client: &QuickLendXContractClient,
    business: &Address,
    amount: i128,
    desc: &str,
) -> BytesN<32> {
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

fn store_investment(
    env: &Env,
    contract_id: &Address,
    invoice_id: &BytesN<32>,
    investor: &Address,
    amount: i128,
    status: InvestmentStatus,
) -> BytesN<32> {
    env.as_contract(contract_id, || {
        let investment_id = InvestmentStorage::generate_unique_investment_id(env);
        let investment = Investment {
            investment_id: investment_id.clone(),
            invoice_id: invoice_id.clone(),
            investor: investor.clone(),
            amount,
            funded_at: env.ledger().timestamp(),
            status,
            insurance: Vec::new(env),
        };
        InvestmentStorage::store_investment(env, &investment);
        investment_id
    })
}

struct IndependentMetrics {
    total_invoices: u32,
    total_investments: u32,
    total_volume: i128,
    average_invoice_amount: i128,
    average_investment_amount: i128,
    default_rate: i128,
    success_rate: i128,
}

fn compute_independent_metrics(env: &Env, contract_id: &Address) -> IndependentMetrics {
    env.as_contract(contract_id, || {
        let mut all_invoices = alloc::vec::Vec::new();

        let pending = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Pending);
        for id in pending.iter() {
            if let Some(inv) = InvoiceStorage::get_invoice(env, &id) {
                all_invoices.push(inv);
            }
        }
        let verified = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Verified);
        for id in verified.iter() {
            if let Some(inv) = InvoiceStorage::get_invoice(env, &id) {
                all_invoices.push(inv);
            }
        }
        let funded = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Funded);
        for id in funded.iter() {
            if let Some(inv) = InvoiceStorage::get_invoice(env, &id) {
                all_invoices.push(inv);
            }
        }
        let paid = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Paid);
        for id in paid.iter() {
            if let Some(inv) = InvoiceStorage::get_invoice(env, &id) {
                all_invoices.push(inv);
            }
        }
        let defaulted = InvoiceStorage::get_invoices_by_status(env, InvoiceStatus::Defaulted);
        for id in defaulted.iter() {
            if let Some(inv) = InvoiceStorage::get_invoice(env, &id) {
                all_invoices.push(inv);
            }
        }

        let total_invoices = all_invoices.len() as u32;
        let expected_total_volume: i128 = all_invoices.iter().map(|i| i.amount).sum();
        let total_investments = (funded.len() + paid.len() + defaulted.len()) as u32;

        let expected_average_invoice_amount = if total_invoices > 0 {
            // integer division truncates toward zero
            expected_total_volume / (total_invoices as i128)
        } else {
            0
        };

        let mut expected_total_invested = 0i128;
        for inv in all_invoices.iter() {
            if inv.status == InvoiceStatus::Funded || inv.status == InvoiceStatus::Paid || inv.status == InvoiceStatus::Defaulted {
                if let Some(investment) = InvestmentStorage::get_investment_by_invoice(env, &inv.id) {
                    expected_total_invested += investment.amount;
                }
            }
        }

        let expected_average_investment_amount = if total_investments > 0 {
            // integer division truncates toward zero
            expected_total_invested / (total_investments as i128)
        } else {
            0
        };

        // denominator is total_investments, scaled by 10000
        let expected_default_rate = if total_investments > 0 {
            ((defaulted.len() as u32).saturating_mul(10_000)) / total_investments
        } else {
            0
        } as i128;

        let expected_success_rate = if total_investments > 0 {
            ((paid.len() as u32).saturating_mul(10_000)) / total_investments
        } else {
            0
        } as i128;

        IndependentMetrics {
            total_invoices,
            total_investments,
            total_volume: expected_total_volume,
            average_invoice_amount: expected_average_invoice_amount,
            average_investment_amount: expected_average_investment_amount,
            default_rate: expected_default_rate,
            success_rate: expected_success_rate,
        }
    })
}

#[test]
fn test_platform_metrics_reconcile_with_independent_sum() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, contract_id, _, business) = setup(&env);
    let investor = Address::generate(&env);

    upload(&env, &client, &business, 1_000, "inv_pending");

    let inv_active = upload(&env, &client, &business, 2_500, "inv_active");
    client.try_update_invoice_status(&inv_active, &InvoiceStatus::Verified).unwrap();
    client.try_update_invoice_status(&inv_active, &InvoiceStatus::Funded).unwrap();
    store_investment(&env, &contract_id, &inv_active, &investor, 2_500, InvestmentStatus::Active);

    let inv_completed = upload(&env, &client, &business, 3_333, "inv_completed");
    client.try_update_invoice_status(&inv_completed, &InvoiceStatus::Verified).unwrap();
    client.try_update_invoice_status(&inv_completed, &InvoiceStatus::Funded).unwrap();
    store_investment(&env, &contract_id, &inv_completed, &investor, 3_333, InvestmentStatus::Completed);
    client.try_update_invoice_status(&inv_completed, &InvoiceStatus::Paid).unwrap();

    let inv_defaulted = upload(&env, &client, &business, 7_777, "inv_defaulted");
    client.try_update_invoice_status(&inv_defaulted, &InvoiceStatus::Verified).unwrap();
    client.try_update_invoice_status(&inv_defaulted, &InvoiceStatus::Funded).unwrap();
    store_investment(&env, &contract_id, &inv_defaulted, &investor, 7_777, InvestmentStatus::Defaulted);

    env.ledger().set_timestamp(env.ledger().timestamp() + 10_000_000u64); // skip grace period
    client.try_update_invoice_status(&inv_defaulted, &InvoiceStatus::Defaulted).unwrap();

    let inv_cancelled = upload(&env, &client, &business, 5_000, "inv_cancelled");
    client.try_cancel_invoice(&inv_cancelled).unwrap();

    let expected = compute_independent_metrics(&env, &contract_id);
    let metrics = env.as_contract(&contract_id, || {
        AnalyticsCalculator::calculate_platform_metrics(&env).unwrap()
    });

    assert_eq!(metrics.total_invoices, expected.total_invoices);
    assert_eq!(metrics.total_investments, expected.total_investments);
    assert_eq!(metrics.total_volume, expected.total_volume);
    assert_eq!(metrics.average_invoice_amount, expected.average_invoice_amount);
    assert_eq!(metrics.average_investment_amount, expected.average_investment_amount);
    assert_eq!(metrics.default_rate, expected.default_rate);
    assert_eq!(metrics.success_rate, expected.success_rate);
}

#[test]
fn test_default_rate_matches_fixture_ratio() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, contract_id, _, business) = setup(&env);
    let investor = Address::generate(&env);

    let inv1 = upload(&env, &client, &business, 1_000, "inv1");
    let inv2 = upload(&env, &client, &business, 1_000, "inv2");
    let inv3 = upload(&env, &client, &business, 1_000, "inv3");

    for inv in [&inv1, &inv2, &inv3] {
        client.try_update_invoice_status(inv, &InvoiceStatus::Verified).unwrap();
        client.try_update_invoice_status(inv, &InvoiceStatus::Funded).unwrap();
        store_investment(&env, &contract_id, inv, &investor, 1_000, InvestmentStatus::Active);
    }

    env.ledger().set_timestamp(env.ledger().timestamp() + 10_000_000u64);
    client.try_update_invoice_status(&inv1, &InvoiceStatus::Defaulted).unwrap();

    let metrics = env.as_contract(&contract_id, || {
        AnalyticsCalculator::calculate_platform_metrics(&env).unwrap()
    });
    let expected = compute_independent_metrics(&env, &contract_id);

    assert_eq!(expected.default_rate, 3333);
    assert_eq!(metrics.default_rate, expected.default_rate);
}

#[test]
fn test_success_rate_matches_fixture_ratio() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, contract_id, _, business) = setup(&env);
    let investor = Address::generate(&env);

    let inv1 = upload(&env, &client, &business, 1_000, "inv1");
    let inv2 = upload(&env, &client, &business, 1_000, "inv2");
    let inv3 = upload(&env, &client, &business, 1_000, "inv3");

    for inv in [&inv1, &inv2, &inv3] {
        client.try_update_invoice_status(inv, &InvoiceStatus::Verified).unwrap();
        client.try_update_invoice_status(inv, &InvoiceStatus::Funded).unwrap();
        store_investment(&env, &contract_id, inv, &investor, 1_000, InvestmentStatus::Active);
    }

    client.try_update_invoice_status(&inv1, &InvoiceStatus::Paid).unwrap();
    client.try_update_invoice_status(&inv2, &InvoiceStatus::Paid).unwrap();

    let metrics = env.as_contract(&contract_id, || {
        AnalyticsCalculator::calculate_platform_metrics(&env).unwrap()
    });
    let expected = compute_independent_metrics(&env, &contract_id);

    assert_eq!(expected.success_rate, 6666);
    assert_eq!(metrics.success_rate, expected.success_rate);
}

#[test]
fn test_average_invoice_amount_rounding() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, contract_id, _, business) = setup(&env);

    upload(&env, &client, &business, 333, "inv1");
    upload(&env, &client, &business, 333, "inv2");
    upload(&env, &client, &business, 334, "inv3");

    let metrics = env.as_contract(&contract_id, || {
        AnalyticsCalculator::calculate_platform_metrics(&env).unwrap()
    });
    let expected = compute_independent_metrics(&env, &contract_id);

    assert_eq!(expected.average_invoice_amount, 333);
    assert_eq!(metrics.average_invoice_amount, expected.average_invoice_amount);
}

#[test]
fn test_average_investment_amount_rounding() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, contract_id, _, business) = setup(&env);
    let investor = Address::generate(&env);

    let amounts = [166, 166, 166, 167, 167, 168];
    for (i, &amt) in amounts.iter().enumerate() {
        let inv = upload(&env, &client, &business, amt, &alloc::format!("inv{}", i));
        client.try_update_invoice_status(&inv, &InvoiceStatus::Verified).unwrap();
        client.try_update_invoice_status(&inv, &InvoiceStatus::Funded).unwrap();
        store_investment(&env, &contract_id, &inv, &investor, amt, InvestmentStatus::Active);
    }

    let metrics = env.as_contract(&contract_id, || {
        AnalyticsCalculator::calculate_platform_metrics(&env).unwrap()
    });
    let expected = compute_independent_metrics(&env, &contract_id);

    assert_eq!(expected.average_investment_amount, 166);
    assert_eq!(metrics.average_investment_amount, expected.average_investment_amount);
}

#[test]
fn test_empty_platform_metrics_are_zero() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (_, contract_id, _, _) = setup(&env);

    let metrics = env.as_contract(&contract_id, || {
        AnalyticsCalculator::calculate_platform_metrics(&env).unwrap()
    });

    assert_eq!(metrics.total_invoices, 0);
    assert_eq!(metrics.total_volume, 0);
    assert_eq!(metrics.default_rate, 0);
    assert_eq!(metrics.success_rate, 0);
    assert_eq!(metrics.average_invoice_amount, 0);
    assert_eq!(metrics.average_investment_amount, 0);
}

#[test]
fn test_all_invoices_defaulted() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, contract_id, _, business) = setup(&env);
    let investor = Address::generate(&env);

    let inv1 = upload(&env, &client, &business, 1_000, "inv1");
    let inv2 = upload(&env, &client, &business, 2_000, "inv2");

    for inv in [&inv1, &inv2] {
        client.try_update_invoice_status(inv, &InvoiceStatus::Verified).unwrap();
        client.try_update_invoice_status(inv, &InvoiceStatus::Funded).unwrap();
        store_investment(&env, &contract_id, inv, &investor, 1_000, InvestmentStatus::Active);
    }

    env.ledger().set_timestamp(env.ledger().timestamp() + 10_000_000u64);

    for inv in [&inv1, &inv2] {
        client.try_update_invoice_status(inv, &InvoiceStatus::Defaulted).unwrap();
    }

    let metrics = env.as_contract(&contract_id, || {
        AnalyticsCalculator::calculate_platform_metrics(&env).unwrap()
    });
    let expected = compute_independent_metrics(&env, &contract_id);

    assert_eq!(expected.default_rate, 10000);
    assert_eq!(expected.success_rate, 0);
    assert_eq!(metrics.default_rate, expected.default_rate);
    assert_eq!(metrics.success_rate, expected.success_rate);
}

#[test]
fn test_single_invoice() {
    let env = Env::default();
    env.ledger().set_timestamp(1_000_000u64);
    let (client, contract_id, _, business) = setup(&env);
    let investor = Address::generate(&env);

    let inv_amount = 5_432;
    let inv = upload(&env, &client, &business, inv_amount, "inv1");

    client.try_update_invoice_status(&inv, &InvoiceStatus::Verified).unwrap();
    client.try_update_invoice_status(&inv, &InvoiceStatus::Funded).unwrap();
    store_investment(&env, &contract_id, &inv, &investor, inv_amount, InvestmentStatus::Active);

    let metrics = env.as_contract(&contract_id, || {
        AnalyticsCalculator::calculate_platform_metrics(&env).unwrap()
    });
    let expected = compute_independent_metrics(&env, &contract_id);

    assert_eq!(expected.average_invoice_amount, inv_amount);
    assert_eq!(expected.average_investment_amount, inv_amount);
    assert_eq!(metrics.average_invoice_amount, expected.average_invoice_amount);
    assert_eq!(metrics.average_investment_amount, expected.average_investment_amount);
}
