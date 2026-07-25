use super::*;
use crate::errors::QuickLendXError;
use crate::storage::InvoiceStorage;
use crate::types::{Invoice, InvoiceStatus};
use soroban_sdk::testutils::{Address as _, Ledger};
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

    fn create_pending_invoice(&self) -> BytesN<32> {
        let due_date = self.env.ledger().timestamp() + 86_400;
        self.client.upload_invoice(
            &self.business,
            &1_000i128,
            &self.currency,
            &due_date,
            &String::from_str(&self.env, "Test invoice"),
            &InvoiceCategory::Services,
            &Vec::new(&self.env),
        )
    }

    fn create_verified_invoice(&self) -> BytesN<32> {
        let invoice_id = self.create_pending_invoice();
        self.client.verify_invoice(&invoice_id);
        invoice_id
    }

    fn create_funded_invoice(&self) -> BytesN<32> {
        let invoice_id = self.create_verified_invoice();
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

    fn create_paid_invoice(&self) -> BytesN<32> {
        let invoice_id = self.create_funded_invoice();
        self.client.settle_invoice(&invoice_id, &1_000i128);
        invoice_id
    }

    fn create_defaulted_invoice(&self) -> BytesN<32> {
        let invoice_id = self.create_funded_invoice();
        self.env.ledger().set_timestamp(self.env.ledger().timestamp() + 86_400 * 40);
        self.client.handle_overdue_invoices(&100u32);
        invoice_id
    }

    fn create_cancelled_invoice(&self) -> BytesN<32> {
        let invoice_id = self.create_pending_invoice();
        self.client.cancel_invoice(&invoice_id);
        invoice_id
    }

    fn create_refunded_invoice(&self) -> BytesN<32> {
        let invoice_id = self.create_funded_invoice();
        self.client.refund_escrow_funds(&invoice_id, &self.business);
        invoice_id
    }

    fn invoice_status(&self, invoice_id: &BytesN<32>) -> InvoiceStatus {
        self.client.get_invoice(invoice_id).status
    }
}

#[test]
fn test_transition_pending_to_verified() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_pending_invoice();
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Pending);
    ctx.client.verify_invoice(&invoice_id);
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Verified);
}

#[test]
fn test_transition_pending_to_cancelled() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_pending_invoice();
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Pending);
    ctx.client.cancel_invoice(&invoice_id);
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Cancelled);
}

#[test]
fn test_transition_verified_to_funded() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Verified);
    let bid_id = ctx.client.place_bid(
        &ctx.investor,
        &invoice_id,
        &1_000i128,
        &1_000i128,
        &BytesN::from_array(&ctx.env, &[0u8; 32]),
    );
    ctx.client.accept_bid(&invoice_id, &bid_id);
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Funded);
}

#[test]
fn test_transition_verified_to_cancelled() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Verified);
    ctx.client.cancel_invoice(&invoice_id);
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Cancelled);
}

#[test]
fn test_transition_funded_to_paid() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Funded);
    ctx.client.settle_invoice(&invoice_id, &1_000i128);
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Paid);
}

#[test]
fn test_transition_funded_to_defaulted() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Funded);
    ctx.env.ledger().set_timestamp(ctx.env.ledger().timestamp() + 86_400 * 40);
    ctx.client.handle_overdue_invoices(&100u32);
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Defaulted);
}

#[test]
fn test_transition_funded_to_refunded() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Funded);
    ctx.client.refund_escrow_funds(&invoice_id, &ctx.business);
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Refunded);
}

#[test]
fn test_transition_funded_to_cancelled() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_funded_invoice();
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Funded);
    ctx.client.cancel_invoice(&invoice_id);
    assert_eq!(ctx.invoice_status(&invoice_id), InvoiceStatus::Cancelled);
}

#[test]
fn test_cannot_verify_pending_twice() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_pending_invoice();
    ctx.client.verify_invoice(&invoice_id);
    let result = ctx.client.try_verify_invoice(&invoice_id);
    assert!(result.is_err());
}

#[test]
fn test_cannot_verify_non_pending() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    let result = ctx.client.try_verify_invoice(&invoice_id);
    assert!(result.is_err());
}

#[test]
fn test_cannot_accept_bid_from_pending() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_pending_invoice();
    let bid_id = ctx.client.place_bid(
        &ctx.investor,
        &invoice_id,
        &1_000i128,
        &1_000i128,
        &BytesN::from_array(&ctx.env, &[0u8; 32]),
    );
    let result = ctx.client.try_accept_bid(&invoice_id, &bid_id);
    assert!(result.is_err());
}

#[test]
fn test_cannot_settle_non_funded() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    let result = ctx.client.try_settle_invoice(&invoice_id, &1_000i128);
    assert!(result.is_err());
}

#[test]
fn test_cannot_settle_already_paid() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_paid_invoice();
    let result = ctx.client.try_settle_invoice(&invoice_id, &1_000i128);
    assert!(result.is_err());
}

#[test]
fn test_cannot_default_non_funded() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    ctx.env.ledger().set_timestamp(ctx.env.ledger().timestamp() + 86_400 * 40);
    ctx.client.handle_overdue_invoices(&100u32);
    let invoice = ctx.client.get_invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Verified, "default should not apply to non-funded invoices");
}

#[test]
fn test_cannot_refund_non_funded() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_verified_invoice();
    let result = ctx.client.try_refund_escrow_funds(&invoice_id, &ctx.business);
    assert!(result.is_err());
}

#[test]
fn test_cannot_refund_defaulted() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    let invoice_id = ctx.create_defaulted_invoice();
    let result = ctx.client.try_refund_escrow_funds(&invoice_id, &ctx.business);
    assert!(result.is_err());
}

#[test]
fn test_terminal_states_are_immutable() {
    let ctx = TestContext::new();
    ctx.setup_kyc();
    for create_fn in [
        Box::new(|| ctx.create_paid_invoice()) as Box<dyn Fn() -> BytesN<32>>,
        Box::new(|| ctx.create_defaulted_invoice()),
        Box::new(|| ctx.create_cancelled_invoice()),
        Box::new(|| ctx.create_refunded_invoice()),
    ] {
        let invoice_id = create_fn();
        let status = ctx.invoice_status(&invoice_id);
        assert!(
            status.is_terminal(),
            "{:?} should be terminal",
            status
        );
        let verify = ctx.client.try_verify_invoice(&invoice_id);
        assert!(verify.is_err(), "verify should fail from {:?}", status);
        let settle = ctx.client.try_settle_invoice(&invoice_id, &1_000i128);
        assert!(settle.is_err(), "settle should fail from {:?}", status);
        let refund = ctx.client.try_refund_escrow_funds(&invoice_id, &ctx.business);
        assert!(refund.is_err(), "refund should fail from {:?}", status);
    }
}

#[test]
fn test_transitions_guard_consistency() {
    let ctx = TestContext::new();
    ctx.setup_kyc();

    let transitions: Vec<(InvoiceStatus, InvoiceStatus, bool)> = vec![
        (InvoiceStatus::Pending, InvoiceStatus::Verified, true),
        (InvoiceStatus::Pending, InvoiceStatus::Cancelled, true),
        (InvoiceStatus::Pending, InvoiceStatus::Funded, false),
        (InvoiceStatus::Pending, InvoiceStatus::Paid, false),
        (InvoiceStatus::Pending, InvoiceStatus::Defaulted, false),
        (InvoiceStatus::Pending, InvoiceStatus::Refunded, false),
        (InvoiceStatus::Verified, InvoiceStatus::Funded, true),
        (InvoiceStatus::Verified, InvoiceStatus::Cancelled, true),
        (InvoiceStatus::Verified, InvoiceStatus::Verified, false),
        (InvoiceStatus::Verified, InvoiceStatus::Paid, false),
        (InvoiceStatus::Verified, InvoiceStatus::Defaulted, false),
        (InvoiceStatus::Verified, InvoiceStatus::Refunded, false),
        (InvoiceStatus::Funded, InvoiceStatus::Paid, true),
        (InvoiceStatus::Funded, InvoiceStatus::Defaulted, true),
        (InvoiceStatus::Funded, InvoiceStatus::Refunded, true),
        (InvoiceStatus::Funded, InvoiceStatus::Cancelled, true),
        (InvoiceStatus::Funded, InvoiceStatus::Verified, false),
        (InvoiceStatus::Funded, InvoiceStatus::Funded, false),
        (InvoiceStatus::Paid, InvoiceStatus::Cancelled, true),
        (InvoiceStatus::Paid, InvoiceStatus::Paid, false),
        (InvoiceStatus::Paid, InvoiceStatus::Verified, false),
        (InvoiceStatus::Paid, InvoiceStatus::Funded, false),
        (InvoiceStatus::Paid, InvoiceStatus::Defaulted, false),
        (InvoiceStatus::Paid, InvoiceStatus::Refunded, false),
        (InvoiceStatus::Defaulted, InvoiceStatus::Cancelled, true),
        (InvoiceStatus::Defaulted, InvoiceStatus::Defaulted, false),
        (InvoiceStatus::Defaulted, InvoiceStatus::Verified, false),
        (InvoiceStatus::Defaulted, InvoiceStatus::Funded, false),
        (InvoiceStatus::Defaulted, InvoiceStatus::Paid, false),
        (InvoiceStatus::Defaulted, InvoiceStatus::Refunded, false),
        (InvoiceStatus::Cancelled, InvoiceStatus::Cancelled, true),
        (InvoiceStatus::Cancelled, InvoiceStatus::Verified, false),
        (InvoiceStatus::Cancelled, InvoiceStatus::Funded, false),
        (InvoiceStatus::Cancelled, InvoiceStatus::Paid, false),
        (InvoiceStatus::Cancelled, InvoiceStatus::Defaulted, false),
        (InvoiceStatus::Cancelled, InvoiceStatus::Refunded, false),
        (InvoiceStatus::Refunded, InvoiceStatus::Cancelled, true),
        (InvoiceStatus::Refunded, InvoiceStatus::Verified, false),
        (InvoiceStatus::Refunded, InvoiceStatus::Funded, false),
        (InvoiceStatus::Refunded, InvoiceStatus::Paid, false),
        (InvoiceStatus::Refunded, InvoiceStatus::Defaulted, false),
        (InvoiceStatus::Refunded, InvoiceStatus::Refunded, false),
    ];

    for (from, to, should_succeed) in transitions {
        let from_label = format!("{:?}", from);
        let to_label = format!("{:?}", to);

        let invoice_id = match from {
            InvoiceStatus::Pending => ctx.create_pending_invoice(),
            InvoiceStatus::Verified => ctx.create_verified_invoice(),
            InvoiceStatus::Funded => ctx.create_funded_invoice(),
            InvoiceStatus::Paid => ctx.create_paid_invoice(),
            InvoiceStatus::Defaulted => ctx.create_defaulted_invoice(),
            InvoiceStatus::Cancelled => ctx.create_cancelled_invoice(),
            InvoiceStatus::Refunded => ctx.create_refunded_invoice(),
        };

        let result = match (from, to) {
            (_, InvoiceStatus::Verified) => ctx.client.try_verify_invoice(&invoice_id),
            (_, InvoiceStatus::Funded) => {
                let bid_id = ctx.client.place_bid(
                    &ctx.investor,
                    &invoice_id,
                    &1_000i128,
                    &1_000i128,
                    &BytesN::from_array(&ctx.env, &[0u8; 32]),
                );
                ctx.client.try_accept_bid(&invoice_id, &bid_id)
            }
            (_, InvoiceStatus::Cancelled) => ctx.client.try_cancel_invoice(&invoice_id),
            (_, InvoiceStatus::Paid) => ctx.client.try_settle_invoice(&invoice_id, &1_000i128),
            (_, InvoiceStatus::Defaulted) => {
                ctx.env.ledger().set_timestamp(ctx.env.ledger().timestamp() + 86_400 * 40);
                ctx.client.try_handle_overdue_invoices(&100u32)
            }
            (_, InvoiceStatus::Refunded) => {
                ctx.client.try_refund_escrow_funds(&invoice_id, &ctx.business)
            }
        };

        if should_succeed {
            let actual = ctx.invoice_status(&invoice_id);
            assert_eq!(
                actual, to,
                "expected {:?} → {:?} to set status to {:?}, got {:?}",
                from, to, to, actual
            );
        } else {
            assert!(
                result.is_err(),
                "expected {:?} → {:?} to fail, but it succeeded",
                from,
                to
            );
        }
    }
}