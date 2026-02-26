//! Tests for the core types module

use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

use crate::bid::{Bid, BidStatus};
use crate::invoice::{
    Invoice, InvoiceCategory, InvoiceMetadata, InvoiceStatus, LineItemRecord,
    PaymentRecord,
};
use crate::investment::{InsuranceCoverage, Investment, InvestmentStatus};
use crate::profits::{PlatformFee, PlatformFeeConfig};

#[test]
fn test_invoice_status_enum() {
    let env = Env::default();

    // Test all invoice statuses
    assert_eq!(InvoiceStatus::Pending as u8, 0);
    assert_eq!(InvoiceStatus::Verified as u8, 1);
    assert_eq!(InvoiceStatus::Funded as u8, 2);
    assert_eq!(InvoiceStatus::Paid as u8, 3);
    assert_eq!(InvoiceStatus::Defaulted as u8, 4);
    assert_eq!(InvoiceStatus::Cancelled as u8, 5);

    // Test clone and equality
    let status1 = InvoiceStatus::Verified;
    let status2 = status1.clone();
    assert_eq!(status1, status2);
}

#[test]
fn test_bid_status_enum() {
    // Test all bid statuses
    assert_eq!(BidStatus::Placed as u8, 0);
    assert_eq!(BidStatus::Withdrawn as u8, 1);
    assert_eq!(BidStatus::Accepted as u8, 2);
    assert_eq!(BidStatus::Expired as u8, 3);

    // Test clone and equality
    let status1 = BidStatus::Accepted;
    let status2 = status1.clone();
    assert_eq!(status1, status2);
}

#[test]
fn test_investment_status_enum() {
    // Test all investment statuses
    assert_eq!(InvestmentStatus::Active as u8, 0);
    assert_eq!(InvestmentStatus::Withdrawn as u8, 1);
    assert_eq!(InvestmentStatus::Completed as u8, 2);
    assert_eq!(InvestmentStatus::Defaulted as u8, 3);

    // Test clone and equality
    let status1 = InvestmentStatus::Completed;
    let status2 = status1.clone();
    assert_eq!(status1, status2);
}


#[test]
fn test_invoice_category_enum() {
    // Test all invoice categories
    assert_eq!(InvoiceCategory::Services as u8, 0);
    assert_eq!(InvoiceCategory::Products as u8, 1);
    assert_eq!(InvoiceCategory::Consulting as u8, 2);
    assert_eq!(InvoiceCategory::Manufacturing as u8, 3);
    assert_eq!(InvoiceCategory::Technology as u8, 4);
    assert_eq!(InvoiceCategory::Healthcare as u8, 5);
    assert_eq!(InvoiceCategory::Other as u8, 6);

    // Test clone and equality
    let category1 = InvoiceCategory::Technology;
    let category2 = category1.clone();
    assert_eq!(category1, category2);
}

#[test]
fn test_line_item_record() {
    let env = Env::default();

    let description = String::from_str(&env, "Service fee");
    let record = LineItemRecord(description.clone(), 1000, 1, 1000);

    assert_eq!(record.0, description);
    assert_eq!(record.1, 1000); // quantity
    assert_eq!(record.2, 1);    // unit price
    assert_eq!(record.3, 1000); // total

    // Test clone and equality
    let record2 = record.clone();
    assert_eq!(record, record2);
}

#[test]
fn test_invoice_metadata() {
    let env = Env::default();

    let customer_name = String::from_str(&env, "ABC Corp");
    let customer_address = String::from_str(&env, "123 Main St");
    let tax_id = String::from_str(&env, "123456789");
    let line_items = Vec::new(&env);
    let notes = String::from_str(&env, "Urgent payment required");

    let metadata = InvoiceMetadata {
        customer_name: customer_name.clone(),
        customer_address: customer_address.clone(),
        tax_id: tax_id.clone(),
        line_items: line_items.clone(),
        notes: notes.clone(),
    };

    assert_eq!(metadata.customer_name, customer_name);
    assert_eq!(metadata.customer_address, customer_address);
    assert_eq!(metadata.tax_id, tax_id);
    assert_eq!(metadata.line_items, line_items);
    assert_eq!(metadata.notes, notes);

    // Test clone and equality
    let metadata2 = metadata.clone();
    assert_eq!(metadata, metadata2);
}

#[test]
fn test_payment_record() {
    let env = Env::default();

    let transaction_id = String::from_str(&env, "tx_123456");
    let record = PaymentRecord {
        amount: 5000,
        timestamp: 1234567890,
        transaction_id: transaction_id.clone(),
    };

    assert_eq!(record.amount, 5000);
    assert_eq!(record.timestamp, 1234567890);
    assert_eq!(record.transaction_id, transaction_id);

    // Test clone and equality
    let record2 = record.clone();
    assert_eq!(record, record2);
}



#[test]
fn test_invoice() {
    let env = Env::default();

    let id = BytesN::from_array(&env, &[1; 32]);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let description = String::from_str(&env, "Consulting services");
    let tags = Vec::new(&env);
    let metadata = InvoiceMetadata {
        customer_name: String::from_str(&env, "ABC Corp"),
        customer_address: String::from_str(&env, "123 Main St"),
        tax_id: String::from_str(&env, "123456789"),
        line_items: Vec::new(&env),
        notes: String::from_str(&env, "Notes"),
    };
    let payments = Vec::new(&env);

    let invoice = Invoice {
        id: id.clone(),
        business: business.clone(),
        amount: 10000,
        currency: currency.clone(),
        due_date: 1234567890,
        status: InvoiceStatus::Pending,
        description: description.clone(),
        category: InvoiceCategory::Consulting,
        tags: tags.clone(),
        metadata_customer_name: Some(metadata.customer_name.clone()),
        metadata_customer_address: Some(metadata.customer_address.clone()),
        metadata_tax_id: Some(metadata.tax_id.clone()),
        metadata_notes: Some(metadata.notes.clone()),
        metadata_line_items: metadata.line_items.clone(),
        funded_amount: 0,
        funded_at: None,
        investor: None,
        settled_at: None,
        total_paid: 0,
        payment_history: payments.clone(),
        created_at: 1234567890,
    };

    assert_eq!(invoice.id, id);
    assert_eq!(invoice.business, business);
    assert_eq!(invoice.amount, 10000);
}

#[test]
fn test_bid() {
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

    assert_eq!(bid.bid_id, bid_id);
    assert_eq!(bid.invoice_id, invoice_id);
    assert_eq!(bid.investor, investor);
    assert_eq!(bid.bid_amount, 9000);
    assert_eq!(bid.expected_return, 9500);
    assert_eq!(bid.timestamp, 1234567890);
    assert_eq!(bid.status, BidStatus::Placed);
    assert_eq!(bid.expiration_timestamp, 1234567890 + 7 * 24 * 60 * 60);

    // Test clone and equality
    let bid2 = bid.clone();
    assert_eq!(bid, bid2);
}

#[test]
fn test_insurance_coverage() {
    let env = Env::default();

    let provider = Address::generate(&env);

    let coverage = InsuranceCoverage {
        provider: provider.clone(),
        coverage_amount: 8000,
        premium_amount: 80,
        coverage_percentage: 80,
        active: true,
    };

    assert_eq!(coverage.provider, provider);
    assert_eq!(coverage.coverage_amount, 8000);
    assert_eq!(coverage.premium_amount, 80);
    assert_eq!(coverage.coverage_percentage, 80);
    assert_eq!(coverage.active, true);

    // Test clone and equality
    let coverage2 = coverage.clone();
    assert_eq!(coverage, coverage2);
}

#[test]
fn test_investment() {
    let env = Env::default();

    let investment_id = BytesN::from_array(&env, &[3; 32]);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let investor = Address::generate(&env);
    let insurance = Vec::new(&env);

    let investment = Investment {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount: 9000,
        funded_at: 1234567890,
        status: InvestmentStatus::Active,
        insurance: insurance.clone(),
    };

    assert_eq!(investment.investment_id, investment_id);
    assert_eq!(investment.invoice_id, invoice_id);
    assert_eq!(investment.investor, investor);
    assert_eq!(investment.amount, 9000);
    assert_eq!(investment.funded_at, 1234567890);
    assert_eq!(investment.status, InvestmentStatus::Active);
    assert_eq!(investment.insurance, insurance);

    // Test clone and equality
    let investment2 = investment.clone();
    assert_eq!(investment, investment2);
}

