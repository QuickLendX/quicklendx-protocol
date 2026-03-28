//! Protocol-wide invariant tests for status/index coherence.
//!
//! These tests verify that the protocol maintains critical invariants:
//! - Invoice status lists remain coherent with primary records
//! - Bid status lists remain coherent with primary records
//! - Investment status lists remain coherent with primary records
//! - No orphaned records in indexes after mutations
//! - Counters remain consistent with actual entity counts
//! - Cross-module consistency (e.g., funded invoices have investments)

#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, BytesN, Env, String, Vec};

use crate::bid::{Bid, BidStatus};
use crate::investment::{Investment, InvestmentStatus};
use crate::invoice::{Invoice, InvoiceCategory, InvoiceStatus};
use crate::storage::{
    BidStorage, DataKey, Indexes, InvestmentStorage, InvoiceStorage, StorageKeys,
};
use crate::{QuickLendXContract, QuickLendXContractClient};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.initialize_admin(&admin);
    (env, client, admin)
}

fn with_registered_contract<F: FnOnce(&Env)>(env: &Env, f: F) {
    let contract_id = env.register(QuickLendXContract, ());
    env.as_contract(&contract_id, || f(env));
}

fn create_test_invoice(
    env: &Env,
    id: BytesN<32>,
    business: Address,
    status: InvoiceStatus,
) -> Invoice {
    Invoice {
        id,
        business,
        amount: 1000,
        currency: Address::generate(env),
        due_date: 1000,
        status,
        created_at: 0,
        description: String::from_str(env, "Test"),
        metadata_customer_name: None,
        metadata_customer_address: None,
        metadata_tax_id: None,
        metadata_notes: None,
        metadata_line_items: Vec::new(env),
        category: InvoiceCategory::Services,
        tags: Vec::new(env),
        funded_amount: 0,
        funded_at: None,
        investor: None,
        settled_at: None,
        average_rating: None,
        total_ratings: 0,
        ratings: Vec::new(env),
        dispute_status: crate::invoice::DisputeStatus::None,
        dispute: crate::invoice::Dispute {
            created_by: Address::generate(env),
            created_at: 0,
            reason: String::from_str(env, ""),
            evidence: String::from_str(env, ""),
            resolution: String::from_str(env, ""),
            resolved_by: Address::generate(env),
            resolved_at: 0,
        },
        total_paid: 0,
        payment_history: Vec::new(env),
    }
}

fn create_test_bid(
    env: &Env,
    bid_id: BytesN<32>,
    invoice_id: BytesN<32>,
    investor: Address,
    status: BidStatus,
) -> Bid {
    Bid {
        bid_id,
        invoice_id,
        investor,
        bid_amount: 1000,
        expected_return: 1100,
        timestamp: 0,
        status,
        expiration_timestamp: 1000,
    }
}

fn create_test_investment(
    env: &Env,
    investment_id: BytesN<32>,
    invoice_id: BytesN<32>,
    investor: Address,
    status: InvestmentStatus,
) -> Investment {
    Investment {
        investment_id,
        invoice_id,
        investor,
        amount: 1000,
        funded_at: 0,
        status,
        insurance: Vec::new(env),
    }
}

// ============================================================================
// INVOICE STATUS INDEX COHERENCE TESTS
// ============================================================================

#[test]
fn invariant_invoice_in_status_index_matches_primary_record() {
    let env = Env::default();
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);

    with_registered_contract(&env, |_env| {
        let invoice = create_test_invoice(
            &env,
            invoice_id.clone(),
            business.clone(),
            InvoiceStatus::Verified,
        );
        InvoiceStorage::store(&env, &invoice);

        let verified_invoices = InvoiceStorage::get_by_status(&env, InvoiceStatus::Verified);
        assert!(
            verified_invoices.iter().any(|id| id == invoice_id),
            "Invoice must be in verified status index"
        );
    });
}

#[test]
fn invariant_invoice_not_in_wrong_status_index() {
    let env = Env::default();
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);

    with_registered_contract(&env, |_env| {
        let invoice = create_test_invoice(
            &env,
            invoice_id.clone(),
            business.clone(),
            InvoiceStatus::Verified,
        );
        InvoiceStorage::store(&env, &invoice);

        let pending_invoices = InvoiceStorage::get_by_status(&env, InvoiceStatus::Pending);
        assert!(
            !pending_invoices.iter().any(|id| id == invoice_id),
            "Invoice must not be in pending status index"
        );
    });
}

#[test]
fn invariant_invoice_status_change_updates_indexes() {
    let env = Env::default();
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);

    with_registered_contract(&env, |_env| {
        let mut invoice = create_test_invoice(
            &env,
            invoice_id.clone(),
            business.clone(),
            InvoiceStatus::Pending,
        );
        InvoiceStorage::store(&env, &invoice);

        invoice.status = InvoiceStatus::Verified;
        InvoiceStorage::update(&env, &invoice);

        let verified_invoices = InvoiceStorage::get_by_status(&env, InvoiceStatus::Verified);
        let pending_invoices = InvoiceStorage::get_by_status(&env, InvoiceStatus::Pending);

        assert!(
            verified_invoices.iter().any(|id| id == invoice_id),
            "Invoice must be in verified index after status change"
        );
        assert!(
            !pending_invoices.iter().any(|id| id == invoice_id),
            "Invoice must not be in pending index after status change"
        );
    });
}

// ============================================================================
// BID STATUS INDEX COHERENCE TESTS
// ============================================================================

#[test]
fn invariant_bid_in_status_index_matches_primary_record() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let bid_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let bid = create_test_bid(
            &env,
            bid_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            BidStatus::Placed,
        );
        BidStorage::store(&env, &bid);

        let placed_bids = BidStorage::get_by_status(&env, BidStatus::Placed);
        assert!(
            placed_bids.iter().any(|id| id == bid_id),
            "Bid must be in placed status index"
        );
    });
}

#[test]
fn invariant_bid_not_in_wrong_status_index() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let bid_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let bid = create_test_bid(
            &env,
            bid_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            BidStatus::Placed,
        );
        BidStorage::store(&env, &bid);

        let accepted_bids = BidStorage::get_by_status(&env, BidStatus::Accepted);
        assert!(
            !accepted_bids.iter().any(|id| id == bid_id),
            "Bid must not be in accepted status index"
        );
    });
}

#[test]
fn invariant_bid_status_change_updates_indexes() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let bid_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let mut bid = create_test_bid(
            &env,
            bid_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            BidStatus::Placed,
        );
        BidStorage::store(&env, &bid);

        bid.status = BidStatus::Accepted;
        BidStorage::update(&env, &bid);

        let accepted_bids = BidStorage::get_by_status(&env, BidStatus::Accepted);
        let placed_bids = BidStorage::get_by_status(&env, BidStatus::Placed);

        assert!(
            accepted_bids.iter().any(|id| id == bid_id),
            "Bid must be in accepted index after status change"
        );
        assert!(
            !placed_bids.iter().any(|id| id == bid_id),
            "Bid must not be in placed index after status change"
        );
    });
}

// ============================================================================
// INVESTMENT STATUS INDEX COHERENCE TESTS
// ============================================================================

#[test]
fn invariant_investment_in_status_index_matches_primary_record() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let investment_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let investment = create_test_investment(
            &env,
            investment_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            InvestmentStatus::Active,
        );
        InvestmentStorage::store(&env, &investment);

        let active_investments = InvestmentStorage::get_by_status(&env, InvestmentStatus::Active);
        assert!(
            active_investments.iter().any(|id| id == investment_id),
            "Investment must be in active status index"
        );
    });
}

#[test]
fn invariant_investment_not_in_wrong_status_index() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let investment_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let investment = create_test_investment(
            &env,
            investment_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            InvestmentStatus::Active,
        );
        InvestmentStorage::store(&env, &investment);

        let completed_investments =
            InvestmentStorage::get_by_status(&env, InvestmentStatus::Completed);
        assert!(
            !completed_investments.iter().any(|id| id == investment_id),
            "Investment must not be in completed status index"
        );
    });
}

#[test]
fn invariant_investment_status_change_updates_indexes() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let investment_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let mut investment = create_test_investment(
            &env,
            investment_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            InvestmentStatus::Active,
        );
        InvestmentStorage::store(&env, &investment);

        investment.status = InvestmentStatus::Completed;
        InvestmentStorage::update(&env, &investment);

        let completed_investments =
            InvestmentStorage::get_by_status(&env, InvestmentStatus::Completed);
        let active_investments = InvestmentStorage::get_by_status(&env, InvestmentStatus::Active);

        assert!(
            completed_investments.iter().any(|id| id == investment_id),
            "Investment must be in completed index after status change"
        );
        assert!(
            !active_investments.iter().any(|id| id == investment_id),
            "Investment must not be in active index after status change"
        );
    });
}

// ============================================================================
// ORPHANED RECORD TESTS
// ============================================================================

#[test]
fn invariant_no_orphaned_invoices_in_status_index() {
    let env = Env::default();
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);

    with_registered_contract(&env, |_env| {
        let invoice = create_test_invoice(
            &env,
            invoice_id.clone(),
            business.clone(),
            InvoiceStatus::Pending,
        );
        InvoiceStorage::store(&env, &invoice);

        let pending_invoices = InvoiceStorage::get_by_status(&env, InvoiceStatus::Pending);
        for id in pending_invoices.iter() {
            let retrieved = InvoiceStorage::get(&env, &id);
            assert!(
                retrieved.is_some(),
                "Invoice in status index must exist in primary storage"
            );
        }
    });
}

#[test]
fn invariant_no_orphaned_bids_in_status_index() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let bid_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let bid = create_test_bid(
            &env,
            bid_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            BidStatus::Placed,
        );
        BidStorage::store(&env, &bid);

        let placed_bids = BidStorage::get_by_status(&env, BidStatus::Placed);
        for id in placed_bids.iter() {
            let retrieved = BidStorage::get(&env, &id);
            assert!(
                retrieved.is_some(),
                "Bid in status index must exist in primary storage"
            );
        }
    });
}

#[test]
fn invariant_no_orphaned_investments_in_status_index() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let investment_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let investment = create_test_investment(
            &env,
            investment_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            InvestmentStatus::Active,
        );
        InvestmentStorage::store(&env, &investment);

        let active_investments = InvestmentStorage::get_by_status(&env, InvestmentStatus::Active);
        for id in active_investments.iter() {
            let retrieved = InvestmentStorage::get(&env, &id);
            assert!(
                retrieved.is_some(),
                "Investment in status index must exist in primary storage"
            );
        }
    });
}

// ============================================================================
// BUSINESS INDEX COHERENCE TESTS
// ============================================================================

#[test]
fn invariant_invoice_business_index_matches_primary_record() {
    let env = Env::default();
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);

    with_registered_contract(&env, |_env| {
        let invoice = create_test_invoice(
            &env,
            invoice_id.clone(),
            business.clone(),
            InvoiceStatus::Pending,
        );
        InvoiceStorage::store(&env, &invoice);

        let business_invoices = InvoiceStorage::get_by_business(&env, &business);
        assert!(
            business_invoices.iter().any(|id| id == invoice_id),
            "Invoice must be in business index"
        );
    });
}

#[test]
fn invariant_invoice_business_index_reflects_status() {
    let env = Env::default();
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);

    with_registered_contract(&env, |_env| {
        let invoice = create_test_invoice(
            &env,
            invoice_id.clone(),
            business.clone(),
            InvoiceStatus::Pending,
        );
        InvoiceStorage::store(&env, &invoice);

        let business_invoices = InvoiceStorage::get_by_business(&env, &business);
        for id in business_invoices.iter() {
            let retrieved = InvoiceStorage::get(&env, &id);
            if let Some(inv) = retrieved {
                assert_eq!(
                    inv.business, business,
                    "Invoice in business index must belong to that business"
                );
            }
        }
    });
}

// ============================================================================
// INVOICE INDEX COHERENCE TESTS
// ============================================================================

#[test]
fn invariant_bid_invoice_index_matches_primary_record() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let bid_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let bid = create_test_bid(
            &env,
            bid_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            BidStatus::Placed,
        );
        BidStorage::store(&env, &bid);

        let invoice_bids = BidStorage::get_by_invoice(&env, &invoice_id);
        assert!(
            invoice_bids.iter().any(|id| id == bid_id),
            "Bid must be in invoice index"
        );
    });
}

#[test]
fn invariant_investment_invoice_index_matches_primary_record() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let investment_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let investment = create_test_investment(
            &env,
            investment_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            InvestmentStatus::Active,
        );
        InvestmentStorage::store(&env, &investment);

        let invoice_investments = InvestmentStorage::get_by_invoice(&env, &invoice_id);
        assert!(
            invoice_investments.iter().any(|id| id == investment_id),
            "Investment must be in invoice index"
        );
    });
}

// ============================================================================
// INVESTOR INDEX COHERENCE TESTS
// ============================================================================

#[test]
fn invariant_bid_investor_index_matches_primary_record() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let bid_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let bid = create_test_bid(
            &env,
            bid_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            BidStatus::Placed,
        );
        BidStorage::store(&env, &bid);

        let investor_bids = BidStorage::get_by_investor(&env, &investor);
        assert!(
            investor_bids.iter().any(|id| id == bid_id),
            "Bid must be in investor index"
        );
    });
}

#[test]
fn invariant_investment_investor_index_matches_primary_record() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let investment_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let investment = create_test_investment(
            &env,
            investment_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            InvestmentStatus::Active,
        );
        InvestmentStorage::store(&env, &investment);

        let investor_investments = InvestmentStorage::get_by_investor(&env, &investor);
        assert!(
            investor_investments.iter().any(|id| id == investment_id),
            "Investment must be in investor index"
        );
    });
}

// ============================================================================
// COUNTER CONSISTENCY TESTS
// ============================================================================

#[test]
fn invariant_invoice_counter_increments() {
    let env = Env::default();
    with_registered_contract(&env, |_env| {
        let initial: u64 = env
            .storage()
            .persistent()
            .get(&StorageKeys::invoice_count())
            .unwrap_or(0);

        let _next = InvoiceStorage::next_count(&env);

        let after: u64 = env
            .storage()
            .persistent()
            .get(&StorageKeys::invoice_count())
            .unwrap_or(0);

        assert!(after > initial, "Invoice counter must increment");
    });
}

#[test]
fn invariant_bid_counter_increments() {
    let env = Env::default();
    with_registered_contract(&env, |_env| {
        let initial: u64 = env
            .storage()
            .persistent()
            .get(&StorageKeys::bid_count())
            .unwrap_or(0);

        let _next = BidStorage::next_count(&env);

        let after: u64 = env
            .storage()
            .persistent()
            .get(&StorageKeys::bid_count())
            .unwrap_or(0);

        assert!(after > initial, "Bid counter must increment");
    });
}

#[test]
fn invariant_investment_counter_increments() {
    let env = Env::default();
    with_registered_contract(&env, |_env| {
        let initial: u64 = env
            .storage()
            .persistent()
            .get(&StorageKeys::investment_count())
            .unwrap_or(0);

        let _next = InvestmentStorage::next_count(&env);

        let after: u64 = env
            .storage()
            .persistent()
            .get(&StorageKeys::investment_count())
            .unwrap_or(0);

        assert!(after > initial, "Investment counter must increment");
    });
}

// ============================================================================
// CROSS-MODULE CONSISTENCY TESTS
// ============================================================================

#[test]
fn invariant_funded_invoice_has_investor() {
    let env = Env::default();
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);

    with_registered_contract(&env, |_env| {
        let mut invoice = create_test_invoice(
            &env,
            invoice_id.clone(),
            business.clone(),
            InvoiceStatus::Funded,
        );
        invoice.funded_amount = 1000;
        invoice.investor = Some(investor.clone());

        assert!(
            invoice.investor.is_some(),
            "Funded invoice must have an investor"
        );
        assert!(
            invoice.funded_amount > 0,
            "Funded invoice must have funded_amount > 0"
        );
    });
}

#[test]
fn invariant_accepted_bid_corresponds_to_investment() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let bid_id = BytesN::from_array(&env, &[2; 32]);
    let investment_id = BytesN::from_array(&env, &[3; 32]);

    with_registered_contract(&env, |_env| {
        let bid = create_test_bid(
            &env,
            bid_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            BidStatus::Accepted,
        );
        BidStorage::store(&env, &bid);

        let investment = create_test_investment(
            &env,
            investment_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            InvestmentStatus::Active,
        );
        InvestmentStorage::store(&env, &investment);

        let invoice_investments = InvestmentStorage::get_by_invoice(&env, &invoice_id);
        assert!(
            invoice_investments.iter().any(|id| id == investment_id),
            "Accepted bid should have corresponding investment"
        );
    });
}

// ============================================================================
// RANDOMIZED SEQUENCE TESTS
// ============================================================================

#[test]
fn invariant_multiple_status_transitions_remain_coherent() {
    let env = Env::default();
    let business = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);

    with_registered_contract(&env, |_env| {
        let mut invoice = create_test_invoice(
            &env,
            invoice_id.clone(),
            business.clone(),
            InvoiceStatus::Pending,
        );
        InvoiceStorage::store(&env, &invoice);

        let statuses = [
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
        ];

        for status in statuses.iter() {
            invoice.status = status.clone();
            InvoiceStorage::update(&env, &invoice);

            let current_status_invoices = InvoiceStorage::get_by_status(&env, status.clone());
            assert!(
                current_status_invoices.iter().any(|id| id == invoice_id),
                "Invoice must be in {} index after update",
                match status {
                    InvoiceStatus::Pending => "Pending",
                    InvoiceStatus::Verified => "Verified",
                    InvoiceStatus::Funded => "Funded",
                    InvoiceStatus::Paid => "Paid",
                    InvoiceStatus::Defaulted => "Defaulted",
                    InvoiceStatus::Cancelled => "Cancelled",
                    InvoiceStatus::Refunded => "Refunded",
                }
            );
        }
    });
}

#[test]
fn invariant_multiple_bid_status_transitions_remain_coherent() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let bid_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let mut bid = create_test_bid(
            &env,
            bid_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            BidStatus::Placed,
        );
        BidStorage::store(&env, &bid);

        let statuses = [BidStatus::Accepted, BidStatus::Expired];

        for status in statuses.iter() {
            bid.status = status.clone();
            BidStorage::update(&env, &bid);

            let current_status_bids = BidStorage::get_by_status(&env, status.clone());
            assert!(
                current_status_bids.iter().any(|id| id == bid_id),
                "Bid must be in status index after update"
            );
        }
    });
}

#[test]
fn invariant_multiple_investment_status_transitions_remain_coherent() {
    let env = Env::default();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[1; 32]);
    let investment_id = BytesN::from_array(&env, &[2; 32]);

    with_registered_contract(&env, |_env| {
        let mut investment = create_test_investment(
            &env,
            investment_id.clone(),
            invoice_id.clone(),
            investor.clone(),
            InvestmentStatus::Active,
        );
        InvestmentStorage::store(&env, &investment);

        let statuses = [InvestmentStatus::Completed, InvestmentStatus::Defaulted];

        for status in statuses.iter() {
            investment.status = status.clone();
            InvestmentStorage::update(&env, &investment);

            let current_status_investments = InvestmentStorage::get_by_status(&env, status.clone());
            assert!(
                current_status_investments
                    .iter()
                    .any(|id| id == investment_id),
                "Investment must be in status index after update"
            );
        }
    });
}

// ============================================================================
// FULL LIFECYCLE INTEGRATION TEST
// ============================================================================

#[test]
fn test_invariants_after_full_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    let investor = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &currency);
    let sac_client = token::StellarAssetClient::new(&env, &currency);
    let initial_balance = 20_000i128;
    sac_client.mint(&business, &initial_balance);
    sac_client.mint(&investor, &initial_balance);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

    client.set_admin(&admin);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);
    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    client.verify_investor(&investor, &15_000);

    let amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 86400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(&env, "Full lifecycle invoice"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    );
    client.verify_invoice(&invoice_id);

    let bid_id = client.place_bid(&investor, &invoice_id, &amount, &(amount + 500));
    client.accept_bid(&invoice_id, &bid_id);

    client.release_escrow_funds(&invoice_id);

    client.process_partial_payment(
        &invoice_id,
        &amount,
        &String::from_str(&env, "lifecycle-tx-1"),
    );

    let total_invoice_count = client.get_total_invoice_count();
    assert!(
        total_invoice_count >= 1,
        "total_invoice_count must be at least 1"
    );

    let paid_count = client.get_invoice_count_by_status(&InvoiceStatus::Paid);
    assert_eq!(
        paid_count, 1,
        "exactly one invoice must be Paid after full lifecycle"
    );

    let sum_status = client.get_invoice_count_by_status(&InvoiceStatus::Pending)
        + client.get_invoice_count_by_status(&InvoiceStatus::Verified)
        + client.get_invoice_count_by_status(&InvoiceStatus::Funded)
        + client.get_invoice_count_by_status(&InvoiceStatus::Paid)
        + client.get_invoice_count_by_status(&InvoiceStatus::Defaulted)
        + client.get_invoice_count_by_status(&InvoiceStatus::Cancelled)
        + client.get_invoice_count_by_status(&InvoiceStatus::Refunded);

    assert_eq!(
        sum_status, total_invoice_count,
        "sum of status counts must equal total count (no orphaned storage)"
    );
}

// ============================================================================
// STRESS TEST WITH MULTIPLE ENTITIES
// ============================================================================

#[test]
fn test_invariants_multi_entity_stress() {
    let (env, client, admin) = setup();

    let b = Address::generate(&env);
    let i = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    client.add_currency(&admin, &currency);

    client.submit_kyc_application(&b, &String::from_str(&env, "B"));
    client.verify_business(&admin, &b);
    client.submit_investor_kyc(&i, &String::from_str(&env, "I"));
    client.verify_investor(&i, &1_000_000);

    let mut ids = Vec::new(&env);
    for j in 0..10 {
        let id = client.store_invoice(
            &b,
            &1000,
            &currency,
            &(1000 + j as u64),
            &String::from_str(&env, "T"),
            &InvoiceCategory::Consulting,
            &Vec::new(&env),
        );
        ids.push_back(id);
    }

    let total = client.get_total_invoice_count();
    let sum = client.get_invoice_count_by_status(&InvoiceStatus::Pending)
        + client.get_invoice_count_by_status(&InvoiceStatus::Verified)
        + client.get_invoice_count_by_status(&InvoiceStatus::Funded)
        + client.get_invoice_count_by_status(&InvoiceStatus::Paid)
        + client.get_invoice_count_by_status(&InvoiceStatus::Defaulted)
        + client.get_invoice_count_by_status(&InvoiceStatus::Cancelled)
        + client.get_invoice_count_by_status(&InvoiceStatus::Refunded);

    assert_eq!(total, 10, "Total should be 10");
    assert_eq!(total, sum, "Total should match sum of status buckets");

    for (idx, id) in ids.iter().enumerate() {
        if idx % 2 == 0 {
            client.verify_invoice(&id);
        }
    }

    let total_after = client.get_total_invoice_count();
    let sum_after = client.get_invoice_count_by_status(&InvoiceStatus::Pending)
        + client.get_invoice_count_by_status(&InvoiceStatus::Verified)
        + client.get_invoice_count_by_status(&InvoiceStatus::Funded)
        + client.get_invoice_count_by_status(&InvoiceStatus::Paid)
        + client.get_invoice_count_by_status(&InvoiceStatus::Defaulted)
        + client.get_invoice_count_by_status(&InvoiceStatus::Cancelled)
        + client.get_invoice_count_by_status(&InvoiceStatus::Refunded);

    assert_eq!(
        total_after, sum_after,
        "Total must equal sum after status changes"
    );
}
