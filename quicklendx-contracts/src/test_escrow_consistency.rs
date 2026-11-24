//! Escrow detail-status consistency tests for QuickLendX protocol.
//!
//! Ensures that `get_escrow_status` and `get_escrow_details` are consistent
//! across all escrow terminal states (Held, Released, Refunded).

use super::*;
use crate::invoice::InvoiceCategory;
use crate::payments::EscrowStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

/// Helper to create a funded invoice with escrow for testing
fn create_funded_invoice_with_escrow_for_tests(
    env: &Env,
    client: &QuickLendXContractClient<'static>,
    business: &Address,
    investor: &Address,
    invoice_amount: i128,
    escrow_amount: i128,
) -> (BytesN<32>, BytesN<32>) {
    // Setup business KYC and verification
    client.submit_kyc_application(business, &String::from_str(env, "Business KYC"));
    let admin = Address::generate(env);
    client.verify_business(&admin, business);
    
    // Setup investor KYC and verification
    client.submit_investor_kyc(investor, &String::from_str(env, "Investor KYC"));
    client.verify_investor(&admin, investor, &(escrow_amount * 2));
    
    // Create and verify invoice
    let currency = Address::generate(env);
    let due_date = env.ledger().timestamp() + 86_400 * 30;
    let invoice_id = client.store_invoice(
        business,
        &invoice_amount,
        &currency,
        &due_date,
        &String::from_str(env, "Test invoice for escrow consistency"),
        &InvoiceCategory::Services,
        &Vec::new(env),
        &None);
    client.verify_invoice(&invoice_id);
    
    // Create bid and accept to create escrow
    let bid_id = client.place_bid(investor, &invoice_id, &escrow_amount, &(invoice_amount + 100), &BytesN::from_array(&env, &[0u8; 32]));
    client.accept_bid(&invoice_id, &bid_id);
    
    (invoice_id, bid_id)
}

impl EscrowFixture {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        EscrowFixture { env, client, contract_id, admin, business, investor }
    }

    /// Set up a SAC, mint `amount` to the investor, and approve the contract.
    /// Returns the currency address.
    fn fund_investor(&self, amount: i128) -> Address {
        let token_admin = Address::generate(&self.env);
        let currency = self
            .env
            .register_stellar_asset_contract_v2(token_admin)
            .address();
        let sac = token::StellarAssetClient::new(&self.env, &currency);
        let tok = token::Client::new(&self.env, &currency);
        sac.mint(&self.investor, &amount);
        // give the contract a nominal balance so its own balance check passes
        sac.mint(&self.contract_id, &1i128);
        let exp = self.env.ledger().sequence() + 10_000;
        tok.approve(&self.investor, &self.contract_id, &amount, &exp);
        currency
    }

    /// KYC both parties, create + verify an invoice, place and accept a bid.
    /// Returns `(invoice_id, bid_id)`.
    ///
    /// KYC is performed unconditionally; callers that call this more than once
    /// for the same parties must have done the KYC setup externally via
    /// `setup_kyc`.
    fn create_funded_escrow(
        &self,
        amount: i128,
        currency: &Address,
    ) -> (BytesN<32>, BytesN<32>) {
        self.setup_kyc(amount * 10);
        self.fund_invoice(amount, currency)
    }

    /// Perform KYC for `self.business` and `self.investor` exactly once.
    /// Panics (via SDK error shim) if either party is already KYC'd.
    fn setup_kyc(&self, investor_limit: i128) {
        self.client.submit_kyc_application(
            &self.business,
            &String::from_str(&self.env, "Business KYC"),
        );
        self.client.verify_business(&self.admin, &self.business);

        self.client.submit_investor_kyc(
            &self.investor,
            &String::from_str(&self.env, "Investor KYC"),
        );
        self.client.verify_investor(&self.investor, &investor_limit);
    }

    /// Create + verify an invoice, place and accept a bid (no KYC step).
    fn fund_invoice(&self, amount: i128, currency: &Address) -> (BytesN<32>, BytesN<32>) {
        let due_date = self.env.ledger().timestamp() + 86_400 * 30;
        let invoice_id = self.client.store_invoice(
            &self.business,
            &amount,
            currency,
            &due_date,
            &String::from_str(&self.env, "Test invoice"),
            &InvoiceCategory::Services,
            &Vec::new(&self.env),
        );
        self.client.verify_invoice(&invoice_id);

        let bid_id =
            self.client
                .place_bid(&self.investor, &invoice_id, &amount, &(amount + 100));
        self.client.accept_bid_and_fund(&invoice_id, &bid_id);

        (invoice_id, bid_id)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn test_escrow_status_consistency_held_state() {
    let fx = EscrowFixture::new();
    let currency = fx.fund_investor(1_000);
    let (invoice_id, _) = fx.create_funded_escrow(1_000, &currency);

    let status = fx.client.get_escrow_status(&invoice_id);
    let details = fx.client.get_escrow_details(&invoice_id);

    assert_eq!(status, EscrowStatus::Held);
    assert_eq!(details.status, EscrowStatus::Held);
    assert_eq!(details.invoice_id, invoice_id);
    assert_eq!(details.investor, fx.investor);
    assert_eq!(details.business, fx.business);
    assert_eq!(details.amount, 1_000);
    assert!(details.created_at > 0);
}

#[test]
fn test_escrow_status_consistency_released_state() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    // Create funded invoice with escrow
    let (invoice_id, _bid_id) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        1000i128,
        1000i128,
    );
    
    // Release escrow
    client.approve_early_escrow_release(&invoice_id, &business);
    client.approve_early_escrow_release(&invoice_id, &investor);
    client.release_escrow_funds(&invoice_id);
    
    // Test: Status and details should be consistent for Released state
    let (status, details) = get_escrow_status_and_details(&env, &client, &invoice_id);
    
    assert_eq!(status, EscrowStatus::Released);
    assert_eq!(details.status, EscrowStatus::Released);
    assert_eq!(details.invoice_id, invoice_id);
    assert_eq!(details.investor, fx.investor);
    assert_eq!(details.business, fx.business);
    assert_eq!(details.amount, 1_000);
}

#[test]
fn test_escrow_status_consistency_refunded_state() {
    let fx = EscrowFixture::new();
    let currency = fx.fund_investor(1_000);
    let (invoice_id, _) = fx.create_funded_escrow(1_000, &currency);

    // Any authorised caller works under mock_all_auths.
    fx.client.refund_escrow_funds(&invoice_id, &fx.business);

    let status = fx.client.get_escrow_status(&invoice_id);
    let details = fx.client.get_escrow_details(&invoice_id);

    assert_eq!(status, EscrowStatus::Refunded);
    assert_eq!(details.status, EscrowStatus::Refunded);
    assert_eq!(details.invoice_id, invoice_id);
    assert_eq!(details.investor, fx.investor);
    assert_eq!(details.business, fx.business);
    assert_eq!(details.amount, 1_000);
}

#[test]
fn test_escrow_status_consistency_multiple_operations() {
    let fx = EscrowFixture::new();
    let currency = fx.fund_investor(1_000);
    let (invoice_id, _) = fx.create_funded_escrow(1_000, &currency);

    // Snapshot before release.
    let status1 = fx.client.get_escrow_status(&invoice_id);
    let details1 = fx.client.get_escrow_details(&invoice_id);
    assert_eq!(status1, EscrowStatus::Held);
    assert_eq!(details1.status, EscrowStatus::Held);
    
    // Release escrow
    client.approve_early_escrow_release(&invoice_id, &business);
    client.approve_early_escrow_release(&invoice_id, &investor);
    client.release_escrow_funds(&invoice_id);
    
    // Test: Status should be Released after release
    let (status2, details2) = get_escrow_status_and_details(&env, &client, &invoice_id);
    assert_eq!(status2, EscrowStatus::Released);
    assert_eq!(details2.status, EscrowStatus::Released);

    // Immutable fields must survive the state transition.
    assert_eq!(details1.invoice_id, details2.invoice_id);
    assert_eq!(details1.investor, details2.investor);
    assert_eq!(details1.business, details2.business);
    assert_eq!(details1.amount, details2.amount);
    assert_eq!(details1.created_at, details2.created_at);
}

#[test]
#[should_panic]
fn test_escrow_status_error_missing_escrow() {
    // The generated Soroban client panics when the contract returns Err.
    let fx = EscrowFixture::new();
    let fake_id = BytesN::from_array(&fx.env, &[1; 32]);
    let _ = fx.client.get_escrow_status(&fake_id);
}

#[test]
fn test_escrow_status_consistency_with_real_token_transfers() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    let currency = Address::generate(&env);
    
    // Setup real token with initial balances
    let token_admin = Address::generate(&env);
    let token_contract = env.register_stellar_asset_contract_v2(token_admin);
    let token_client = token::Client::new(&env, &token_contract);
    
    let initial_balance = 10_000i128;
    token_client.mint(&business, &initial_balance);
    token_client.mint(&investor, &initial_balance);
    token_client.mint(&contract_id, &1i128);
    
    let expiration = env.ledger().sequence() + 1000;
    token_client.approve(&business, &contract_id, &initial_balance, &expiration);
    token_client.approve(&investor, &contract_id, &initial_balance, &expiration);
    
    // Create and verify invoice
    let due_date = env.ledger().timestamp() + 86_400 * 30;
    let invoice_id = client.store_invoice(
        &business,
        &1000i128,
        &currency,
        &due_date,
        &String::from_str(&env, "Test invoice with real escrow"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
        &None);
    client.verify_invoice(&invoice_id);
    
    // Setup and verify investor
    client.submit_investor_kyc(&investor, &String::from_str(&env, "Investor KYC"));
    let admin = Address::generate(&env);
    client.verify_investor(&admin, &investor, &5000i128);
    
    // Place bid and accept to create real escrow
    let bid_id = client.place_bid(&investor, &invoice_id, &1000i128, &1200i128, &BytesN::from_array(&env, &[0u8; 32]));
    client.accept_bid(&invoice_id, &bid_id);
    
    // Test: Real escrow should be created and consistent
    let (status, details) = get_escrow_status_and_details(&env, &client, &invoice_id);
    
    assert_eq!(status, EscrowStatus::Held);
    assert_eq!(details.status, EscrowStatus::Held);
    assert_eq!(details.invoice_id, invoice_id);
    assert_eq!(details.investor, investor);
    assert_eq!(details.business, business);
    assert_eq!(details.amount, 1000i128);
    assert!(details.created_at > 0);
    
    // Verify actual token transfers occurred
    assert_eq!(token_client.balance(&investor), initial_balance - 1000i128);
    assert_eq!(token_client.balance(&contract_id), 1000i128);
}

#[test]
fn test_escrow_status_consistency_state_transitions() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    // Create funded invoice with escrow
    let (invoice_id, _bid_id) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        1000i128,
        1000i128,
    );
    
    // Test state transitions and consistency at each step
    let mut previous_details: Option<crate::payments::Escrow> = None;
    
    // Initial: Held state
    let (status1, details1) = get_escrow_status_and_details(&env, &client, &invoice_id);
    assert_eq!(status1, EscrowStatus::Held);
    assert_eq!(details1.status, EscrowStatus::Held);
    previous_details = Some(details1.clone());
    
    // After release: Released state
    client.approve_early_escrow_release(&invoice_id, &business);
    client.approve_early_escrow_release(&invoice_id, &investor);
    client.release_escrow_funds(&invoice_id);
    let (status2, details2) = get_escrow_status_and_details(&env, &client, &invoice_id);
    assert_eq!(status2, EscrowStatus::Released);
    assert_eq!(details2.status, EscrowStatus::Released);
    
    // Verify only status changed, other fields remained same
    if let Some(prev) = previous_details {
        assert_eq!(prev.invoice_id, details2.invoice_id);
        assert_eq!(prev.investor, details2.investor);
        assert_eq!(prev.business, details2.business);
        assert_eq!(prev.amount, details2.amount);
        assert_eq!(prev.created_at, details2.created_at);
    }
    
    // Test that Released cannot be released again (idempotency)
    let result = client.release_escrow_funds(&invoice_id);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), QuickLendXError::InvalidStatus);
}

    // Fund the investor with enough for two escrows (1 000 + 2 000).
    let token_admin = Address::generate(&fx.env);
    let currency = fx
        .env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let sac = token::StellarAssetClient::new(&fx.env, &currency);
    let tok = token::Client::new(&fx.env, &currency);
    sac.mint(&fx.investor, &3_000i128);
    sac.mint(&fx.contract_id, &1i128);
    let exp = fx.env.ledger().sequence() + 10_000;
    tok.approve(&fx.investor, &fx.contract_id, &3_000i128, &exp);

    // KYC once, then create two independent invoices.
    fx.setup_kyc(10_000);
    let (invoice_id1, _) = fx.fund_invoice(1_000, &currency);
    let (invoice_id2, _) = fx.fund_invoice(2_000, &currency);

    // Release only the first escrow.
    fx.client.release_escrow_funds(&invoice_id1);

    let status1 = fx.client.get_escrow_status(&invoice_id1);
    let details1 = fx.client.get_escrow_details(&invoice_id1);
    let status2 = fx.client.get_escrow_status(&invoice_id2);
    let details2 = fx.client.get_escrow_details(&invoice_id2);

#[test]
fn test_escrow_status_consistency_with_multiple_invoices() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    
    let business = Address::generate(&env);
    let investor = Address::generate(&env);
    
    // Create two invoices with escrows
    let (invoice_id1, _bid_id1) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        1000i128,
        1000i128,
    );
    
    let (invoice_id2, _bid_id2) = create_funded_invoice_with_escrow_for_tests(
        &env,
        &client,
        &business,
        &investor,
        2000i128,
        2000i128,
    );
    
    // Release first escrow
    client.approve_early_escrow_release(&invoice_id1, &business);
    client.approve_early_escrow_release(&invoice_id1, &investor);
    client.release_escrow_funds(&invoice_id1);
    
    // Test: Each escrow should have consistent status/details independently
    let (status1, details1) = get_escrow_status_and_details(&env, &client, &invoice_id1);
    let (status2, details2) = get_escrow_status_and_details(&env, &client, &invoice_id2);
    
    // First escrow should be Released
    assert_eq!(status1, EscrowStatus::Released);
    assert_eq!(details1.status, EscrowStatus::Released);

    assert_eq!(status2, EscrowStatus::Held);
    assert_eq!(details2.status, EscrowStatus::Held);

    // The two escrows are independent.
    assert_ne!(details1.invoice_id, details2.invoice_id);
    assert_ne!(details1.amount, details2.amount);
}
