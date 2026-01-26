//! Tests for the storage module

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

use crate::storage::{BidStorage, ConfigStorage, Indexes, InvestmentStorage, InvoiceStorage, StorageKeys};
use crate::types::{
    Bid, BidStatus, InsuranceCoverage, Investment, InvestmentStatus, Invoice, InvoiceCategory,
    InvoiceMetadata, InvoiceStatus, PaymentRecord, PlatformFee, PlatformFeeConfig,
};

#[test]
fn test_storage_keys() {
    let env = Env::default();
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let bid_id = BytesN::from_array(&env, &[2; 32]);
    let investment_id = BytesN::from_array(&env, &[3; 32]);

    // Test invoice key
    let key = StorageKeys::invoice(&invoice_id);
    assert_eq!(key, invoice_id);

    // Test bid key
    let key = StorageKeys::bid(&bid_id);
    assert_eq!(key, bid_id);

    // Test investment key
    let key = StorageKeys::investment(&investment_id);
    assert_eq!(key, investment_id);

    // Test platform fees key
    let key = StorageKeys::platform_fees();
    assert_eq!(key, soroban_sdk::symbol_short!("fees"));
}

#[test]
fn test_indexes() {
    let env = Env::default();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);

    // Test invoice by business index
    let (symbol, addr) = Indexes::invoices_by_business(&business);
    assert_eq!(symbol, soroban_sdk::symbol_short!("inv_bus"));
    assert_eq!(addr, business);

    // Test invoice by status indexes
    let (symbol, status_symbol) = Indexes::invoices_by_status(InvoiceStatus::Pending);
    assert_eq!(symbol, soroban_sdk::symbol_short!("inv_stat"));
    assert_eq!(status_symbol, soroban_sdk::symbol_short!("pending"));

    let (symbol, status_symbol) = Indexes::invoices_by_status(InvoiceStatus::Verified);
    assert_eq!(status_symbol, soroban_sdk::symbol_short!("verified"));

    // Test bid indexes
    let (symbol, id) = Indexes::bids_by_invoice(&invoice_id);
    assert_eq!(symbol, soroban_sdk::symbol_short!("bids_inv"));
    assert_eq!(id, invoice_id);

    let (symbol, addr) = Indexes::bids_by_investor(&investor);
    assert_eq!(symbol, soroban_sdk::symbol_short!("bids_invstr"));
    assert_eq!(addr, investor);

    let (symbol, status_symbol) = Indexes::bids_by_status(BidStatus::Placed);
    assert_eq!(symbol, soroban_sdk::symbol_short!("bids_stat"));
    assert_eq!(status_symbol, soroban_sdk::symbol_short!("placed"));

    // Test investment indexes
    let (symbol, id) = Indexes::investments_by_invoice(&invoice_id);
    assert_eq!(symbol, soroban_sdk::symbol_short!("invst_inv"));
    assert_eq!(id, invoice_id);

    let (symbol, addr) = Indexes::investments_by_investor(&investor);
    assert_eq!(symbol, soroban_sdk::symbol_short!("invst_invstr"));
    assert_eq!(addr, investor);

    let (symbol, status_symbol) = Indexes::investments_by_status(InvestmentStatus::Active);
    assert_eq!(symbol, soroban_sdk::symbol_short!("invst_stat"));
    assert_eq!(status_symbol, soroban_sdk::symbol_short!("active"));
}

#[test]
fn test_invoice_storage() {
    let env = Env::default();

    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "ABC Corp"),
        customer_address: String::from_str(&env, "123 Main St"),
        tax_id: String::from_str(&env, "123456789"),
        line_items: Vec::new(&env),
        notes: String::from_str(&env, "Notes"),
    };

    let dispute = crate::types::Dispute {
        created_by: Address::generate(&env),
        created_at: 0,
        reason: String::from_str(&env, ""),
        evidence: String::from_str(&env, ""),
        resolution: String::from_str(&env, ""),
        resolved_by: Address::generate(&env),
        resolved_at: 0,
    };

    let invoice = Invoice {
        id: invoice_id.clone(),
        business: business.clone(),
        amount: 10000,
        currency: currency.clone(),
        due_date: 1234567890,
        status: InvoiceStatus::Pending,
        description: String::from_str(&env, "Consulting services"),
        category: InvoiceCategory::Consulting,
        tags: Vec::new(&env),
        metadata: metadata.clone(),
        dispute: dispute.clone(),
        payments: Vec::new(&env),
        ratings: Vec::new(&env),
        created_at: 1234567890,
        updated_at: 1234567890,
    };

    // Test storing invoice
    InvoiceStorage::store(&env, &invoice);

    // Test retrieving invoice
    let retrieved = InvoiceStorage::get(&env, &invoice_id).unwrap();
    assert_eq!(retrieved, invoice);

    // Test getting invoices by business
    let business_invoices = InvoiceStorage::get_by_business(&env, &business);
    assert_eq!(business_invoices.len(), 1);
    assert_eq!(business_invoices.get(0).unwrap(), invoice_id);

    // Test getting invoices by status
    let pending_invoices = InvoiceStorage::get_by_status(&env, InvoiceStatus::Pending);
    assert_eq!(pending_invoices.len(), 1);
    assert_eq!(pending_invoices.get(0).unwrap(), invoice_id);

    // Test updating invoice status
    let mut updated_invoice = invoice.clone();
    updated_invoice.status = InvoiceStatus::Verified;
    InvoiceStorage::update(&env, &updated_invoice);

    let retrieved_updated = InvoiceStorage::get(&env, &invoice_id).unwrap();
    assert_eq!(retrieved_updated.status, InvoiceStatus::Verified);

    // Check that indexes are updated
    let verified_invoices = InvoiceStorage::get_by_status(&env, InvoiceStatus::Verified);
    assert_eq!(verified_invoices.len(), 1);
    assert_eq!(verified_invoices.get(0).unwrap(), invoice_id);

    let pending_invoices_after = InvoiceStorage::get_by_status(&env, InvoiceStatus::Pending);
    assert_eq!(pending_invoices_after.len(), 0);

    // Test invoice counter
    let count1 = InvoiceStorage::next_count(&env);
    let count2 = InvoiceStorage::next_count(&env);
    assert_eq!(count1, 1);
    assert_eq!(count2, 2);
}

#[test]
fn test_bid_storage() {
    let env = Env::default();

    let bid_id = BytesN::from_array(&env, &[2; 32]);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let investor = Address::generate(&env);

    let bid = Bid {
        bid_id: bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount: 9000,
        expected_return: 9500,
        timestamp: 1234567890,
        status: BidStatus::Placed,
        expiration_timestamp: 1234567890 + 7 * 24 * 60 * 60,
    };

    // Test storing bid
    BidStorage::store(&env, &bid);

    // Test retrieving bid
    let retrieved = BidStorage::get(&env, &bid_id).unwrap();
    assert_eq!(retrieved, bid);

    // Test getting bids by invoice
    let invoice_bids = BidStorage::get_by_invoice(&env, &invoice_id);
    assert_eq!(invoice_bids.len(), 1);
    assert_eq!(invoice_bids.get(0).unwrap(), bid_id);

    // Test getting bids by investor
    let investor_bids = BidStorage::get_by_investor(&env, &investor);
    assert_eq!(investor_bids.len(), 1);
    assert_eq!(investor_bids.get(0).unwrap(), bid_id);

    // Test getting bids by status
    let placed_bids = BidStorage::get_by_status(&env, BidStatus::Placed);
    assert_eq!(placed_bids.len(), 1);
    assert_eq!(placed_bids.get(0).unwrap(), bid_id);

    // Test updating bid status
    let mut updated_bid = bid.clone();
    updated_bid.status = BidStatus::Accepted;
    BidStorage::update(&env, &updated_bid);

    let retrieved_updated = BidStorage::get(&env, &bid_id).unwrap();
    assert_eq!(retrieved_updated.status, BidStatus::Accepted);

    // Check that indexes are updated
    let accepted_bids = BidStorage::get_by_status(&env, BidStatus::Accepted);
    assert_eq!(accepted_bids.len(), 1);
    assert_eq!(accepted_bids.get(0).unwrap(), bid_id);

    let placed_bids_after = BidStorage::get_by_status(&env, BidStatus::Placed);
    assert_eq!(placed_bids_after.len(), 0);

    // Test bid counter
    let count1 = BidStorage::next_count(&env);
    let count2 = BidStorage::next_count(&env);
    assert_eq!(count1, 1);
    assert_eq!(count2, 2);
}

#[test]
fn test_investment_storage() {
    let env = Env::default();

    let investment_id = BytesN::from_array(&env, &[3; 32]);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let investor = Address::generate(&env);

    let investment = Investment {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount: 9000,
        funded_at: 1234567890,
        status: InvestmentStatus::Active,
        insurance: Vec::new(&env),
    };

    // Test storing investment
    InvestmentStorage::store(&env, &investment);

    // Test retrieving investment
    let retrieved = InvestmentStorage::get(&env, &investment_id).unwrap();
    assert_eq!(retrieved, investment);

    // Test getting investments by invoice
    let invoice_investments = InvestmentStorage::get_by_invoice(&env, &invoice_id);
    assert_eq!(invoice_investments.len(), 1);
    assert_eq!(invoice_investments.get(0).unwrap(), investment_id);

    // Test getting investments by investor
    let investor_investments = InvestmentStorage::get_by_investor(&env, &investor);
    assert_eq!(investor_investments.len(), 1);
    assert_eq!(investor_investments.get(0).unwrap(), investment_id);

    // Test getting investments by status
    let active_investments = InvestmentStorage::get_by_status(&env, InvestmentStatus::Active);
    assert_eq!(active_investments.len(), 1);
    assert_eq!(active_investments.get(0).unwrap(), investment_id);

    // Test updating investment status
    let mut updated_investment = investment.clone();
    updated_investment.status = InvestmentStatus::Completed;
    InvestmentStorage::update(&env, &updated_investment);

    let retrieved_updated = InvestmentStorage::get(&env, &investment_id).unwrap();
    assert_eq!(retrieved_updated.status, InvestmentStatus::Completed);

    // Check that indexes are updated
    let completed_investments = InvestmentStorage::get_by_status(&env, InvestmentStatus::Completed);
    assert_eq!(completed_investments.len(), 1);
    assert_eq!(completed_investments.get(0).unwrap(), investment_id);

    let active_investments_after = InvestmentStorage::get_by_status(&env, InvestmentStatus::Active);
    assert_eq!(active_investments_after.len(), 0);

    // Test investment counter
    let count1 = InvestmentStorage::next_count(&env);
    let count2 = InvestmentStorage::next_count(&env);
    assert_eq!(count1, 1);
    assert_eq!(count2, 2);
}

#[test]
fn test_config_storage() {
    let env = Env::default();

    let recipient = Address::generate(&env);

    let config = PlatformFeeConfig {
        verification_fee: PlatformFee {
            fee_bps: 25,
            recipient: recipient.clone(),
            description: String::from_str(&env, "Verification fee"),
        },
        settlement_fee: PlatformFee {
            fee_bps: 50,
            recipient: recipient.clone(),
            description: String::from_str(&env, "Settlement fee"),
        },
        bid_fee: PlatformFee {
            fee_bps: 10,
            recipient: recipient.clone(),
            description: String::from_str(&env, "Bid fee"),
        },
        investment_fee: PlatformFee {
            fee_bps: 20,
            recipient: recipient.clone(),
            description: String::from_str(&env, "Investment fee"),
        },
    };

    // Test storing config
    ConfigStorage::set_platform_fees(&env, &config);

    // Test retrieving config
    let retrieved = ConfigStorage::get_platform_fees(&env).unwrap();
    assert_eq!(retrieved, config);
}

#[test]
fn test_storage_isolation() {
    let env = Env::default();

    // Create different entities
    let invoice_id1 = BytesN::from_array(&env, &[1; 32]);
    let invoice_id2 = BytesN::from_array(&env, &[2; 32]);
    let business1 = Address::generate(&env);
    let business2 = Address::generate(&env);

    // Create invoices for different businesses
    let invoice1 = create_test_invoice(&env, invoice_id1.clone(), business1.clone());
    let invoice2 = create_test_invoice(&env, invoice_id2.clone(), business2.clone());

    InvoiceStorage::store(&env, &invoice1);
    InvoiceStorage::store(&env, &invoice2);

    // Test that businesses have separate invoice lists
    let business1_invoices = InvoiceStorage::get_by_business(&env, &business1);
    let business2_invoices = InvoiceStorage::get_by_business(&env, &business2);

    assert_eq!(business1_invoices.len(), 1);
    assert_eq!(business2_invoices.len(), 1);
    assert_eq!(business1_invoices.get(0).unwrap(), invoice_id1);
    assert_eq!(business2_invoices.get(0).unwrap(), invoice_id2);
}

fn create_test_invoice(env: &Env, id: BytesN<32>, business: Address) -> Invoice {
    let currency = Address::generate(env);

    let metadata = InvoiceMetadata {
        customer_name: String::from_str(env, "Test Corp"),
        customer_address: String::from_str(env, "123 Test St"),
        tax_id: String::from_str(env, "123456789"),
        line_items: Vec::new(env),
        notes: String::from_str(env, "Test notes"),
    };

    let dispute = crate::types::Dispute {
        created_by: Address::generate(env),
        created_at: 0,
        reason: String::from_str(env, ""),
        evidence: String::from_str(env, ""),
        resolution: String::from_str(env, ""),
        resolved_by: Address::generate(env),
        resolved_at: 0,
    };

    Invoice {
        id,
        business,
        amount: 10000,
        currency,
        due_date: 1234567890,
        status: InvoiceStatus::Pending,
        description: String::from_str(env, "Test invoice"),
        category: InvoiceCategory::Services,
        tags: Vec::new(env),
        metadata,
        dispute,
        payments: Vec::new(env),
        ratings: Vec::new(env),
        created_at: 1234567890,
        updated_at: 1234567890,
    }
}