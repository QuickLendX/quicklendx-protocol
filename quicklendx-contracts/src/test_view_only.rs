//! Tests for the view-only assertion mechanism.
//!
//! Verifies that `assert_view_only!` correctly panics when the view-only flag
//! is set and permits operations when it is not.

use crate::bid::BidStorage;
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
use crate::invoice::{Invoice, InvoiceCategory, InvoiceStatus};
use crate::storage::{InvoiceStorage, StorageManager};
use crate::types::{Bid, BidStatus};
use crate::QuickLendXContract;
use soroban_sdk::{testutils::Address as _, Address, BytesN, Env, String, Vec};

#[test]
fn test_view_only_allows_writes_normally() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1u8; 32]);
    let invoice = Invoice {
        id: invoice_id.clone(),
        business: business.clone(),
        amount: 1000,
        currency: Address::generate(&env),
        due_date: 100,
        description: String::from_str(&env, "test"),
        status: InvoiceStatus::Pending,
        category: InvoiceCategory::Services,
        tags: Vec::new(&env),
        created_at: 0,
        settled_at: None,
        funded_amount: 0,
        funded_at: None,
        average_rating: None,
        total_ratings: 0,
        total_paid: 0,
        investor: None,
        dispute_status: crate::invoice::DisputeStatus::None,
        dispute: crate::types::Dispute {
            created_by: business.clone(),
            created_at: 0,
            reason: String::from_str(&env, ""),
            evidence: String::from_str(&env, ""),
            resolution: String::from_str(&env, ""),
            resolved_by: business.clone(),
            resolved_at: 0,
            resolution_outcome: crate::types::DisputeResolution::None,
        },
        payment_history: Vec::new(&env),
        ratings: Vec::new(&env),
        metadata_customer_name: None,
        metadata_customer_address: None,
        metadata_tax_id: None,
        metadata_notes: None,
        metadata_line_items: Vec::new(&env),
    };

    // Should NOT panic
    env.as_contract(&contract_id, || {
        InvoiceStorage::store(&env, &invoice);
        assert!(InvoiceStorage::get(&env, &invoice_id).is_some());
    });
}

#[test]
#[should_panic(expected = "illegal state write attempted in view-only context")]
fn test_view_only_panics_on_invoice_store() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    let business = Address::generate(&env);
    let invoice = Invoice {
        id: BytesN::from_array(&env, &[1u8; 32]),
        business: business.clone(),
        amount: 1000,
        currency: Address::generate(&env),
        due_date: 100,
        description: String::from_str(&env, "test"),
        status: InvoiceStatus::Pending,
        category: InvoiceCategory::Services,
        tags: Vec::new(&env),
        created_at: 0,
        settled_at: None,
        funded_amount: 0,
        funded_at: None,
        average_rating: None,
        total_ratings: 0,
        total_paid: 0,
        investor: None,
        dispute_status: crate::invoice::DisputeStatus::None,
        dispute: crate::types::Dispute {
            created_by: business.clone(),
            created_at: 0,
            reason: String::from_str(&env, ""),
            evidence: String::from_str(&env, ""),
            resolution: String::from_str(&env, ""),
            resolved_by: business.clone(),
            resolved_at: 0,
            resolution_outcome: crate::types::DisputeResolution::None,
        },
        payment_history: Vec::new(&env),
        ratings: Vec::new(&env),
        metadata_customer_name: None,
        metadata_customer_address: None,
        metadata_tax_id: None,
        metadata_notes: None,
        metadata_line_items: Vec::new(&env),
    };

    env.as_contract(&contract_id, || {
        StorageManager::with_view_only(&env, || {
            InvoiceStorage::store(&env, &invoice);
        });
    });
}

#[test]
#[should_panic(expected = "illegal state write attempted in view-only context")]
fn test_view_only_panics_on_bid_store() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    let bid = Bid {
        bid_id: BytesN::from_array(&env, &[2u8; 32]),
        invoice_id: BytesN::from_array(&env, &[1u8; 32]),
        investor: Address::generate(&env),
        bid_amount: 500,
        expected_return: 600,
        timestamp: 0,
        status: BidStatus::Placed,
        expiration_timestamp: 1000,
    };

    env.as_contract(&contract_id, || {
        StorageManager::with_view_only(&env, || {
            BidStorage::store_bid(&env, &bid);
        });
    });
}

#[test]
#[should_panic(expected = "illegal state write attempted in view-only context")]
fn test_view_only_panics_on_investment_store() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    let investment = Investment {
        investment_id: BytesN::from_array(&env, &[3u8; 32]),
        invoice_id: BytesN::from_array(&env, &[1u8; 32]),
        investor: Address::generate(&env),
        amount: 1000,
        funded_at: 0,
        status: InvestmentStatus::Active,
        insurance: Vec::new(&env),
    };

    env.as_contract(&contract_id, || {
        StorageManager::with_view_only(&env, || {
            InvestmentStorage::store_investment(&env, &investment);
        });
    });
}

#[test]
fn test_view_only_restores_state_after_with_view_only() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());

    env.as_contract(&contract_id, || {
        assert!(!StorageManager::is_view_only(&env));

        StorageManager::with_view_only(&env, || {
            assert!(StorageManager::is_view_only(&env));
        });

        assert!(!StorageManager::is_view_only(&env));
    });
}
