use super::*;
use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus, InvoiceStorage};
use crate::payments::{EscrowStatus, EscrowStorage};
use crate::reentrancy::{is_payment_guard_locked, with_payment_guard};
use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, String, Vec};

/// Test fixture for payment-path reentrancy regressions.
struct PaymentFixture {
    env: Env,
    contract_id: Address,
    admin: Address,
    business: Address,
    investor: Address,
    currency: Address,
}

impl PaymentFixture {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);

        let admin = Address::generate(&env);
        let business = Address::generate(&env);
        let investor = Address::generate(&env);
        let token_admin = Address::generate(&env);
        let currency = env
            .register_stellar_asset_contract_v2(token_admin)
            .address();

        let initial_balance = 100_000i128;
        let expiration = env.ledger().sequence() + 10_000;
        let token_client = token::Client::new(&env, &currency);
        let sac_client = token::StellarAssetClient::new(&env, &currency);

        sac_client.mint(&business, &initial_balance);
        sac_client.mint(&investor, &initial_balance);
        token_client.approve(&business, &contract_id, &initial_balance, &expiration);
        token_client.approve(&investor, &contract_id, &initial_balance, &expiration);

        client.set_admin(&admin);
        client.submit_kyc_application(&business, &String::from_str(&env, "business-kyc"));
        client.verify_business(&admin, &business);
        client.submit_investor_kyc(&investor, &String::from_str(&env, "investor-kyc"));
        client.verify_investor(&investor, &initial_balance);

        Self {
            env,
            contract_id,
            admin,
            business,
            investor,
            currency,
        }
    }

    fn client(&self) -> QuickLendXContractClient<'_> {
        QuickLendXContractClient::new(&self.env, &self.contract_id)
    }

    fn token_client(&self) -> token::Client<'_> {
        token::Client::new(&self.env, &self.currency)
    }

    fn create_invoice_with_bid(
        &self,
        invoice_amount: i128,
        bid_amount: i128,
    ) -> (BytesN<32>, BytesN<32>) {
        let due_date = self.env.ledger().timestamp() + 86_400;
        let client = self.client();
        let invoice_id = client.store_invoice(
            &self.business,
            &invoice_amount,
            &self.currency,
            &due_date,
            &String::from_str(&self.env, "reentrancy-regression"),
            &InvoiceCategory::Services,
            &Vec::new(&self.env),
        );
        client.verify_invoice(&invoice_id);

        let bid_id = client.place_bid(
            &self.investor,
            &invoice_id,
            &bid_amount,
            &(invoice_amount + 100),
        );

        (invoice_id, bid_id)
    }

    fn fund_invoice(&self, invoice_id: &BytesN<32>, bid_id: &BytesN<32>) {
        self.client().accept_bid(invoice_id, bid_id);
    }

    fn invoice(&self, invoice_id: &BytesN<32>) -> crate::invoice::Invoice {
        self.env.as_contract(&self.contract_id, || {
            InvoiceStorage::get_invoice(&self.env, invoice_id).unwrap()
        })
    }

    fn escrow(&self, invoice_id: &BytesN<32>) -> crate::payments::Escrow {
        self.env.as_contract(&self.contract_id, || {
            EscrowStorage::get_escrow_by_invoice(&self.env, invoice_id).unwrap()
        })
    }

    fn guard_locked(&self) -> bool {
        self.env
            .as_contract(&self.contract_id, || is_payment_guard_locked(&self.env))
    }

    fn business_balance(&self) -> i128 {
        self.token_client().balance(&self.business)
    }

    fn investor_balance(&self) -> i128 {
        self.token_client().balance(&self.investor)
    }

    fn contract_balance(&self) -> i128 {
        self.token_client().balance(&self.contract_id)
    }
}

fn run_nested_attempt<R, F>(fixture: &PaymentFixture, operation: F) -> Result<R, QuickLendXError>
where
    F: FnOnce() -> Result<R, QuickLendXError>,
{
    fixture
        .env
        .as_contract(&fixture.contract_id, || with_payment_guard(&fixture.env, operation))
}

#[test]
fn test_guard_rejects_direct_nested_execution_and_clears_lock() {
    let fixture = PaymentFixture::new();

    let result = run_nested_attempt(&fixture, || {
        with_payment_guard(&fixture.env, || Ok::<(), QuickLendXError>(()))
    });

    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    assert!(!fixture.guard_locked(), "guard must clear after nested rejection");
}

#[test]
fn test_guard_clears_after_inner_error() {
    let fixture = PaymentFixture::new();

    let result: Result<(), QuickLendXError> = fixture.env.as_contract(&fixture.contract_id, || {
        with_payment_guard(&fixture.env, || Err(QuickLendXError::InvoiceNotFound))
    });

    assert_eq!(result, Err(QuickLendXError::InvoiceNotFound));
    assert!(!fixture.guard_locked(), "guard must clear after inner errors");
}

#[test]
fn test_nested_accept_bid_is_rejected_without_state_change() {
    let fixture = PaymentFixture::new();
    let (invoice_id, bid_id) = fixture.create_invoice_with_bid(1_000, 1_000);

    let result = run_nested_attempt(&fixture, || {
        QuickLendXContract::accept_bid(
            fixture.env.clone(),
            invoice_id.clone(),
            bid_id.clone(),
        )
    });

    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    assert_eq!(fixture.invoice(&invoice_id).status, InvoiceStatus::Verified);
    assert!(fixture.env.as_contract(&fixture.contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&fixture.env, &invoice_id).is_none()
    }));
    assert!(!fixture.guard_locked());

    fixture.client().accept_bid(&invoice_id, &bid_id);
    assert_eq!(fixture.invoice(&invoice_id).status, InvoiceStatus::Funded);
}

#[test]
fn test_nested_accept_bid_and_fund_is_rejected_without_state_change() {
    let fixture = PaymentFixture::new();
    let (invoice_id, bid_id) = fixture.create_invoice_with_bid(1_200, 1_000);

    let result = run_nested_attempt(&fixture, || {
        QuickLendXContract::accept_bid_and_fund(
            fixture.env.clone(),
            invoice_id.clone(),
            bid_id.clone(),
        )
    });

    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    assert_eq!(fixture.invoice(&invoice_id).status, InvoiceStatus::Verified);
    assert!(fixture.env.as_contract(&fixture.contract_id, || {
        EscrowStorage::get_escrow_by_invoice(&fixture.env, &invoice_id).is_none()
    }));
    assert!(!fixture.guard_locked());
}

#[test]
fn test_nested_release_escrow_is_rejected_without_releasing_funds() {
    let fixture = PaymentFixture::new();
    let (invoice_id, bid_id) = fixture.create_invoice_with_bid(1_000, 1_000);
    fixture.fund_invoice(&invoice_id, &bid_id);

    let business_before = fixture.business_balance();
    let contract_before = fixture.contract_balance();

    let result = run_nested_attempt(&fixture, || {
        QuickLendXContract::release_escrow_funds(fixture.env.clone(), invoice_id.clone())
    });

    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    assert_eq!(fixture.escrow(&invoice_id).status, EscrowStatus::Held);
    assert_eq!(fixture.invoice(&invoice_id).status, InvoiceStatus::Funded);
    assert_eq!(fixture.business_balance(), business_before);
    assert_eq!(fixture.contract_balance(), contract_before);
    assert!(!fixture.guard_locked());
}

#[test]
fn test_nested_refund_escrow_is_rejected_without_refunding_funds() {
    let fixture = PaymentFixture::new();
    let (invoice_id, bid_id) = fixture.create_invoice_with_bid(1_000, 1_000);
    fixture.fund_invoice(&invoice_id, &bid_id);

    let investor_before = fixture.investor_balance();
    let contract_before = fixture.contract_balance();

    let result = run_nested_attempt(&fixture, || {
        QuickLendXContract::refund_escrow_funds(
            fixture.env.clone(),
            invoice_id.clone(),
            fixture.admin.clone(),
        )
    });

    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    assert_eq!(fixture.escrow(&invoice_id).status, EscrowStatus::Held);
    assert_eq!(fixture.invoice(&invoice_id).status, InvoiceStatus::Funded);
    assert_eq!(fixture.investor_balance(), investor_before);
    assert_eq!(fixture.contract_balance(), contract_before);
    assert!(!fixture.guard_locked());
}

#[test]
fn test_nested_settle_invoice_is_rejected_without_payout() {
    let fixture = PaymentFixture::new();
    let (invoice_id, bid_id) = fixture.create_invoice_with_bid(1_000, 900);
    fixture.fund_invoice(&invoice_id, &bid_id);

    let business_before = fixture.business_balance();
    let investor_before = fixture.investor_balance();
    let contract_before = fixture.contract_balance();

    let result = run_nested_attempt(&fixture, || {
        QuickLendXContract::settle_invoice(fixture.env.clone(), invoice_id.clone(), 1_000)
    });

    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    let invoice = fixture.invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.total_paid, 0);
    assert_eq!(fixture.business_balance(), business_before);
    assert_eq!(fixture.investor_balance(), investor_before);
    assert_eq!(fixture.contract_balance(), contract_before);
    assert!(!fixture.guard_locked());
}

#[test]
fn test_nested_partial_payment_is_rejected_without_recording_progress() {
    let fixture = PaymentFixture::new();
    let (invoice_id, bid_id) = fixture.create_invoice_with_bid(1_000, 900);
    fixture.fund_invoice(&invoice_id, &bid_id);

    let business_before = fixture.business_balance();
    let investor_before = fixture.investor_balance();
    let contract_before = fixture.contract_balance();

    let result = run_nested_attempt(&fixture, || {
        QuickLendXContract::process_partial_payment(
            fixture.env.clone(),
            invoice_id.clone(),
            400,
            String::from_str(&fixture.env, "nested-callback"),
        )
    });

    assert_eq!(result, Err(QuickLendXError::OperationNotAllowed));
    let invoice = fixture.invoice(&invoice_id);
    assert_eq!(invoice.status, InvoiceStatus::Funded);
    assert_eq!(invoice.total_paid, 0);
    assert_eq!(invoice.payment_history.len(), 0);
    assert_eq!(fixture.business_balance(), business_before);
    assert_eq!(fixture.investor_balance(), investor_before);
    assert_eq!(fixture.contract_balance(), contract_before);
    assert!(!fixture.guard_locked());
}
