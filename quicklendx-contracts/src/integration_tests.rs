#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::{Address as _, AuthorizedFunction, AuthorizedInvocation},
    vec, Address, BytesN, Env, String, Symbol, Vec,
};
use crate::{
    audit::{AuditStorage, AuditQueryFilter, AuditOperation, AuditOperationFilter},
    invoice::{Invoice, InvoiceStatus, InvoiceStorage, InvoiceCategory},
    bid::{Bid, BidStatus, BidStorage},
    payments::{EscrowStatus, EscrowStorage},
    verification::{BusinessVerificationStatus, BusinessVerificationStorage},
};

/// Integration test helper for setting up test environment
pub struct IntegrationTestSetup<'a> {
    pub env: Env,
    pub contract_id: Address,
    pub client: QuickLendXContractClient<'a>,
    pub admin: Address,
    pub business: Address,
    pub investor: Address,
    pub currency: Address,
}

impl IntegrationTestSetup {
    pub fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        
        let contract_id = env.register_contract(None, QuickLendXContract);
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let currency = Address::generate(&env);
        
        // Set up admin
        client.set_admin(&admin);
        
        Self {
            env,
            contract_id,
            client,
            admin,
            business,
            investor,
            currency,
        }
    }
    
    pub fn setup_verified_business(&self) -> Result<(), QuickLendXError> {
        let kyc_data = String::from_str(&self.env, "Complete business registration documents");
        
        // Submit KYC application
        self.client.submit_kyc_application(&self.business, &kyc_data)?;
        
        // Verify business
        self.client.verify_business(&self.admin, &self.business)?;
        
        Ok(())
    }
    
    pub fn create_test_invoice(&self, amount: i128) -> Result<BytesN<32>, QuickLendXError> {
        let due_date = self.env.ledger().timestamp() + 86400; // 1 day from now
        let description = String::from_str(&self.env, "Integration test invoice");
        let category = InvoiceCategory::Services;
        let tags = vec![&self.env, String::from_str(&self.env, "test")];
        
        self.client.upload_invoice(
            &self.business,
            &amount,
            &self.currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
    }
}

/// Test complete invoice lifecycle from creation to settlement
#[test]
fn test_complete_invoice_lifecycle() {
    let setup = IntegrationTestSetup::new();
    
    // Step 1: Setup verified business
    setup.setup_verified_business().unwrap();
    
    // Step 2: Create invoice
    let invoice_amount = 1000i128;
    let invoice_id = setup.create_test_invoice(invoice_amount).unwrap();
    
    // Verify invoice is created and pending
    let invoice = setup.client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(invoice.amount, invoice_amount);
    
    // Step 3: Verify invoice
    setup.client.verify_invoice(&invoice_id).unwrap();
    let invoice = setup.client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified);
    
    // Step 4: Place bid
    let bid_amount = 950i128;
    let expected_return = 1050i128;
    let bid_id = setup.client.place_bid(
        &setup.investor,
        &invoice_id,
        &bid_amount,
        &expected_return,
    );
    
    // Verify bid is placed
    let bid = setup.client.get_bid(&bid_id).unwrap();
    assert_eq!(bid.bid_amount, bid_amount);
    assert_eq!(bid.status, BidStatus::Placed);
    
    // Step 5: Accept bid (creates escrow)
    setup.client.accept_bid(&invoice_id, &bid_id);
    
    // Verify escrow is created
    let escrow_status = setup.client.get_escrow_status(&invoice_id);
    assert_eq!(escrow_status, EscrowStatus::Held);
    
    // Verify invoice is funded
    let invoice = setup.client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, bid_amount);
    
    // Step 6: Release escrow funds
    setup.client.release_escrow_funds(&invoice_id).unwrap();
    
    // Verify escrow is released
    let escrow_status = setup.client.get_escrow_status(&invoice_id);
    assert_eq!(escrow_status, EscrowStatus::Released);
    
    // Step 7: Settle invoice
    let payment_amount = 1000i128;
    let platform_fee_bps = 250i128; // 2.5%
    setup.client.settle_invoice(
        &invoice_id,
        &payment_amount,
        &setup.admin,
        &platform_fee_bps,
    ).unwrap();
    
    // Verify invoice is settled
    let invoice = setup.client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
    assert!(invoice.settled_at.is_some());
    
    // Step 8: Add rating
    setup.env.as_contract(&setup.contract_id, || {
        let mut invoice = InvoiceStorage::get_invoice(&setup.env, &invoice_id).unwrap();
        invoice.add_rating(
            5,
            String::from_str(&setup.env, "Excellent service!"),
            setup.investor.clone(),
            setup.env.ledger().timestamp(),
        ).unwrap();
        InvoiceStorage::update_invoice(&setup.env, &invoice);
    });
    
    // Verify rating is added
    let (avg_rating, total_ratings, highest, lowest) = 
        setup.client.get_invoice_rating_stats(&invoice_id).unwrap();
    assert_eq!(avg_rating, Some(5));
    assert_eq!(total_ratings, 1);
    assert_eq!(highest, Some(5));
    assert_eq!(lowest, Some(5));
    
    // Verify audit trail is complete
    let audit_trail = setup.client.get_invoice_audit_trail(&invoice_id);
    assert!(!audit_trail.is_empty());
    
    // Verify audit integrity
    let is_valid = setup.client.validate_invoice_audit_integrity(&invoice_id);
    assert!(is_valid);
}

/// Test business verification workflow
#[test]
fn test_business_verification_workflow() {
    let setup = IntegrationTestSetup::new();
    
    let kyc_data = String::from_str(&setup.env, "Business registration documents");
    
    // Step 1: Submit KYC application
    setup.client.submit_kyc_application(&setup.business, &kyc_data).unwrap();
    
    // Verify status is pending
    let verification = setup.client.get_business_verification_status(&setup.business).unwrap();
    assert!(matches!(verification.status, BusinessVerificationStatus::Pending));
    
    // Verify business appears in pending list
    let pending_businesses = setup.client.get_pending_businesses();
    assert!(pending_businesses.contains(&setup.business));
    
    // Step 2: Verify business
    setup.client.verify_business(&setup.admin, &setup.business).unwrap();
    
    // Verify status is verified
    let verification = setup.client.get_business_verification_status(&setup.business).unwrap();
    assert!(matches!(verification.status, BusinessVerificationStatus::Verified));
    assert!(verification.verified_at.is_some());
    assert_eq!(verification.verified_by, Some(setup.admin.clone()));
    
    // Verify business appears in verified list
    let verified_businesses = setup.client.get_verified_businesses();
    assert!(verified_businesses.contains(&setup.business));
    
    // Step 3: Test invoice upload after verification
    let invoice_id = setup.create_test_invoice(1000).unwrap();
    let invoice = setup.client.get_invoice(&invoice_id);
    assert_eq!(invoice.business, setup.business);
}

/// Test bidding competition scenario
#[test]
fn test_bidding_competition() {
    let setup = IntegrationTestSetup::new();
    
    // Setup verified business and create invoice
    setup.setup_verified_business().unwrap();
    let invoice_id = setup.create_test_invoice(1000).unwrap();
    setup.client.verify_invoice(&invoice_id).unwrap();
    
    // Create multiple investors
    let investor1 = Address::generate(&setup.env);
    let investor2 = Address::generate(&setup.env);
    let investor3 = Address::generate(&setup.env);
    
    // Place multiple bids
    let bid1_id = setup.client.place_bid(&investor1, &invoice_id, &950, &1050);
    let bid2_id = setup.client.place_bid(&investor2, &invoice_id, &960, &1040);
    let bid3_id = setup.client.place_bid(&investor3, &invoice_id, &970, &1030);
    
    // Verify all bids are placed
    let bid1 = setup.client.get_bid(&bid1_id).unwrap();
    let bid2 = setup.client.get_bid(&bid2_id).unwrap();
    let bid3 = setup.client.get_bid(&bid3_id).unwrap();
    
    assert_eq!(bid1.status, BidStatus::Placed);
    assert_eq!(bid2.status, BidStatus::Placed);
    assert_eq!(bid3.status, BidStatus::Placed);
    
    // Accept the best bid (highest amount)
    setup.client.accept_bid(&invoice_id, &bid3_id);
    
    // Verify winning bid is accepted
    let winning_bid = setup.client.get_bid(&bid3_id).unwrap();
    assert_eq!(winning_bid.status, BidStatus::Accepted);
    
    // Verify invoice is funded with winning bid amount
    let invoice = setup.client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.funded_amount, 970);
    assert_eq!(invoice.investor, Some(investor3));
}

/// Test error handling and edge cases
#[test]
fn test_error_handling_scenarios() {
    let setup = IntegrationTestSetup::new();
    
    // Test 1: Try to upload invoice without verification
    let result = setup.client.try_upload_invoice(
        &setup.business,
        &1000,
        &setup.currency,
        &(setup.env.ledger().timestamp() + 86400),
        &String::from_str(&setup.env, "Test"),
        &InvoiceCategory::Services,
        &vec![&setup.env],
    );
    assert!(result.is_err());
    
    // Test 2: Try to bid on non-existent invoice
    let fake_invoice_id = BytesN::from_array(&setup.env, &[0u8; 32]);
    let result = setup.client.try_place_bid(
        &setup.investor,
        &fake_invoice_id,
        &1000,
        &1100,
    );
    assert!(result.is_err());
    
    // Test 3: Try to verify business with unauthorized admin
    let unauthorized_admin = Address::generate(&setup.env);
    setup.client.submit_kyc_application(
        &setup.business,
        &String::from_str(&setup.env, "KYC data"),
    ).unwrap();
    
    let result = setup.client.try_verify_business(&unauthorized_admin, &setup.business);
    assert!(result.is_err());
}
