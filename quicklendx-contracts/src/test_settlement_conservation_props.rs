#![cfg(all(test, feature = "fuzz-tests"))]

use crate::fees::FeeType;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::settlement::RepaymentLedger;
use crate::{QuickLendXContract, QuickLendXContractClient};
use alloc::format;
use alloc::vec::Vec as RustVec;
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, ProptestConfig};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, Map, String, Vec as SorobanVec,
};

const MIN_INVOICE_AMOUNT: i128 = 2_000;
const MAX_INVOICE_AMOUNT: i128 = 50_000;
const MAX_PARTIAL_PAYMENTS: usize = 8;
type PaymentScenario = (i128, u32, u32, RustVec<i128>, u32, u32, i128, u32);

fn conservation_config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(32);

    ProptestConfig {
        cases,
        failure_persistence: Some(Box::new(FileFailurePersistence::WithSource(
            "settlement_conservation",
        ))),
        ..ProptestConfig::default()
    }
}

fn payment_scenarios() -> impl Strategy<Value = PaymentScenario> {
    (
        MIN_INVOICE_AMOUNT..=MAX_INVOICE_AMOUNT,
        0u32..=1_000,
        1u32..=100,
        prop::collection::vec(1i128..=MAX_INVOICE_AMOUNT, 1..=MAX_PARTIAL_PAYMENTS),
        0u32..=10_000,
        0u32..=10_000,
        1i128..=MAX_INVOICE_AMOUNT,
        0u32..=50_000,
    )
}

fn assert_ledger_conserves(ledger: &RepaymentLedger) -> Result<(), TestCaseError> {
    prop_assert_eq!(
        ledger.principal + ledger.investor_profit + ledger.platform_fee + ledger.late_penalty,
        ledger.total_paid,
        "repayment buckets must account for every paid stroop"
    );
    Ok(())
}

proptest! {
    #![proptest_config(conservation_config())]

    #[test]
    fn randomized_payment_sequences_conserve_settlement_and_revenue(
        (
            face,
            fee_bps,
            insurance_pct,
            requested_payments,
            treasury_raw,
            developer_raw,
            revenue_total,
            principal_seed,
        )
            in payment_scenarios()
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let treasury = Address::generate(&env);
        let insurance_provider = Address::generate(&env);
        let principal = 1_000 + (principal_seed as i128 % (face - 999));

        client.initialize_admin(&admin);
        client.initialize_fee_system(&admin);
        client.update_platform_fee_bps(&fee_bps);
        client.configure_treasury(&treasury);

        let treasury_share_bps = treasury_raw;
        let developer_share_bps = developer_raw % (10_001 - treasury_share_bps);
        let platform_share_bps = 10_000 - treasury_share_bps - developer_share_bps;
        client.configure_revenue_distribution(
            &admin,
            &treasury,
            &treasury_share_bps,
            &developer_share_bps,
            &platform_share_bps,
            &false,
            &1,
        );

        let mut fees = Map::new(&env);
        fees.set(FeeType::Platform, revenue_total);
        client.collect_transaction_fees(&business, &fees, &revenue_total);
        let current_period = env.ledger().timestamp() / 2_592_000;
        let (treasury_revenue, developer_revenue, platform_revenue) =
            client.distribute_revenue(&admin, &current_period);
        prop_assert_eq!(
            treasury_revenue + developer_revenue + platform_revenue,
            revenue_total
        );
        prop_assert_eq!(
            platform_revenue,
            revenue_total - treasury_revenue - developer_revenue,
            "platform receives the revenue-split rounding remainder"
        );

        let token_admin = Address::generate(&env);
        let currency = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let token_client = token::Client::new(&env, &currency);
        let asset_admin = token::StellarAssetClient::new(&env, &currency);
        let initial_balance = face * 3;
        asset_admin.mint(&business, &initial_balance);
        asset_admin.mint(&investor, &initial_balance);
        let expiration = env.ledger().sequence() + 10_000;
        token_client.approve(&business, &contract_id, &initial_balance, &expiration);
        token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

        client.submit_kyc_application(&business, &String::from_str(&env, "business-kyc"));
        client.verify_business(&admin, &business);
        client.submit_investor_kyc(&investor, &String::from_str(&env, "investor-kyc"));
        client.verify_investor(&investor, &initial_balance);

        let invoice_id = client.store_invoice(
            &business,
            &face,
            &currency,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Settlement conservation property"),
            &InvoiceCategory::Services,
            &SorobanVec::new(&env),
            &None,
        );
        client.verify_invoice(&invoice_id);
        let bid_id = client.place_bid(
            &investor,
            &invoice_id,
            &principal,
            &face,
            &BytesN::from_array(&env, &[0u8; 32]),
        );
        client.accept_bid(&invoice_id, &bid_id);

        let mut investment = client.get_investment(&invoice_id).unwrap();
        let premium = crate::investment::Investment::calculate_premium(principal, insurance_pct);
        prop_assert!(premium > 0);
        investment
            .add_insurance(insurance_provider, insurance_pct, premium)
            .expect("generated insurance coverage must be valid");
        env.as_contract(&contract_id, || {
            crate::investment::InvestmentStorage::update_investment(&env, &investment);
        });
        prop_assert_eq!(
            client
                .get_investment(&invoice_id)
                .unwrap()
                .insurance
                .len(),
            1
        );

        let investor_before = token_client.balance(&investor);
        let treasury_before = token_client.balance(&treasury);
        let mut previous_progress = 0;
        let mut previous_ledger: Option<RepaymentLedger> = None;
        let mut payment_count = 0;

        for (index, requested) in requested_payments.iter().enumerate() {
            let remaining = face - previous_progress;
            if remaining <= 1 {
                break;
            }
            let payment = (*requested).min(remaining - 1);
            client.process_partial_payment(
                &invoice_id,
                &payment,
                &String::from_str(&env, &format!("property-payment-{index}")),
            );
            payment_count += 1;

            let progress = client.get_invoice_progress(&invoice_id).unwrap();
            prop_assert!(progress.total_paid > previous_progress);
            prop_assert_eq!(progress.payment_count, payment_count);
            let ledger = env.as_contract(&contract_id, || {
                crate::settlement::get_repayment_ledger(&env, &invoice_id)
            }).expect("partial payment must persist its repayment ledger");
            prop_assert_eq!(ledger.total_paid, progress.total_paid);
            assert_ledger_conserves(&ledger)?;
            if let Some(previous) = &previous_ledger {
                prop_assert!(ledger.principal >= previous.principal);
                prop_assert!(ledger.investor_profit >= previous.investor_profit);
                prop_assert!(ledger.platform_fee >= previous.platform_fee);
                prop_assert!(ledger.late_penalty >= previous.late_penalty);
            }
            previous_progress = progress.total_paid;
            previous_ledger = Some(ledger);
        }

        let final_payment = face - previous_progress;
        let investment = client.get_investment(&invoice_id).unwrap();
        client.settle_invoice(&invoice_id, &final_payment, &investment);
        let final_ledger = env.as_contract(&contract_id, || {
            crate::settlement::get_repayment_ledger(&env, &invoice_id)
        }).expect("settlement must persist its final repayment ledger");
        assert_ledger_conserves(&final_ledger)?;
        prop_assert_eq!(final_ledger.total_paid, face);
        let expected_platform_fee = (face - principal) * fee_bps as i128 / 10_000;
        prop_assert_eq!(final_ledger.platform_fee, expected_platform_fee);

        let investor_payout = token_client.balance(&investor) - investor_before;
        let platform_fee_payout = token_client.balance(&treasury) - treasury_before;
        prop_assert_eq!(investor_payout, face - expected_platform_fee);
        prop_assert_eq!(
            investor_payout,
            final_ledger.principal + final_ledger.investor_profit + final_ledger.late_penalty
        );
        prop_assert_eq!(platform_fee_payout, final_ledger.platform_fee);
        prop_assert_eq!(investor_payout + platform_fee_payout, face);
        prop_assert_eq!(client.get_invoice(&invoice_id).status, InvoiceStatus::Paid);
    }
}
