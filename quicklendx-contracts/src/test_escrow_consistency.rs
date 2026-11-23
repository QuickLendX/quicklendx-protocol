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

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal fixture: contract, admin, two party addresses.
struct EscrowFixture {
    env: Env,
    client: QuickLendXContractClient<'static>,
    contract_id: Address,
    admin: Address,
    business: Address,
    investor: Address,
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
    let fx = EscrowFixture::new();
    let currency = fx.fund_investor(1_000);
    let (invoice_id, _) = fx.create_funded_escrow(1_000, &currency);

    fx.client.release_escrow_funds(&invoice_id);

    let status = fx.client.get_escrow_status(&invoice_id);
    let details = fx.client.get_escrow_details(&invoice_id);

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

    fx.client.release_escrow_funds(&invoice_id);

    let status2 = fx.client.get_escrow_status(&invoice_id);
    let details2 = fx.client.get_escrow_details(&invoice_id);
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
#[should_panic]
fn test_escrow_details_error_missing_escrow() {
    let fx = EscrowFixture::new();
    let fake_id = BytesN::from_array(&fx.env, &[2; 32]);
    let _ = fx.client.get_escrow_details(&fake_id);
}

#[test]
fn test_escrow_status_consistency_with_multiple_invoices() {
    let fx = EscrowFixture::new();

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

    assert_eq!(status1, EscrowStatus::Released);
    assert_eq!(details1.status, EscrowStatus::Released);

    assert_eq!(status2, EscrowStatus::Held);
    assert_eq!(details2.status, EscrowStatus::Held);

    // The two escrows are independent.
    assert_ne!(details1.invoice_id, details2.invoice_id);
    assert_ne!(details1.amount, details2.amount);
}
