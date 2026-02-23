#![cfg(all(test, feature = "fuzz-tests"))]

use crate::{
    invoice::InvoiceCategory,
    QuickLendXContract, QuickLendXContractClient,
};
use soroban_sdk::{testutils::Address as _, Address, Env, String as SorobanString, Vec as SorobanVec};

use proptest::prelude::*;

const MIN_AMOUNT: i128 = 1;
const MAX_AMOUNT: i128 = 1_000_000_000;
const MIN_DUE_DATE_OFFSET: u64 = 1;
const MAX_DUE_DATE_OFFSET: u64 = 365 * 24 * 60 * 60;
const MAX_DESC_LEN: usize = 100;

fn setup_test_env() -> (Env, QuickLendXContractClient<'static>, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    client.initialize_admin(&admin);
    
    let currency = Address::generate(&env);
    client.add_currency(&admin, &currency);
    
    client.submit_kyc_application(&business, &SorobanString::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);
    
    client.submit_investor_kyc(&investor, &SorobanString::from_str(&env, "Investor KYC"));
    client.verify_investor(&investor, &1_000_000_000);
    
    (env, client, admin, business, investor)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    
    #[test]
    fn fuzz_store_invoice_valid_ranges(
        amount in MIN_AMOUNT..MAX_AMOUNT,
        due_date_offset in MIN_DUE_DATE_OFFSET..MAX_DUE_DATE_OFFSET,
        desc_len in 1usize..MAX_DESC_LEN,
    ) {
        let (env, client, _admin, business, _investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();
        
        let current_time = env.ledger().timestamp();
        let due_date = current_time + due_date_offset;
        let description = SorobanString::from_str(&env, &"x".repeat(desc_len));
        let tags = SorobanVec::new(&env);
        
        let result = client.try_store_invoice(
            &business,
            &amount,
            &currency,
            &due_date,
            &description,
            &InvoiceCategory::Services,
            &tags,
        );
        
        match result {
            Ok(invoice_id) => {
                let invoice_result = client.try_get_invoice(&invoice_id);
                assert!(invoice_result.is_ok());
                if let Ok(inv) = invoice_result {
                    assert_eq!(inv.amount, amount);
                    assert_eq!(inv.due_date, due_date);
                }
            }
            Err(_) => {}
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    
    #[test]
    fn fuzz_place_bid_valid_ranges(
        bid_amount in MIN_AMOUNT..MAX_AMOUNT,
        expected_return_multiplier in 100u32..200u32,
    ) {
        let (env, client, _admin, business, investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();
        
        let invoice_amount = 1_000_000;
        let due_date = env.ledger().timestamp() + 10000;
        let invoice_id_result = client.try_store_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &SorobanString::from_str(&env, "Test invoice"),
            &InvoiceCategory::Services,
            &SorobanVec::new(&env),
        );
        
        if invoice_id_result.is_err() {
            return Ok(());
        }
        let invoice_id = invoice_id_result.unwrap();
        
        let _ = client.try_verify_invoice(&invoice_id);
        
        let expected_return = (bid_amount * expected_return_multiplier as i128) / 100;
        
        let result = client.try_place_bid(
            &investor,
            &invoice_id,
            &bid_amount,
            &expected_return,
        );
        
        match result {
            Ok(bid_id) => {
                let bid = client.get_bid(&bid_id);
                assert!(bid.is_some());
                if let Some(b) = bid {
                    assert_eq!(b.bid_amount, bid_amount);
                    assert_eq!(b.expected_return, expected_return);
                }
            }
            Err(_) => {}
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]
    
    #[test]
    fn fuzz_settle_invoice_payment_amounts(
        payment_multiplier in 50u32..200u32,
    ) {
        let (env, client, _admin, business, investor) = setup_test_env();
        let currency = client.get_whitelisted_currencies().get(0).unwrap();
        
        let invoice_amount = 1_000_000;
        let due_date = env.ledger().timestamp() + 10000;
        let invoice_id_result = client.try_store_invoice(
            &business,
            &invoice_amount,
            &currency,
            &due_date,
            &SorobanString::from_str(&env, "Test invoice"),
            &InvoiceCategory::Services,
            &SorobanVec::new(&env),
        );
        
        if invoice_id_result.is_err() {
            return Ok(());
        }
        let invoice_id = invoice_id_result.unwrap();
        
        let _ = client.try_verify_invoice(&invoice_id);
        
        let bid_amount = 900_000;
        let expected_return = 1_000_000;
        let bid_result = client.try_place_bid(&investor, &invoice_id, &bid_amount, &expected_return);
        
        if bid_result.is_err() {
            return Ok(());
        }
        let bid_id = bid_result.unwrap();
        
        let _ = client.try_accept_bid(&business, &bid_id);
        
        let payment_amount = (bid_amount * payment_multiplier as i128) / 100;
        
        let invoice_before = client.try_get_invoice(&invoice_id);
        
        let result = client.try_settle_invoice(&invoice_id, &payment_amount);
        
        match result {
            Ok(_) => {
                if let Ok(inv_before) = invoice_before {
                    if let Ok(invoice_after) = client.try_get_invoice(&invoice_id) {
                        assert!(invoice_after.total_paid >= inv_before.total_paid);
                    }
                }
            }
            Err(_) => {
                if let Ok(inv_before) = invoice_before {
                    if let Ok(invoice_after) = client.try_get_invoice(&invoice_id) {
                        assert_eq!(inv_before.total_paid, invoice_after.total_paid);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod standard_tests {
    use super::*;

    #[test]
    fn test_fuzz_infrastructure_works() {
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
        
        let invoice_result = client.try_get_invoice(&invoice_id);
        assert!(invoice_result.is_ok());
    }
}
