use super::*;

use crate::invoice::{InvoiceCategory, InvoiceStatus};
use crate::profits::{
    calculate_profit, calculate_treasury_split, verify_no_dust, PlatformFee, BPS_DENOMINATOR,
};
use crate::settlement::get_invoice_progress;
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

fn setup_funded_invoice_with_fee(
    env: &Env,
    client: &QuickLendXContractClient,
    contract_id: &Address,
    invoice_amount: i128,
    investment_amount: i128,
    fee_bps: u32,
) -> (BytesN<32>, Address, Address, Address) {
    let admin = Address::generate(env);
    let business = Address::generate(env);
    let investor = Address::generate(env);

    client.set_admin(&admin);
    client.initialize_fee_system(&admin);
    client.update_platform_fee_bps(&fee_bps);

    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);

    let initial_balance = 500_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);

    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, contract_id, &initial_balance, &expiration);

    client.submit_kyc_application(&business, &String::from_str(env, "business-kyc"));
    client.verify_business(&admin, &business);

    client.submit_investor_kyc(&investor, &String::from_str(env, "investor-kyc"));
    client.verify_investor(&investor, &initial_balance);

    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(env, "Invoice for accounting identity tests"),
        &InvoiceCategory::Services,
        &Vec::new(env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &investment_amount, &invoice_amount, &BytesN::from_array(&env, &[0u8; 32]));
    client.accept_bid(&invoice_id, &bid_id);

    (invoice_id, business, investor, currency)
}

#[test]
fn test_settlement_accounting_identity_exhaustive_fee_bps_pure() {
    let payment_amounts = [0i128, 1, 2, 3, 10, 49, 50, 51, 99, 100, 1_001, 1_000_000];

    for fee_bps in 0i128..=BPS_DENOMINATOR {
        for payment_amount in payment_amounts {
            let investment_variants = [
                0i128,
                payment_amount,
                payment_amount.saturating_sub(1),
                payment_amount.saturating_add(1),
            ];

            for investment_amount in investment_variants {
                let (investor_return, platform_fee) =
                    PlatformFee::calculate_with_fee_bps(investment_amount, payment_amount, fee_bps);

                assert!(
                    verify_no_dust(investor_return, platform_fee, payment_amount),
                    "dust detected for investment={}, payment={}, fee_bps={}",
                    investment_amount,
                    payment_amount,
                    fee_bps
                );
                assert!(
                    platform_fee <= payment_amount,
                    "platform_fee cannot exceed total_paid for investment={}, payment={}, fee_bps={}",
                    investment_amount,
                    payment_amount,
                    fee_bps
                );
            }
        }
    }
}

#[test]
fn test_settlement_accounting_identity_table_driven_cases() {
    let max = i128::MAX;
    let cases = [
        (0i128, 1i128, 0i128),
        (0i128, 1i128, 1i128),
        (10_000i128, 1i128, 1i128),
        (10_000i128, 1_000i128, 1_001i128),
        (200i128, max - 1, max),
        (10_000i128, max - 1, max),
        (9_999i128, max / 2 - 1, max / 2),
    ];

    for (fee_bps, investment_amount, payment_amount) in cases {
        let (investor_return, platform_fee) =
            PlatformFee::calculate_with_fee_bps(investment_amount, payment_amount, fee_bps);

        assert_eq!(investor_return + platform_fee, payment_amount);
        assert!(platform_fee <= payment_amount);
        assert!(
            verify_no_dust(investor_return, platform_fee, payment_amount),
            "table-driven identity failed for investment={}, payment={}, fee_bps={}",
            investment_amount,
            payment_amount,
            fee_bps
        );
    }
}

#[test]
fn test_settlement_accounting_identity_boundary_amounts_pure() {
    let max = i128::MAX;
    let cases = [
        (0i128, 1i128, 0i128),
        (0i128, 1i128, 10_000i128),
        (1i128, 1i128, 10_000i128),
        (1i128, 2i128, 5_000i128),
        (max - 1, max, 10_000i128),
        (max / 2 - 1, max / 2, 9_999i128),
    ];

    for (investment_amount, payment_amount, fee_bps) in cases {
        let (investor_return, platform_fee) =
            PlatformFee::calculate_with_fee_bps(investment_amount, payment_amount, fee_bps);

        assert!(
            verify_no_dust(investor_return, platform_fee, payment_amount),
            "identity must hold at boundary investment={}, payment={}, fee_bps={}",
            investment_amount,
            payment_amount,
            fee_bps
        );
        assert!(
            platform_fee <= payment_amount,
            "platform_fee cannot exceed total_paid at boundary investment={}, payment={}, fee_bps={}",
            investment_amount,
            payment_amount,
            fee_bps
        );
    }
}

#[test]
fn test_settlement_accounting_identity_partial_then_final() {
    let cases = [
        (0u32, 1_001i128, 1_000i128, 1i128),
        (1u32, 1_101i128, 1_000i128, 101i128),
        (200u32, 2_001i128, 1_500i128, 501i128),
        (1_000u32, 5_001i128, 2_000i128, 2_500i128),
    ];

    for (fee_bps, invoice_amount, investment_amount, partial_payment) in cases {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let (invoice_id, _business, investor, currency) = setup_funded_invoice_with_fee(
            &env,
            &client,
            &contract_id,
            invoice_amount,
            investment_amount,
            fee_bps,
        );

        let token_client = token::Client::new(&env, &currency);
        let investor_before = token_client.balance(&investor);

        client.process_partial_payment(
            &invoice_id,
            &partial_payment,
            &String::from_str(&env, "partial-before-final"),
        );

        let progress_after_partial = env.as_contract(&contract_id, || {
            get_invoice_progress(&env, &invoice_id).unwrap()
        });
        assert_eq!(progress_after_partial.total_paid, partial_payment);
        assert_eq!(progress_after_partial.status, InvoiceStatus::Funded);

        let remaining_due = invoice_amount - partial_payment;
        client.settle_invoice(&invoice_id, &remaining_due);

        let investor_received = token_client.balance(&investor) - investor_before;
        let implied_platform_fee = invoice_amount - investor_received;

        let (expected_investor_return, expected_platform_fee) = env
            .as_contract(&contract_id, || {
                calculate_profit(&env, investment_amount, invoice_amount)
            });

        assert_eq!(investor_received, expected_investor_return);
        assert_eq!(implied_platform_fee, expected_platform_fee);
        assert_eq!(investor_received + implied_platform_fee, invoice_amount);
        assert!(implied_platform_fee <= invoice_amount);

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.status, InvoiceStatus::Paid);
    }
}

#[test]
fn test_settlement_accounting_identity_treasury_split_is_dust_free() {
    let platform_fee_samples = [0i128, 1, 2, 3, 10, 99, 100, 101, 1_000, 1_000_000];

    for platform_fee in platform_fee_samples {
        for treasury_share_bps in 0i128..=BPS_DENOMINATOR {
            let (treasury_amount, remaining_amount) =
                calculate_treasury_split(platform_fee, treasury_share_bps);

            assert_eq!(treasury_amount + remaining_amount, platform_fee.max(0));
            assert!(treasury_amount <= platform_fee.max(0));
            assert!(remaining_amount <= platform_fee.max(0));
        }
    }
}
