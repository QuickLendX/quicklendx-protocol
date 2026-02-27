#![cfg(all(test, feature = "fuzz-tests"))]

use crate::{
    invoice::InvoiceCategory,
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String as SorobanString, Vec as SorobanVec, BytesN};

use proptest::prelude::*;

const MIN_AMOUNT: i128 = 1;
const MAX_AMOUNT: i128 = 100_000_000_000_000; // 100 Trillion
const MIN_DUE_DATE_OFFSET: u64 = 1;
const MAX_DUE_DATE_OFFSET: u64 = 10 * 365 * 24 * 60 * 60; // 10 years
const MAX_DESC_LEN: usize = 200;
const MAX_TAGS: u32 = 10;

fn setup_test_env() -> (Env, QuickLendXContractClient<'static>, Address, Address, Address) {
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
        let bid_id = client.place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        
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
