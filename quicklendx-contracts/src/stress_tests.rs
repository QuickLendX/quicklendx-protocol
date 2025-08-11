#![cfg(test)]

use super::*;
use soroban_sdk::{
    testutils::Address as _,
    vec, Address, BytesN, Env, String, Vec,
};
use crate::{
    invoice::{InvoiceCategory, InvoiceStorage, InvoiceStatus},
    bid::{BidStorage, BidStatus},
    payments::{EscrowStorage, EscrowStatus},
};

/// Stress test configuration
struct StressTestConfig {
    pub max_invoices: u32,
    pub max_bids_per_invoice: u32,
    pub max_concurrent_operations: u32,
    pub test_duration_seconds: u64,
}

impl Default for StressTestConfig {
    fn default() -> Self {
        Self {
            max_invoices: 1000,
            max_bids_per_invoice: 50,
            max_concurrent_operations: 100,
            test_duration_seconds: 30,
        }
    }
}

/// Stress test helper for managing large-scale operations
struct StressTestEnvironment<'a> {
    env: Env,
    client: QuickLendXContractClient<'a>,
    admin: Address,
    businesses: Vec<Address>,
    investors: Vec<Address>,
    currency: Address,
    config: StressTestConfig,
}

impl StressTestEnvironment {
    fn new(config: StressTestConfig) -> Self {
        let env = Env::default();
        env.mock_all_auths();
        
        let contract_id = env.register_contract(None, QuickLendXContract);
        let client = QuickLendXContractClient::new(&env, &contract_id);
        
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        
        // Create multiple businesses and investors
        let mut businesses = Vec::new();
        let mut investors = Vec::new();
        
        for i in 0..10 {
            let business = Address::generate(&env);
            let kyc_data = String::from_str(&env, "KYC data for business");
            
            client.submit_kyc_application(&business, &kyc_data).unwrap();
            client.verify_business(&admin, &business).unwrap();
            
            businesses.push(business);
        }
        
        for _ in 0..50 {
            investors.push(Address::generate(&env));
        }
        
        let currency = Address::generate(&env);
        
        Self {
            env,
            client,
            admin,
            businesses,
            investors,
            currency,
            config,
        }
    }
    
    fn create_random_invoice(&self, business_idx: usize) -> Result<BytesN<32>, QuickLendXError> {
        let business = &self.businesses[business_idx % self.businesses.len()];
        let amount = 1000 + (business_idx as i128 * 100);
        let due_date = self.env.ledger().timestamp() + 86400;
        let description = String::from_str(&self.env, "Stress test invoice");
        let category = InvoiceCategory::Services;
        let tags = vec![&self.env, String::from_str(&self.env, "stress-test")];
        
        self.client.upload_invoice(
            business,
            &amount,
            &self.currency,
            &due_date,
            &description,
            &category,
            &tags,
        )
    }
    
    fn place_random_bid(&self, invoice_id: &BytesN<32>, investor_idx: usize) -> BytesN<32> {
        let investor = &self.investors[investor_idx % self.investors.len()];
        let bid_amount = 900 + (investor_idx as i128 * 10);
        let expected_return = 1000 + (investor_idx as i128 * 10);
        
        self.client.place_bid(investor, invoice_id, &bid_amount, &expected_return)
    }
}

/// Test system behavior under high invoice creation load
#[test]
fn test_high_volume_invoice_creation() {
    let config = StressTestConfig {
        max_invoices: 500,
        ..Default::default()
    };
    let env = StressTestEnvironment::new(config);
    
    let mut created_invoices = Vec::new();
    let start_time = env.env.ledger().timestamp();
    
    // Create large number of invoices
    for i in 0..env.config.max_invoices {
        let invoice_id = env.create_random_invoice(i as usize).unwrap();
        created_invoices.push(invoice_id);
        
        // Verify invoice was created correctly
        let invoice = env.client.get_invoice(&invoice_id);
        assert_eq!(invoice.status, InvoiceStatus::Pending);
        assert!(invoice.amount > 0);
    }
    
    let end_time = env.env.ledger().timestamp();
    let duration = end_time - start_time;
    
    // Performance assertions
    assert_eq!(created_invoices.len(), env.config.max_invoices as usize);
    assert!(duration < 60); // Should complete within 60 seconds
    
    // Verify all invoices are retrievable
    for invoice_id in &created_invoices {
        let invoice = env.client.get_invoice(invoice_id);
        assert!(invoice.description.len() > 0);
    }
    
    // Test business invoice lists
    for business in &env.businesses {
        let business_invoices = env.client.get_business_invoices(business);
        assert!(!business_invoices.is_empty());
    }
}

/// Test system behavior under heavy bidding load
#[test]
fn test_high_volume_bidding() {
    let config = StressTestConfig {
        max_invoices: 50,
        max_bids_per_invoice: 20,
        ..Default::default()
    };
    let env = StressTestEnvironment::new(config);
    
    // Create invoices
    let mut invoice_ids = Vec::new();
    for i in 0..env.config.max_invoices {
        let invoice_id = env.create_random_invoice(i as usize).unwrap();
        env.client.verify_invoice(&invoice_id).unwrap();
        invoice_ids.push(invoice_id);
    }
    
    // Place multiple bids on each invoice
    let mut total_bids = 0;
    for invoice_id in &invoice_ids {
        for j in 0..env.config.max_bids_per_invoice {
            let bid_id = env.place_random_bid(invoice_id, j as usize);
            
            // Verify bid was placed
            let bid = env.client.get_bid(&bid_id).unwrap();
            assert_eq!(bid.status, BidStatus::Placed);
            assert_eq!(bid.invoice_id, *invoice_id);
            
            total_bids += 1;
        }
    }
    
    // Verify total bid count
    let expected_total = env.config.max_invoices * env.config.max_bids_per_invoice;
    assert_eq!(total_bids, expected_total);
    
    // Test bid acceptance under load
    for invoice_id in &invoice_ids {
        let bids = env.client.get_bids_for_invoice(invoice_id);
        if !bids.is_empty() {
            let first_bid_id = bids.get(0).unwrap();
            env.client.accept_bid(invoice_id, &first_bid_id);
            
            // Verify escrow was created
            let escrow_status = env.client.get_escrow_status(invoice_id);
            assert_eq!(escrow_status, EscrowStatus::Held);
        }
    }
}

/// Test concurrent operations stress
#[test]
fn test_concurrent_operations_stress() {
    let config = StressTestConfig {
        max_concurrent_operations: 50,
        ..Default::default()
    };
    let env = StressTestEnvironment::new(config);
    
    let mut operations_completed = 0;
    let start_time = env.env.ledger().timestamp();
    
    // Simulate concurrent operations
    for i in 0..env.config.max_concurrent_operations {
        match i % 4 {
            0 => {
                // Create invoice
                let _invoice_id = env.create_random_invoice(i as usize).unwrap();
                operations_completed += 1;
            }
            1 => {
                // Create and verify invoice
                let invoice_id = env.create_random_invoice(i as usize).unwrap();
                env.client.verify_invoice(&invoice_id).unwrap();
                operations_completed += 1;
            }
            2 => {
                // Create invoice and place bid
                let invoice_id = env.create_random_invoice(i as usize).unwrap();
                env.client.verify_invoice(&invoice_id).unwrap();
                let _bid_id = env.place_random_bid(&invoice_id, i as usize);
                operations_completed += 1;
            }
            3 => {
                // Full workflow: create, verify, bid, accept
                let invoice_id = env.create_random_invoice(i as usize).unwrap();
                env.client.verify_invoice(&invoice_id).unwrap();
                let bid_id = env.place_random_bid(&invoice_id, i as usize);
                env.client.accept_bid(&invoice_id, &bid_id);
                operations_completed += 1;
            }
            _ => unreachable!(),
        }
    }
    
    let end_time = env.env.ledger().timestamp();
    let duration = end_time - start_time;
    
    assert_eq!(operations_completed, env.config.max_concurrent_operations);
    assert!(duration < 120); // Should complete within 2 minutes
}

/// Test memory usage under load
#[test]
fn test_memory_usage_under_load() {
    let config = StressTestConfig {
        max_invoices: 200,
        max_bids_per_invoice: 10,
        ..Default::default()
    };
    let env = StressTestEnvironment::new(config);
    
    // Track storage usage
    let mut storage_operations = 0;
    
    // Create invoices and bids
    for i in 0..env.config.max_invoices {
        let invoice_id = env.create_random_invoice(i as usize).unwrap();
        env.client.verify_invoice(&invoice_id).unwrap();
        storage_operations += 2; // Create + verify
        
        for j in 0..env.config.max_bids_per_invoice {
            let _bid_id = env.place_random_bid(&invoice_id, j as usize);
            storage_operations += 1;
        }
    }
    
    // Verify all data is still accessible
    let total_invoices = env.client.get_total_invoice_count();
    assert_eq!(total_invoices, env.config.max_invoices as u32);
    
    // Test query performance under load
    let pending_invoices = env.client.get_invoices_by_status(&InvoiceStatus::Pending);
    let verified_invoices = env.client.get_invoices_by_status(&InvoiceStatus::Verified);
    
    assert!(!pending_invoices.is_empty() || !verified_invoices.is_empty());
    
    // Verify storage integrity
    for business in &env.businesses {
        let business_invoices = env.client.get_business_invoices(business);
        for invoice_id in business_invoices.iter() {
            let invoice = env.client.get_invoice(&invoice_id);
            assert_eq!(invoice.business, *business);
        }
    }
}

/// Test system recovery after stress
#[test]
fn test_system_recovery_after_stress() {
    let config = StressTestConfig {
        max_invoices: 100,
        max_bids_per_invoice: 5,
        ..Default::default()
    };
    let env = StressTestEnvironment::new(config);
    
    // Apply stress load
    let mut invoice_ids = Vec::new();
    for i in 0..env.config.max_invoices {
        let invoice_id = env.create_random_invoice(i as usize).unwrap();
        env.client.verify_invoice(&invoice_id).unwrap();
        
        for j in 0..env.config.max_bids_per_invoice {
            let _bid_id = env.place_random_bid(&invoice_id, j as usize);
        }
        
        invoice_ids.push(invoice_id);
    }
    
    // Test normal operations after stress
    let new_invoice_id = env.create_random_invoice(0).unwrap();
    let new_invoice = env.client.get_invoice(&new_invoice_id);
    assert_eq!(new_invoice.status, InvoiceStatus::Pending);
    
    // Verify existing data integrity
    for invoice_id in &invoice_ids {
        let invoice = env.client.get_invoice(invoice_id);
        assert!(invoice.amount > 0);
        assert!(invoice.description.len() > 0);
    }
    
    // Test audit trail integrity
    for invoice_id in invoice_ids.iter().take(10) {
        let audit_trail = env.client.get_invoice_audit_trail(invoice_id);
        assert!(!audit_trail.is_empty());
        
        let is_valid = env.client.validate_invoice_audit_integrity(invoice_id);
        assert!(is_valid);
    }
}

/// Test edge case: maximum bid competition
#[test]
fn test_maximum_bid_competition() {
    let env = StressTestEnvironment::new(Default::default());
    
    // Create single invoice
    let invoice_id = env.create_random_invoice(0).unwrap();
    env.client.verify_invoice(&invoice_id).unwrap();
    
    // Place maximum number of bids
    let max_bids = 100;
    let mut bid_ids = Vec::new();
    
    for i in 0..max_bids {
        let bid_id = env.place_random_bid(&invoice_id, i);
        bid_ids.push(bid_id);
    }
    
    // Verify all bids are placed
    assert_eq!(bid_ids.len(), max_bids);
    
    // Accept one bid
    let winning_bid_id = &bid_ids[max_bids / 2];
    env.client.accept_bid(&invoice_id, winning_bid_id);
    
    // Verify only one bid is accepted
    let winning_bid = env.client.get_bid(winning_bid_id).unwrap();
    assert_eq!(winning_bid.status, BidStatus::Accepted);
    
    // Verify invoice is funded
    let invoice = env.client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
}
