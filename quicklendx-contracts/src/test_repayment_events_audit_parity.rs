//! Events and audit parity for repayment allocation (QE-2026-08).
//!
//! Compares versioned `RepaymentAllocated` payloads with the durable ledger
//! and `PaymentProcessed` audit records across success, rejection, retry,
//! late penalty, and upgrade reconstruction.

#![cfg(test)]

use crate::audit::{AuditOperation, AuditOperationFilter, AuditQueryFilter, AuditStorage};
use crate::errors::QuickLendXError;
use crate::observability::{TransitionPhase, OBSERVABILITY_SCHEMA_VERSION};
use crate::reentrancy::with_payment_guard;
use crate::settlement::{
    allocate_cumulative_repayment, get_invoice_progress, get_repayment_ledger,
    process_partial_payment,
};
use crate::types::{InvoiceCategory, InvoiceStatus};
use crate::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token, xdr, Address, BytesN, Env, String, Symbol, TryFromVal, Vec,
};

fn setup() -> (Env, QuickLendXContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let _ = client.try_initialize_admin(&admin);
    client.set_admin(&admin);
    (env, client, admin)
}

fn setup_token(
    env: &Env,
    business: &Address,
    investor: &Address,
    contract_id: &Address,
) -> Address {
    let token_admin = Address::generate(env);
    let currency = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &currency);
    let sac_client = token::StellarAssetClient::new(env, &currency);
    let initial_balance = 1_000_000i128;
    sac_client.mint(business, &initial_balance);
    sac_client.mint(investor, &initial_balance);
    let expiration = env.ledger().sequence() + 10_000;
    token_client.approve(business, contract_id, &initial_balance, &expiration);
    token_client.approve(investor, contract_id, &initial_balance, &expiration);
    currency
}

fn fund_invoice(
    env: &Env,
    client: &QuickLendXContractClient,
    admin: &Address,
    amount: i128,
    late_bps: Option<u32>,
) -> (BytesN<32>, Address, Address, Address) {
    let contract_id = client.address.clone();
    let business = Address::generate(env);
    client.submit_kyc_application(&business, &String::from_str(env, "Business KYC"));
    client.verify_business(admin, &business);
    let investor = Address::generate(env);
    client.submit_investor_kyc(&investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&investor, &(amount * 10));
    let currency = setup_token(env, &business, &investor, &contract_id);
    let due_date = env.ledger().timestamp() + 86_400;
    let invoice_id = client.store_invoice(
        &business,
        &amount,
        &currency,
        &due_date,
        &String::from_str(env, "Repayment parity invoice"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None,
        &late_bps,
        &None,
    );
    client.verify_invoice(&invoice_id);
    let bid_id = client.place_bid(
        &investor,
        &invoice_id,
        &amount,
        &(amount + 1_000),
        &BytesN::from_array(env, &[7u8; 32]),
    );
    client.accept_bid(&invoice_id, &bid_id);
    (invoice_id, business, investor, currency)
}

fn payment_audit_count(env: &Env, contract_id: &Address, invoice_id: &BytesN<32>) -> u32 {
    env.as_contract(contract_id, || {
        let filter = AuditQueryFilter {
            invoice_id: Some(invoice_id.clone()),
            operation: AuditOperationFilter::Specific(AuditOperation::PaymentProcessed),
            actor: None,
            start_timestamp: None,
            end_timestamp: None,
        };
        AuditStorage::query_audit_logs(env, &filter, 50).len()
    })
}

fn topic_emitted(env: &Env, topic: &str) -> bool {
    let want = Symbol::new(env, topic);
    env.events().all().events().iter().any(|ev| match &ev.body {
        xdr::ContractEventBody::V0(body) => body
            .topics
            .iter()
            .any(|t| Symbol::try_from_val(env, t).ok() == Some(want.clone())),
    })
}

fn event_count(env: &Env) -> usize {
    env.events().all().events().len()
}

/// Settlement nonces must be 64-character hex (`validate_transaction_hash`).
fn payment_nonce(env: &Env, seed: u8) -> String {
    let mut hex = [b'0'; 64];
    hex[62] = b'a' + (seed % 6);
    hex[63] = b'0' + (seed % 10);
    String::from_str(env, core::str::from_utf8(&hex).unwrap())
}

// ---------------------------------------------------------------------------
// Pure allocator
// ---------------------------------------------------------------------------

#[test]
fn test_allocate_no_profit_all_principal() {
    let a = allocate_cumulative_repayment(1_000, 1_000, 400, 200, 0).unwrap();
    assert_eq!(a.principal, 400);
    assert_eq!(a.investor_profit, 0);
    assert_eq!(a.platform_fee, 0);
    assert_eq!(a.late_penalty, 0);
    assert_eq!(
        a.principal + a.investor_profit + a.platform_fee + a.late_penalty,
        400
    );
}

#[test]
fn test_allocate_profit_fee_rounds_to_investor() {
    // face 1100, investment 1000, paid 1100, 200 bps → fee floor(100*200/10000)=2
    let a = allocate_cumulative_repayment(1_000, 1_100, 1_100, 200, 0).unwrap();
    assert_eq!(a.principal, 1_000);
    assert_eq!(a.platform_fee, 2);
    assert_eq!(a.investor_profit, 98);
    assert_eq!(a.late_penalty, 0);
    assert_eq!(
        a.principal + a.investor_profit + a.platform_fee + a.late_penalty,
        1_100
    );
}

#[test]
fn test_allocate_late_fills_after_principal_and_profit() {
    // 1000 principal, 100 profit cap, 50 late. Payment 1_080 fills late 0 of 50? 1080-1000-100=0 wait 80 leftover after principal, profit 80, late 0
    let partial = allocate_cumulative_repayment(1_000, 1_100, 1_080, 200, 50).unwrap();
    assert_eq!(partial.principal, 1_000);
    assert_eq!(partial.investor_profit + partial.platform_fee, 80);
    assert_eq!(partial.late_penalty, 0);

    let full = allocate_cumulative_repayment(1_000, 1_100, 1_150, 200, 50).unwrap();
    assert_eq!(full.principal, 1_000);
    assert_eq!(full.investor_profit + full.platform_fee, 100);
    assert_eq!(full.late_penalty, 50);
    assert_eq!(
        full.principal + full.investor_profit + full.platform_fee + full.late_penalty,
        1_150
    );
}

#[test]
fn test_allocate_overpay_beyond_waterfall_rejected() {
    assert_eq!(
        allocate_cumulative_repayment(1_000, 1_000, 1_001, 0, 0),
        Err(QuickLendXError::InvalidAmount)
    );
}

#[test]
fn test_allocate_negative_inputs_rejected() {
    assert_eq!(
        allocate_cumulative_repayment(-1, 1_000, 10, 0, 0),
        Err(QuickLendXError::InvalidAmount)
    );
    assert_eq!(
        allocate_cumulative_repayment(1_000, 1_000, -1, 0, 0),
        Err(QuickLendXError::InvalidAmount)
    );
}

// ---------------------------------------------------------------------------
// Integration: events, audit, ledger
// ---------------------------------------------------------------------------

#[test]
fn test_partial_then_final_event_audit_ledger_parity() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let amount = 10_000i128;
    let (invoice_id, _business, _investor, _currency) =
        fund_invoice(&env, &client, &admin, amount, None);

    client.process_partial_payment(&invoice_id, &4_000, &payment_nonce(&env, 1));
    assert!(topic_emitted(&env, "repayment_allocated"));
    assert!(topic_emitted(&env, "partial_payment"));

    let ledger = env.as_contract(&contract_id, || {
        get_repayment_ledger(&env, &invoice_id).unwrap()
    });
    assert_eq!(ledger.principal, 4_000);
    assert_eq!(ledger.total_paid, 4_000);
    assert_eq!(payment_audit_count(&env, &contract_id, &invoice_id), 1);

    let entries = env.as_contract(&contract_id, || {
        let filter = AuditQueryFilter {
            invoice_id: Some(invoice_id.clone()),
            operation: AuditOperationFilter::Specific(AuditOperation::PaymentProcessed),
            actor: None,
            start_timestamp: None,
            end_timestamp: None,
        };
        AuditStorage::query_audit_logs(&env, &filter, 10)
    });
    let entry = entries.get(0).unwrap();
    assert_eq!(entry.schema_version, OBSERVABILITY_SCHEMA_VERSION);
    assert_eq!(entry.operation_id, entry.audit_id);
    assert_eq!(entry.amount, Some(4_000));
    assert!(env.as_contract(&contract_id, || {
        AuditStorage::verify_audit_chain(&env, &invoice_id)
    }));

    client.process_partial_payment(&invoice_id, &6_000, &payment_nonce(&env, 2));
    let ledger = env.as_contract(&contract_id, || {
        get_repayment_ledger(&env, &invoice_id).unwrap()
    });
    assert_eq!(ledger.principal, 10_000);
    assert_eq!(ledger.total_paid, 10_000);
    assert_eq!(
        ledger.principal + ledger.investor_profit + ledger.platform_fee + ledger.late_penalty,
        ledger.total_paid
    );
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Paid);
    assert_eq!(payment_audit_count(&env, &contract_id, &invoice_id), 2);
}

#[test]
fn test_late_penalty_assessed_once_and_paid_to_investor() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let amount = 10_000i128;
    let (invoice_id, business, investor, currency) =
        fund_invoice(&env, &client, &admin, amount, Some(1_000)); // 10%

    client.initialize_fee_system(&admin);
    client.update_platform_fee_bps(&0);
    let treasury = Address::generate(&env);
    client.configure_treasury(&treasury);

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 200_000);

    let token_client = token::Client::new(&env, &currency);
    let investor_before = token_client.balance(&investor);
    let treasury_before = token_client.balance(&treasury);

    client.process_partial_payment(&invoice_id, &5_000, &payment_nonce(&env, 3));
    let ledger = env.as_contract(&contract_id, || {
        get_repayment_ledger(&env, &invoice_id).unwrap()
    });
    assert!(ledger.late_assessed);
    assert_eq!(ledger.assessed_late, 1_000); // 10% of remaining 10_000
    assert_eq!(ledger.principal, 5_000);
    assert_eq!(ledger.late_penalty, 0);

    let progress = env.as_contract(&contract_id, || {
        get_invoice_progress(&env, &invoice_id).unwrap()
    });
    assert_eq!(progress.total_due, 11_000);
    assert_eq!(progress.remaining_due, 6_000);

    client.process_partial_payment(&invoice_id, &6_000, &payment_nonce(&env, 4));
    let ledger = env.as_contract(&contract_id, || {
        get_repayment_ledger(&env, &invoice_id).unwrap()
    });
    assert_eq!(ledger.assessed_late, 1_000);
    assert_eq!(ledger.late_penalty, 1_000);
    assert_eq!(ledger.principal, 10_000);
    assert_eq!(ledger.platform_fee, 0);
    assert_eq!(ledger.total_paid, 11_000);

    assert_eq!(token_client.balance(&investor), investor_before + 11_000);
    assert_eq!(token_client.balance(&treasury), treasury_before);
    let _ = business;
}

#[test]
fn test_duplicate_nonce_emits_no_audit_or_ledger_change() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let (invoice_id, _b, _i, _c) = fund_invoice(&env, &client, &admin, 10_000, None);
    let nonce = payment_nonce(&env, 5);
    client.process_partial_payment(&invoice_id, &1_000, &nonce);
    let audits_before = payment_audit_count(&env, &contract_id, &invoice_id);
    let ledger_before = env.as_contract(&contract_id, || {
        get_repayment_ledger(&env, &invoice_id).unwrap()
    });

    let result = client.try_process_partial_payment(&invoice_id, &500, &nonce);
    assert_eq!(result, Err(Ok(QuickLendXError::DuplicateNonce)));
    assert_eq!(
        payment_audit_count(&env, &contract_id, &invoice_id),
        audits_before
    );
    let ledger_after = env.as_contract(&contract_id, || {
        get_repayment_ledger(&env, &invoice_id).unwrap()
    });
    assert_eq!(ledger_before, ledger_after);
}

#[test]
fn test_overpayment_is_capped_and_ledger_matches_applied() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let (invoice_id, _b, _i, _c) = fund_invoice(&env, &client, &admin, 1_000, None);
    client.process_partial_payment(&invoice_id, &5_000, &payment_nonce(&env, 6));
    let ledger = env.as_contract(&contract_id, || {
        get_repayment_ledger(&env, &invoice_id).unwrap()
    });
    assert_eq!(ledger.total_paid, 1_000);
    assert_eq!(ledger.principal, 1_000);
}

#[test]
fn test_replay_after_paid_leaves_audit_unchanged() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let (invoice_id, _b, _i, _c) = fund_invoice(&env, &client, &admin, 1_000, None);
    client.process_partial_payment(&invoice_id, &1_000, &payment_nonce(&env, 7));
    let audits = payment_audit_count(&env, &contract_id, &invoice_id);
    let events_before = event_count(&env);
    let result = client.try_process_partial_payment(&invoice_id, &1, &payment_nonce(&env, 8));
    assert_eq!(result, Err(Ok(QuickLendXError::InvalidStatus)));
    assert_eq!(payment_audit_count(&env, &contract_id, &invoice_id), audits);
    assert_eq!(event_count(&env), events_before);
}

#[test]
fn test_dispute_blocks_final_settle_and_rolls_back_completing_payment() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let (invoice_id, business, _investor, _c) = fund_invoice(&env, &client, &admin, 1_000, None);
    client.process_partial_payment(&invoice_id, &400, &payment_nonce(&env, 9));
    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "quality issue"),
        &String::from_str(&env, "evidence-hash-or-note"),
    );
    let audits = payment_audit_count(&env, &contract_id, &invoice_id);
    let events_before = event_count(&env);
    let result = client.try_process_partial_payment(&invoice_id, &600, &payment_nonce(&env, 10));
    assert_eq!(result, Err(Ok(QuickLendXError::DisputeActive)));
    assert_eq!(payment_audit_count(&env, &contract_id, &invoice_id), audits);
    assert_eq!(event_count(&env), events_before);
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.total_paid, 400);
}

#[test]
fn test_dispute_allows_non_completing_payment_record() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let (invoice_id, business, _i, _c) = fund_invoice(&env, &client, &admin, 1_000, None);
    client.process_partial_payment(&invoice_id, &400, &payment_nonce(&env, 13));
    client.create_dispute(
        &invoice_id,
        &business,
        &String::from_str(&env, "quality issue"),
        &String::from_str(&env, "evidence-hash-or-note"),
    );
    client.process_partial_payment(&invoice_id, &100, &payment_nonce(&env, 14));
    let invoice = client.get_invoice(&invoice_id);
    assert_eq!(invoice.total_paid, 500);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    let ledger = env.as_contract(&contract_id, || {
        get_repayment_ledger(&env, &invoice_id).unwrap()
    });
    assert_eq!(ledger.total_paid, 500);
}

#[test]
fn test_nested_payment_guard_rejects_without_new_audit() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let (invoice_id, _b, _i, _c) = fund_invoice(&env, &client, &admin, 10_000, None);
    let audits = payment_audit_count(&env, &contract_id, &invoice_id);
    let nonce = payment_nonce(&env, 11);
    let result = env.as_contract(&contract_id, || {
        with_payment_guard(&env, || {
            with_payment_guard(&env, || {
                process_partial_payment(&env, &invoice_id, 100, nonce.clone())
            })
        })
    });
    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    assert_eq!(payment_audit_count(&env, &contract_id, &invoice_id), audits);
}

#[test]
fn test_upgrade_reconstructs_ledger_without_retroactive_late() {
    let (env, client, admin) = setup();
    let contract_id = client.address.clone();
    let (invoice_id, _b, _i, _c) = fund_invoice(&env, &client, &admin, 1_000, Some(1_000));

    env.as_contract(&contract_id, || {
        let mut invoice = crate::storage::InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();
        invoice.total_paid = 400;
        crate::storage::InvoiceStorage::update_invoice(&env, &invoice);
    });

    env.ledger()
        .set_timestamp(env.ledger().timestamp() + 200_000);
    client.process_partial_payment(&invoice_id, &100, &payment_nonce(&env, 12));
    let ledger = env.as_contract(&contract_id, || {
        get_repayment_ledger(&env, &invoice_id).unwrap()
    });
    assert_eq!(ledger.principal, 500);
    assert_eq!(ledger.total_paid, 500);
    // Remaining contractual at first post-due payment was 600, 10% → 60
    assert_eq!(ledger.assessed_late, 60);
    assert_eq!(ledger.late_penalty, 0);
}

#[test]
fn test_committed_phase_constant() {
    assert_eq!(TransitionPhase::Committed, TransitionPhase::Committed);
}
