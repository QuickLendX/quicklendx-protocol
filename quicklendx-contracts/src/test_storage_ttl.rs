#![cfg(test)]

use crate::bid::{Bid, BidStatus, BidStorage};
use crate::dispute::{add_to_dispute_index, get_dispute_index, reserve_evidence};
use crate::dispute_timeline::{get_under_review_timestamp, set_under_review_timestamp};
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
use crate::payments::{Escrow, EscrowStatus, EscrowStorage};
use crate::storage::{
    DataKey, Indexes, InvoiceStorage, StorageKeys, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_EXTEND_TO,
};
use crate::types::{Dispute, DisputeStatus, Invoice, InvoiceCategory, InvoiceLock, InvoiceStatus};
use crate::vesting::{VestingSchedule, VestingStorage};
use crate::QuickLendXContract;
use soroban_sdk::testutils::{storage::Persistent as _, Address as _, Ledger as _};
use soroban_sdk::{symbol_short, Address, BytesN, Env, String, Vec};

fn setup_env() -> (Env, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    env.ledger().with_mut(|li| {
        li.sequence_number = 100;
        li.min_persistent_entry_ttl = 100;
        li.max_entry_ttl = 1_000_000;
    });
    (env, contract_id)
}

fn make_invoice(env: &Env, id_byte: u8, business: &Address, currency: &Address) -> Invoice {
    Invoice {
        id: BytesN::from_array(env, &[id_byte; 32]),
        business: business.clone(),
        amount: 10_000,
        currency: currency.clone(),
        due_date: 100_000,
        status: InvoiceStatus::Verified,
        created_at: 1_000,
        description: String::from_str(env, "TTL test invoice"),
        metadata_customer_name: Some(String::from_str(env, "Acme Corp")),
        metadata_customer_address: None,
        metadata_tax_id: Some(String::from_str(env, "TAX-001")),
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
        dispute_status: DisputeStatus::None,
        dispute: Dispute {
            created_by: business.clone(),
            created_at: 0,
            reason: String::from_str(env, ""),
            evidence: String::from_str(env, ""),
            resolution: String::from_str(env, ""),
            resolved_by: business.clone(),
            resolved_at: 0,
        },
        total_paid: 0,
        payment_history: Vec::new(env),
        grace_period_ends_at: 0,
        defaulted_at: None,
    }
}

/// Advances the ledger sequence number past the bump trigger point, touches the
/// record, verifies the TTL is bumped >= PERSISTENT_BUMP_THRESHOLD, then advances
/// past the original expiry window and verifies the record is still readable.
fn advance_past_threshold(env: &Env) {
    let delta = PERSISTENT_EXTEND_TO - PERSISTENT_BUMP_THRESHOLD + 100;
    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number.saturating_add(delta);
    });
}

fn advance_near_expiry(env: &Env) {
    let delta = PERSISTENT_EXTEND_TO - 100;
    env.ledger().with_mut(|li| {
        li.sequence_number = li.sequence_number.saturating_add(delta);
    });
}

#[test]
fn test_storage_ttl_invoice_family() {
    let (env, contract_id) = setup_env();
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let invoice = make_invoice(&env, 1, &business, &currency);
    let invoice_id = invoice.id.clone();

    env.as_contract(&contract_id, || {
        InvoiceStorage::store(&env, &invoice);
        let _ = InvoiceStorage::next_count(&env);
        InvoiceStorage::set_per_investor_position_cap(&env, &invoice_id, Some(5_000));
        InvoiceStorage::set_invoice_lock(&env, &invoice_id, InvoiceLock::AdminFrozen);

        let inv_key = DataKey::Invoice(invoice_id.clone());
        let cap_key = DataKey::PerInvestorPositionCap(invoice_id.clone());
        let lock_key = DataKey::FrozenInvoice(invoice_id.clone());
        let count_key = StorageKeys::invoice_count();
        let bus_idx_key = Indexes::invoices_by_business(&business);

        assert!(env.storage().persistent().get_ttl(&inv_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&cap_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&lock_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&count_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&bus_idx_key) >= PERSISTENT_BUMP_THRESHOLD);
    });

    advance_past_threshold(&env);

    env.as_contract(&contract_id, || {
        assert!(InvoiceStorage::get(&env, &invoice_id).is_some());
        assert_eq!(
            InvoiceStorage::get_per_investor_position_cap(&env, &invoice_id),
            Some(5_000)
        );
        assert_eq!(
            InvoiceStorage::get_invoice_lock(&env, &invoice_id),
            InvoiceLock::AdminFrozen
        );
        assert_eq!(InvoiceStorage::get_total_count(&env), 1);
        assert_eq!(InvoiceStorage::get_by_business(&env, &business).len(), 1);

        let inv_key = DataKey::Invoice(invoice_id.clone());
        let cap_key = DataKey::PerInvestorPositionCap(invoice_id.clone());
        let lock_key = DataKey::FrozenInvoice(invoice_id.clone());
        let count_key = StorageKeys::invoice_count();
        let bus_idx_key = Indexes::invoices_by_business(&business);

        assert!(env.storage().persistent().get_ttl(&inv_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&cap_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&lock_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&count_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&bus_idx_key) >= PERSISTENT_BUMP_THRESHOLD);
    });

    advance_near_expiry(&env);

    env.as_contract(&contract_id, || {
        assert!(InvoiceStorage::get(&env, &invoice_id).is_some());
        assert_eq!(
            InvoiceStorage::get_per_investor_position_cap(&env, &invoice_id),
            Some(5_000)
        );
        assert_eq!(
            InvoiceStorage::get_invoice_lock(&env, &invoice_id),
            InvoiceLock::AdminFrozen
        );
        assert_eq!(InvoiceStorage::get_total_count(&env), 1);
        assert_eq!(InvoiceStorage::get_by_business(&env, &business).len(), 1);
    });
}

#[test]
fn test_storage_ttl_bid_family() {
    let (env, contract_id) = setup_env();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[2u8; 32]);
    let bid_id = BytesN::from_array(&env, &[3u8; 32]);

    let bid = Bid {
        bid_id: bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount: 9_500,
        expected_return: 10_000,
        timestamp: 1_000,
        status: BidStatus::Placed,
        expiration_timestamp: 100_000,
    };

    env.as_contract(&contract_id, || {
        BidStorage::store_bid(&env, &bid);
        assert!(env.storage().persistent().get_ttl(&bid_id) >= PERSISTENT_BUMP_THRESHOLD);
    });

    advance_past_threshold(&env);

    env.as_contract(&contract_id, || {
        assert!(BidStorage::get_bid(&env, &bid_id).is_some());
        assert_eq!(BidStorage::get_bids_for_invoice(&env, &invoice_id).len(), 1);
        assert!(env.storage().persistent().get_ttl(&bid_id) >= PERSISTENT_BUMP_THRESHOLD);
    });

    advance_near_expiry(&env);

    env.as_contract(&contract_id, || {
        assert!(BidStorage::get_bid(&env, &bid_id).is_some());
        assert_eq!(BidStorage::get_bids_for_invoice(&env, &invoice_id).len(), 1);
    });
}

#[test]
fn test_storage_ttl_investment_family() {
    let (env, contract_id) = setup_env();
    let investor = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[4u8; 32]);
    let investment_id = BytesN::from_array(&env, &[5u8; 32]);

    let investment = Investment {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        amount: 9_500,
        funded_at: 1_000,
        status: InvestmentStatus::Active,
        insurance: Vec::new(&env),
    };

    env.as_contract(&contract_id, || {
        InvestmentStorage::store_investment(&env, &investment);
        assert!(env.storage().persistent().get_ttl(&investment_id) >= PERSISTENT_BUMP_THRESHOLD);
    });

    advance_past_threshold(&env);

    env.as_contract(&contract_id, || {
        assert!(InvestmentStorage::get_investment(&env, &investment_id).is_some());
        assert_eq!(
            InvestmentStorage::get_investments_by_investor(&env, &investor).len(),
            1
        );
        assert!(env.storage().persistent().get_ttl(&investment_id) >= PERSISTENT_BUMP_THRESHOLD);
    });

    advance_near_expiry(&env);

    env.as_contract(&contract_id, || {
        assert!(InvestmentStorage::get_investment(&env, &investment_id).is_some());
        assert_eq!(
            InvestmentStorage::get_investments_by_investor(&env, &investor).len(),
            1
        );
    });
}

#[test]
fn test_storage_ttl_escrow_family() {
    let (env, contract_id) = setup_env();
    let investor = Address::generate(&env);
    let business = Address::generate(&env);
    let currency = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[6u8; 32]);
    let escrow_id = BytesN::from_array(&env, &[7u8; 32]);

    let escrow = Escrow {
        escrow_id: escrow_id.clone(),
        invoice_id: invoice_id.clone(),
        investor,
        business,
        amount: 9_500,
        currency,
        created_at: 1_000,
        status: EscrowStatus::Held,
    };

    env.as_contract(&contract_id, || {
        EscrowStorage::store_escrow(&env, &escrow);
        let invoice_key = (symbol_short!("escrow"), invoice_id.clone());
        assert!(env.storage().persistent().get_ttl(&escrow_id) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&invoice_key) >= PERSISTENT_BUMP_THRESHOLD);
    });

    advance_past_threshold(&env);

    env.as_contract(&contract_id, || {
        assert!(EscrowStorage::get_escrow(&env, &escrow_id).is_some());
        assert!(EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_some());
        let invoice_key = (symbol_short!("escrow"), invoice_id.clone());
        assert!(env.storage().persistent().get_ttl(&escrow_id) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&invoice_key) >= PERSISTENT_BUMP_THRESHOLD);
    });

    advance_near_expiry(&env);

    env.as_contract(&contract_id, || {
        assert!(EscrowStorage::get_escrow(&env, &escrow_id).is_some());
        assert!(EscrowStorage::get_escrow_by_invoice(&env, &invoice_id).is_some());
    });
}

#[test]
fn test_storage_ttl_dispute_family() {
    let (env, contract_id) = setup_env();
    let creator = Address::generate(&env);
    let invoice_id = BytesN::from_array(&env, &[8u8; 32]);
    let evidence = String::from_str(&env, "evidence-payload-ttl");

    let digest = env.as_contract(&contract_id, || {
        add_to_dispute_index(&env, &invoice_id);
        set_under_review_timestamp(&env, &invoice_id, 2_000);
        let d = reserve_evidence(&env, &invoice_id, &creator, &evidence).unwrap();

        let index_key = symbol_short!("dispute");
        let review_key = (symbol_short!("dsp_rev"), invoice_id.clone());
        let evidence_key = DataKey::DisputeEvidence(d.clone());

        assert!(env.storage().persistent().get_ttl(&index_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&review_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&evidence_key) >= PERSISTENT_BUMP_THRESHOLD);
        d
    });

    advance_past_threshold(&env);

    env.as_contract(&contract_id, || {
        assert_eq!(get_dispute_index(&env).len(), 1);
        assert_eq!(get_under_review_timestamp(&env, &invoice_id), Some(2_000));

        let index_key = symbol_short!("dispute");
        let review_key = (symbol_short!("dsp_rev"), invoice_id.clone());
        let evidence_key = DataKey::DisputeEvidence(digest.clone());

        assert!(env.storage().persistent().get_ttl(&index_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().get_ttl(&review_key) >= PERSISTENT_BUMP_THRESHOLD);
        assert!(env.storage().persistent().has(&evidence_key));
    });

    advance_near_expiry(&env);

    env.as_contract(&contract_id, || {
        assert_eq!(get_dispute_index(&env).len(), 1);
        assert_eq!(get_under_review_timestamp(&env, &invoice_id), Some(2_000));
    });
}

#[test]
fn test_storage_ttl_vesting_family() {
    let (env, contract_id) = setup_env();
    let beneficiary = Address::generate(&env);
    let token = Address::generate(&env);
    let schedule_id = 42u64;

    let schedule = VestingSchedule {
        id: schedule_id,
        beneficiary,
        token,
        total_amount: 100_000,
        released_amount: 0,
        start_time: 1_000,
        cliff_duration: 100,
        duration: 10_000,
        revoked: false,
    };

    env.as_contract(&contract_id, || {
        VestingStorage::store(&env, &schedule);
        let key = (symbol_short!("vesting"), schedule_id);
        assert!(env.storage().persistent().get_ttl(&key) >= PERSISTENT_BUMP_THRESHOLD);
    });

    advance_past_threshold(&env);

    env.as_contract(&contract_id, || {
        let fetched = VestingStorage::get(&env, schedule_id);
        assert!(fetched.is_some());
        let key = (symbol_short!("vesting"), schedule_id);
        assert!(env.storage().persistent().get_ttl(&key) >= PERSISTENT_BUMP_THRESHOLD);
    });

    advance_near_expiry(&env);

    env.as_contract(&contract_id, || {
        let fetched = VestingStorage::get(&env, schedule_id);
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().total_amount, 100_000);
    });
}
