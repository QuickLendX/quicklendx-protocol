use super::*;
use crate::errors::QuickLendXError;
use crate::storage::InvoiceStorage;
use crate::types::{DisputeStatus, InvoiceStatus};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Bytes, BytesN, Env, String, Vec};

struct TestContext {
    env: Env,
    client: QuickLendXContractClient<'static>,
    admin: Address,
    business: Address,
    investor: Address,
    currency: Address,
    contract_id: Address,
}

impl TestContext {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        client.initialize_admin(&admin);

        let token_admin = Address::generate(&env);
        let currency = env.register_stellar_asset_contract_v2(token_admin).address();
        let sac = token::StellarAssetClient::new(&env, &currency);
        sac.mint(&business, &100_000i128);
        sac.mint(&investor, &100_000i128);
        let tok = token::Client::new(&env, &currency);
        let exp = env.ledger().sequence() + 50_000;
        tok.approve(&business, &contract_id, &400_000i128, &exp);
        tok.approve(&investor, &contract_id, &400_000i128, &exp);

        client.add_currency(&admin, &currency);

        Self { env, client, admin, business, investor, currency, contract_id }
    }

    fn setup_kyc(&self) {
        self.client.submit_kyc_application(&self.business, &Bytes::from_slice(&self.env, b"KYC"));
        self.client.verify_business(&self.admin, &self.business);
        self.client.submit_investor_kyc(&self.investor, &Bytes::from_slice(&self.env, b"KYC"));
        self.client.verify_investor(&self.admin, &self.investor, &200_000i128);
    }

    fn create_funded_invoice(&self) -> BytesN<32> {
        let due_date = self.env.ledger().timestamp() + 86_400;
        let invoice_id = self.client.upload_invoice(
            &self.business,
            &1_000i128,
            &self.currency,
            &due_date,
            &String::from_str(&self.env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&self.env),
        &None);
        self.client.verify_invoice(&invoice_id);
        let bid_id = self.client.place_bid(
            &self.investor,
            &invoice_id,
            &1_000i128,
            &1_000i128,
            &BytesN::from_array(&self.env, &[0u8; 32]),
        );
        self.client.accept_bid(&invoice_id, &bid_id);
        invoice_id
    }

    fn dispute_status(&self, invoice_id: &BytesN<32>) -> DisputeStatus {
        self.client.get_invoice(invoice_id).dispute_status
    }

    fn create_dispute(&self, invoice_id: &BytesN<32>) {
        self.client.create_dispute(
            invoice_id,
            &self.business,
            &String::from_str(&self.env, "Reason for dispute"),
            &String::from_str(&self.env, "Evidence description"),
        );
    }

    fn put_under_review(&self, invoice_id: &BytesN<32>) {
        self.client.put_dispute_under_review(invoice_id);
    }

    fn resolve_dispute(&self, invoice_id: &BytesN<32>) {
        self.client.resolve_dispute(
            invoice_id,
            &String::from_str(&self.env, "Resolution notes"),
        );
    }
}

#[test]
fn test_transition_none_to_disputed() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    assert_eq!(ctx.dispute_status(&invoice_id), DisputeStatus::None);
    ctx.create_dispute(&invoice_id);
    assert_eq!(ctx.dispute_status(&invoice_id), DisputeStatus::Disputed);
}

#[test]
fn test_transition_disputed_to_under_review() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    ctx.create_dispute(&invoice_id);
    assert_eq!(ctx.dispute_status(&invoice_id), DisputeStatus::Disputed);
    ctx.put_under_review(&invoice_id);
    assert_eq!(ctx.dispute_status(&invoice_id), DisputeStatus::UnderReview);
}

#[test]
fn test_transition_under_review_to_resolved() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    ctx.create_dispute(&invoice_id);
    ctx.put_under_review(&invoice_id);
    assert_eq!(ctx.dispute_status(&invoice_id), DisputeStatus::UnderReview);
    ctx.resolve_dispute(&invoice_id);
    assert_eq!(ctx.dispute_status(&invoice_id), DisputeStatus::Resolved);
}

#[test]
fn test_cannot_create_duplicate_dispute() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    ctx.create_dispute(&invoice_id);
    let result = ctx.client.try_create_dispute(
        &invoice_id,
        &ctx.business,
        &String::from_str(&ctx.env, "Second dispute"),
        &String::from_str(&ctx.env, "More evidence"),
    );
    assert!(result.is_err());
}

#[test]
fn test_cannot_put_non_disputed_under_review() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    let result = ctx.client.try_put_dispute_under_review(&invoice_id);
    assert!(result.is_err());
}

#[test]
fn test_cannot_resolve_without_under_review() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    ctx.create_dispute(&invoice_id);
    let result = ctx.client.try_resolve_dispute(
        &invoice_id,
        &String::from_str(&ctx.env, "Resolution"),
    );
    assert!(result.is_err());
}

#[test]
fn test_cannot_resolve_twice() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    ctx.create_dispute(&invoice_id);
    ctx.put_under_review(&invoice_id);
    ctx.resolve_dispute(&invoice_id);
    let result = ctx.client.try_resolve_dispute(
        &invoice_id,
        &String::from_str(&ctx.env, "Second resolution"),
    );
    assert!(result.is_err());
}

#[test]
fn test_cannot_put_resolved_under_review() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    ctx.create_dispute(&invoice_id);
    ctx.put_under_review(&invoice_id);
    ctx.resolve_dispute(&invoice_id);
    let result = ctx.client.try_put_dispute_under_review(&invoice_id);
    assert!(result.is_err());
}

#[test]
fn test_transitions_guard_consistency() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();

    let transitions: Vec<(DisputeStatus, DisputeStatus, bool)> = vec![
        (DisputeStatus::None, DisputeStatus::Disputed, true),
        (DisputeStatus::None, DisputeStatus::UnderReview, false),
        (DisputeStatus::None, DisputeStatus::Resolved, false),
        (DisputeStatus::Disputed, DisputeStatus::UnderReview, true),
        (DisputeStatus::Disputed, DisputeStatus::Resolved, false),
        (DisputeStatus::Disputed, DisputeStatus::Disputed, false),
        (DisputeStatus::UnderReview, DisputeStatus::Resolved, true),
        (DisputeStatus::UnderReview, DisputeStatus::Disputed, false),
        (DisputeStatus::UnderReview, DisputeStatus::UnderReview, false),
        (DisputeStatus::Resolved, DisputeStatus::Disputed, false),
        (DisputeStatus::Resolved, DisputeStatus::UnderReview, false),
        (DisputeStatus::Resolved, DisputeStatus::Resolved, false),
    ];

    for (from, to, should_succeed) in transitions {
        let inv_id = ctx.client.get_invoices_by_status(InvoiceStatus::Funded).get(0);
        let inv_id = if let Some(id) = inv_id { id } else { ctx.create_funded_invoice() };

        match from {
            DisputeStatus::Disputed => ctx.create_dispute(&inv_id),
            DisputeStatus::UnderReview => { ctx.create_dispute(&inv_id); ctx.put_under_review(&inv_id); }
            DisputeStatus::Resolved => { ctx.create_dispute(&inv_id); ctx.put_under_review(&inv_id); ctx.resolve_dispute(&inv_id); }
            DisputeStatus::None => {}
        }

        let actual = ctx.dispute_status(&inv_id);
        if actual != from {
            continue;
        }

        let result = match to {
            DisputeStatus::Disputed => ctx.client.try_create_dispute(
                &inv_id,
                &ctx.business,
                &String::from_str(&ctx.env, "Reason"),
                &String::from_str(&ctx.env, "Evidence"),
            ),
            DisputeStatus::UnderReview => ctx.client.try_put_dispute_under_review(&inv_id),
            DisputeStatus::Resolved => ctx.client.try_resolve_dispute(
                &inv_id,
                &String::from_str(&ctx.env, "Resolution"),
            ),
        };

        assert_eq!(
            result.is_ok(), should_succeed,
            "{:?} -> {:?} should {} but got {:?}",
            from, to, if should_succeed { "succeed" } else { "fail" }, result
        );
    }
}
