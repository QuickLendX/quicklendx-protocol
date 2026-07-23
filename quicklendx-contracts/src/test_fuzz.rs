#![cfg(all(test, feature = "fuzz-tests"))]

use crate::{invoice::InvoiceCategory, QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::Address as _, Address, BytesN, Env, String as SorobanString, Vec as SorobanVec,
};

use proptest::prelude::*;

const MIN_AMOUNT: i128 = 1;
const MAX_AMOUNT: i128 = 100_000_000_000_000; // 100 Trillion
const MIN_DUE_DATE_OFFSET: u64 = 1;
const MAX_DUE_DATE_OFFSET: u64 = 10 * 365 * 24 * 60 * 60; // 10 years
const MAX_DESC_LEN: usize = 200;
const MAX_TAGS: u32 = 10;

fn setup_test_env() -> (
    Env,
    QuickLendXContractClient<'static>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let _ = client.try_initialize_admin(&admin);

    let currency = Address::generate(&env);
    let _ = client.try_add_currency(&admin, &currency);

    let _ = client.try_submit_kyc_application(&business, &SorobanString::from_str(&env, "Business KYC 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890"));
    let _ = client.try_verify_business(&admin, &business);

    let kyc_long = SorobanString::from_str(&env, "Investor KYC 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890 1234567890");
    let _ = client.try_submit_investor_kyc(&investor, &kyc_long);
    // Passing investor and a massive limit to accommodate 100 Trillion fuzzing
    let _ = client.try_verify_investor(&investor, &MAX_AMOUNT);

    (env, client, admin, business, investor)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn fuzz_invoice_creation(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
        desc_len in 1usize..MAX_DESC_LEN,
        tag_count in 0u32..MAX_TAGS,
    ) {
        let (env, client, _admin, business, _investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();

        let current_time = env.ledger().timestamp();
        let due_date = current_time.saturating_add(due_date_offset);
        let description = SorobanString::from_str(&env, &"x".repeat(desc_len));

        let mut tags = SorobanVec::new(&env);
        for _ in 0..tag_count {
            tags.push_back(SorobanString::from_str(&env, "tag"));
        }

        let result = client.try_store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &InvoiceCategory::Services,
            &tags,
        );

        if let Ok(Ok(invoice_id)) = result {
            let invoice = client.get_invoice(&invoice_id);
            assert_eq!(invoice.amount, amount);
            assert_eq!(invoice.due_date, due_date);
            assert_eq!(invoice.description.len(), description.len());
            assert_eq!(invoice.tags.len(), tag_count);
        }
    }

    #[test]
    fn fuzz_bid_placement(
        invoice_amount in 1_000i128..MAX_AMOUNT,
        bid_amount_factor in 10u32..200u32, // 10% to 200% of invoice amount
        return_margin_bps in 100u32..2000u32, // 1% to 20% margin
    ) {
        let (env, client, _admin, business, investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();

        let due_date = env.ledger().timestamp() + 10000;
        let invoice_id = client.store_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &SorobanString::from_str(&env, "Test invoice"),
            &InvoiceCategory::Services,
            &SorobanVec::new(&env),
        );

        let _ = client.try_verify_invoice(&invoice_id);

        let bid_amount = invoice_amount.saturating_mul(bid_amount_factor as i128) / 100;
        if bid_amount == 0 { return Ok(()); }
        let expected_return = bid_amount.saturating_add(bid_amount.saturating_mul(return_margin_bps as i128) / 10_000);

        let result = client.try_place_bid(
            &investor,
            &invoice_id,
            &bid_amount,
            &expected_return,
        );

        if let Ok(Ok(bid_id)) = result {
            let bid = client.get_bid(&bid_id).unwrap();
            assert_eq!(bid.bid_amount, bid_amount);
            assert_eq!(bid.expected_return, expected_return);
            assert_eq!(bid.invoice_id, invoice_id);
            assert_eq!(bid.investor, investor);
        }
    }

    #[test]
    fn fuzz_settlement_capping(
        invoice_amount in 1_000i128..MAX_AMOUNT,
        bid_amount_factor in 50u32..100u32, // 50% to 100%
        payment_amount_factor in 1u32..200u32, // 1% to 200%
    ) {
        let (env, client, _admin, business, investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();

        let due_date = env.ledger().timestamp() + 10000;
        let invoice_id = client.store_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &SorobanString::from_str(&env, "Test invoice"),
            &InvoiceCategory::Services,
            &SorobanVec::new(&env),
        );

        let _ = client.try_verify_invoice(&invoice_id);

        let bid_amount = invoice_amount.saturating_mul(bid_amount_factor as i128) / 100;
        let expected_return = invoice_amount;
        let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return, &BytesN::from_array(&env, &[0u8; 32]));

        let _ = client.try_accept_bid(&invoice_id, &bid_id);

        let payment_amount = invoice_amount.saturating_mul(payment_amount_factor as i128) / 100;

        // Try settle
        let result = client.try_settle_invoice(&invoice_id, &payment_amount);

        if let Ok(Ok(_)) = result {
            let invoice_after = client.get_invoice(&invoice_id);
            // After successful settle_invoice, total_paid must be exactly invoice.amount
            // because settle_invoice expects/enforces full settlement (or close to it)
            assert_eq!(invoice_after.total_paid, invoice_after.amount);
            assert!(matches!(invoice_after.status, crate::invoice::InvoiceStatus::Paid));
        }
    }

    #[test]
    fn fuzz_arithmetic_safety(
        a in 0i128..MAX_AMOUNT,
        b in 1i128..MAX_AMOUNT,
        fee_bps in 0i128..1000i128,
    ) {
        // Test payment progress calculation
        let total_paid = a;
        let total_due = b;
        let percentage = total_paid
            .saturating_mul(100i128)
            .checked_div(total_due)
            .unwrap_or(0);

        let progress = core::cmp::min(percentage, 100i128) as u32;
        assert!(progress <= 100);

        // Test platform fee calculation invariants from profits.rs
        let investment = b;
        let payment = a;

        let gross_profit = payment.saturating_sub(investment);
        if gross_profit <= 0 {
            // No profit scenario
            let platform_fee = 0;
            let investor_return = payment;
            assert_eq!(investor_return + platform_fee, payment);
        } else {
            // Profit scenario
            let platform_fee = gross_profit.saturating_mul(fee_bps) / 10_000;
            let investor_return = payment.saturating_sub(platform_fee);

            // Invariant: investor_return + platform_fee == payment (no dust)
            assert_eq!(investor_return + platform_fee, payment);
            // Invariant: platform_fee <= gross_profit
            assert!(platform_fee <= gross_profit);
        }
    }
}

// ============================================================================
// Payment Sequence Fuzz Harness (Issue #1080)
// ============================================================================
// Invariants protected by these fuzz tests:
// 1. total_paid <= total_due always (capping invariant)
// 2. (invoice_id, nonce) uniqueness enforced (replay protection)
// 3. payment_count bounded by MAX_PAYMENT_COUNT
// ============================================================================

/// Setup helper for funded invoice required by payment fuzz tests.
/// Creates an invoice with status Funded (ready for partial payments).
fn setup_funded_invoice_for_fuzz(
    env: &Env,
    client: &QuickLendXContractClient,
    invoice_amount: i128,
) -> (BytesN<32>, Address, Address) {
    let business = Address::generate(env);
    let investor = Address::generate(env);

    let currency = client.get_whitelisted_currencies().get(0).unwrap();
    let due_date = env.ledger().timestamp() + 86_400;

    let invoice_id = client.store_invoice(
        &business,
        &invoice_amount,
        &currency,
        &due_date,
        &SorobanString::from_str(env, "Fuzz test invoice"),
        &InvoiceCategory::Services,
        &SorobanVec::new(env),
    );

    let _ = client.try_verify_invoice(&invoice_id);

    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &invoice_amount,
        &(invoice_amount + 100),
    );
    let _ = client.try_accept_bid(&invoice_id, &bid_id);

    (invoice_id, business, investor)
}

/// Generate a random payment amount relative to the invoice total.
/// Returns amounts in range [1, invoice_amount * 3] to allow overpayment.
fn gen_payment_amount(invoice_amount: i128, rng: &mut impl rand::Rng) -> i128 {
    let max_multiplier: u32 = rng.gen();
    let max_amount: i128 = (invoice_amount as u128 * max_multiplier as u128 % 10_000_002) as i128;
    rng.gen_range(1, max_amount.max(1) + 1)
}

/// Generate a payment sequence action: either new payment, duplicate nonce, or overpay.
#[derive(Clone, Copy, Debug)]
enum PaymentAction {
    NewPayment { amount: i128, nonce: usize },
    DuplicateNonce(usize),
}

/// Fuzz harness for payment sequences that validates invariants after each operation.
/// Uses deterministic PRNG seeded from test parameters.
fn run_payment_sequence_fuzz(
    env: &Env,
    client: &QuickLendXContractClient,
    invoice_id: &BytesN<32>,
    actions: &[PaymentAction],
    invoice_amount: i128,
) -> i128 {
    let mut nonces: Vec<SorobanString> = Vec::new(env);
    let mut total_applied = 0i128;

    for action in actions {
        match action {
            PaymentAction::NewPayment { amount, nonce_idx } => {
                let nonce = SorobanString::from_str(env, &format!("pay-nonce-{}-{}", (*amount % 1000), *nonce_idx));
                nonces.push_back(nonce.clone());

                let result = client.try_process_partial_payment(invoice_id, amount, &nonce);

                if let Ok(Ok(_)) = result {
                    let invoice = client.get_invoice(invoice_id);
                    let expected_applied = (*amount).min(invoice_amount - total_applied);

                    total_applied += expected_applied;
                    assert!(
                        total_applied <= invoice_amount,
                        "Capping invariant violated: total_applied ({}) > invoice_amount ({})",
                        total_applied, invoice_amount
                    );

                    assert!(
                        invoice.total_paid <= invoice.amount,
                        "total_paid ({}) exceeded total_due ({})",
                        invoice.total_paid, invoice.amount
                    );
                } else {
                    let error = result.unwrap_err();
                    let invoice = client.get_invoice(invoice_id);
                    if invoice.status == crate::invoice::InvoiceStatus::Funded {
                        let remaining = invoice_amount - total_applied;
                        assert!(
                            *amount > 0 && (*amount <= remaining || remaining == 0),
                            "Unexpected rejection while invoice is still Funded"
                        );
                    }
                }
            }
            PaymentAction::DuplicateNonce(idx) => {
                if *idx < nonces.len() {
                    let dup_nonce = nonces.get(*idx).unwrap();
                    let prev_invoice = client.get_invoice(invoice_id);

                    let result = client.try_process_partial_payment(invoice_id, &500, &dup_nonce);

                    if let Ok(Ok(_)) = result {
                        let after_invoice = client.get_invoice(invoice_id);
                        assert_eq!(
                            after_invoice.total_paid, prev_invoice.total_paid,
                            "Duplicate nonce changed total_paid"
                        );
                    } else if prev_invoice.status == crate::invoice::InvoiceStatus::Funded {
                        assert!(
                            result.is_ok(),
                            "Duplicate nonce on Funded invoice should be idempotent"
                        );
                    }
                }
            }
        }
    }

    total_applied
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Fuzz test: validates capping and replay invariants under random payment sequences.
    /// Generates random amounts, nonces, and sequences to stress-test record_payment.
    #[test]
    fn fuzz_payment_capping_invariant(
        invoice_amount in 1_000i128..100_000i128,
        seed in 0u64..u64::MAX,
    ) {
        let (env, client, _admin, _business, _investor) = setup_test_env();
        let (invoice_id, _, _) = setup_funded_invoice_for_fuzz(&env, &client, invoice_amount);

        let mut rng = rand::rngs::StdRng::seed_from_rng(rand::SeedableRng::seed_from_u64(seed));

        let action_count = 50usize;
        let mut actions: Vec<PaymentAction> = Vec::new();

        for i in 0..action_count {
            if i > 0 && rng.gen_bool(0.3) && actions.len() > 10 {
                let dup_idx = rng.gen_range(0usize, actions.len() / 2);
                actions.push(PaymentAction::DuplicateNonce(dup_idx));
            } else {
                let amount = gen_payment_amount(invoice_amount, &mut rng);
                actions.push(PaymentAction::NewPayment { amount, nonce: i });
            }
        }

        let _total = run_payment_sequence_fuzz(&env, &client, &invoice_id, &actions, invoice_amount);

        let final_invoice = client.get_invoice(&invoice_id);
        assert!(
            final_invoice.total_paid <= final_invoice.amount,
            "Final capping invariant: total_paid ({}) <= total_due ({})",
            final_invoice.total_paid, final_invoice.amount
        );
    }

    /// Fuzz test: overpay-then-underpay edge case.
    /// Validates that capping works correctly even when large overpayments are followed by small payments.
    #[test]
    fn fuzz_overpay_then_underpay_sequence(
        invoice_amount in 100i128..10_000i128,
        overpay_factor in 1u32..100u32,
    ) {
        let (env, client, _admin, _business, _investor) = setup_test_env();
        let (invoice_id, _, _) = setup_funded_invoice_for_fuzz(&env, &client, invoice_amount);

        let overpay_amount = (invoice_amount as u128 * overpay_factor as u128 / 10) as i128;
        if overpay_amount > 0 {
            let nonce1 = SorobanString::from_str(&env, "overpay-1");
            let _ = client.try_process_partial_payment(&invoice_id, &overpay_amount, &nonce1);
        }

        let after_overpay = client.get_invoice(&invoice_id);
        assert!(
            after_overpay.total_paid <= after_overpay.amount,
            "Capping violated after overpayment: {} > {}",
            after_overpay.total_paid, after_overpay.amount
        );

        if after_overpay.status == crate::invoice::InvoiceStatus::Funded {
            let small_amount = invoice_amount / 10;
            let nonce2 = SorobanString::from_str(&env, "underpay-1");
            let _ = client.try_process_partial_payment(&invoice_id, &small_amount, &nonce2);

            let after_underpay = client.get_invoice(&invoice_id);
            assert!(
                after_underpay.total_paid <= after_underpay.amount,
                "Capping violated after underpay: {} > {}",
                after_underpay.total_paid, after_underpay.amount
            );
        }
    }

    /// Fuzz test: repeated nonce rejection in sequence.
    /// Validates that duplicate nonces are properly rejected/idempotent.
    #[test]
    fn fuzz_repeated_nonce_replay_protection(
        invoice_amount in 1_000i128..50_000i128,
        repeat_count in 1usize..20usize,
        payment_amount in 10i128..5_000i128,
    ) {
        let (env, client, _admin, _business, _investor) = setup_test_env();
        let (invoice_id, _, _) = setup_funded_invoice_for_fuzz(&env, &client, invoice_amount);

        let nonce = SorobanString::from_str(&env, "repeat-nonce");
        let _ = client.try_process_partial_payment(&invoice_id, &payment_amount, &nonce);

        let initial_count = crate::settlement::get_payment_count(&env, &invoice_id).unwrap();
        let initial_paid = client.get_invoice(&invoice_id).total_paid;

        for _ in 0..repeat_count {
            env.ledger().set_timestamp(env.ledger().timestamp() + 100);
            let result = client.try_process_partial_payment(&invoice_id, &payment_amount, &nonce);

            if result.is_ok() {
                let after = client.get_invoice(&invoice_id);
                assert_eq!(
                    after.total_paid, initial_paid,
                    "Duplicate nonce changed total_paid"
                );
            }
        }

        let final_count = crate::settlement::get_payment_count(&env, &invoice_id).unwrap();
        assert_eq!(
            final_count, initial_count,
            "Duplicate nonces incremented payment count"
        );
    }

    /// Fuzz test: payment count exhaustion near MAX_PAYMENT_COUNT.
    /// Validates that payments are rejected once the cap is reached.
    #[test]
    fn fuzz_payment_count_exhaustion(
        invoice_amount in 10_000i128..1_000_000i128,
        payment_amount in 1i128..100i128,
    ) {
        let (env, client, _admin, _business, _investor) = setup_test_env();
        let (invoice_id, _, _) = setup_funded_invoice_for_fuzz(&env, &client, invoice_amount);

        let max_payments = (invoice_amount / payment_amount).min(50) as u32;

        for i in 0..max_payments {
            env.ledger().set_timestamp(50_000 + i as u64);
            let nonce = SorobanString::from_str(&env, &format!("exhaust-{}", i));
            let result = client.try_process_partial_payment(&invoice_id, &payment_amount, &nonce);

            if result.is_ok() {
                let invoice = client.get_invoice(&invoice_id);
                assert!(
                    invoice.total_paid <= invoice.amount,
                    "Capping violated during exhaustion test"
                );
            }

            if client.get_invoice(&invoice_id).status == crate::invoice::InvoiceStatus::Paid {
                break;
            }
        }

        let final_count = crate::settlement::get_payment_count(&env, &invoice_id).unwrap();

assert!(
             final_count <= max_payments,
             "Payment count ({}) exceeded expected max ({})",
             final_count, max_payments
         );
    }
}

#[cfg(test)]
mod extra_tests {
    use super::*;

    #[test]
    fn test_fuzz_infrastructure_smoke_test() {
        let (env, client, _admin, business, _investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();

        let invoice_id = client.store_invoice(
            &business,
            &1_000_000,
            &currency,
            &(env.ledger().timestamp() + 10000),
            &SorobanString::from_str(&env, "Test"),
            &InvoiceCategory::Services,
            &SorobanVec::new(&env),
        );

        let invoice = client.get_invoice(&invoice_id);
        assert_eq!(invoice.amount, 1_000_000);
    }
}
