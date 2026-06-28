use super::*;
use crate::alloc::string::ToString;
use crate::errors::QuickLendXError;
use crate::invoice::InvoiceCategory;
use crate::storage::InvoiceStorage;
use crate::types::InvoiceStatus;
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, BytesN, Env, String, Vec,
};

struct TestFixture {
    env: Env,
    client: QuickLendXContractClient<'static>,
    contract_id: Address,
    admin: Address,
    business: Address,
    investor: Address,
    currency: Address,
}

impl TestFixture {
    fn setup() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let contract_id = env.register(QuickLendXContract, ());
        let client = QuickLendXContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        client.set_admin(&admin);
        client.initialize_fee_system(&admin);
        let business = Address::generate(&env);
        let investor = Address::generate(&env);

        // KYC once
        let kyc = String::from_str(&env, "KYC");
        client.submit_kyc_application(&business, &kyc);
        client.verify_business(&admin, &business);
        client.submit_investor_kyc(&investor, &kyc);
        client.verify_investor(&investor, &200_000i128);

        let currency = {
            let token_admin = Address::generate(&env);
            let c = env
                .register_stellar_asset_contract_v2(token_admin)
                .address();
            let sac = token::StellarAssetClient::new(&env, &c);
            sac.mint(&business, &100_000i128);
            sac.mint(&investor, &100_000i128);
            sac.mint(&contract_id, &1i128);
            let tok = token::Client::new(&env, &c);
            let exp = env.ledger().sequence() + 50_000;
            tok.approve(&business, &contract_id, &400_000i128, &exp);
            tok.approve(&investor, &contract_id, &400_000i128, &exp);
            c
        };

        TestFixture {
            env,
            client,
            contract_id,
            admin,
            business,
            investor,
            currency,
        }
    }

    fn create_paid_invoice(&self, amount: i128, timestamp: u64) -> BytesN<32> {
        self.env.ledger().set_timestamp(timestamp);
        let due_date = timestamp + 86_400;
        let invoice_id = self.client.upload_invoice(
            &self.business,
            &amount,
            &self.currency,
            &due_date,
            &String::from_str(&self.env, "Test invoice"),
            &InvoiceCategory::Services,
            &Vec::new(&self.env),
        );
        self.client.verify_invoice(&invoice_id);
        let bid_id = self.client.place_bid(&self.investor, &invoice_id, &amount, &(amount + 100), &BytesN::from_array(&self.env, &[0u8; 32]));
        self.client.accept_bid(&invoice_id, &bid_id);
        self.env.ledger().set_timestamp(timestamp + 10);
        self.client.settle_invoice(&invoice_id, &amount);
        let inv = self.client.get_invoice(&invoice_id);
        assert_eq!(inv.status, InvoiceStatus::Paid);
        invoice_id
    }

    fn create_invoice_with_status(
        &self,
        amount: i128,
        status: InvoiceStatus,
        timestamp: u64,
    ) -> BytesN<32> {
        match status {
            InvoiceStatus::Paid => self.create_paid_invoice(amount, timestamp),
            InvoiceStatus::Defaulted | InvoiceStatus::Refunded => {
                self.env.ledger().set_timestamp(timestamp);
                let due_date = timestamp + 86_400;
                let invoice_id = self.client.upload_invoice(
                    &self.business,
                    &amount,
                    &self.currency,
                    &due_date,
                    &String::from_str(&self.env, "Test invoice"),
                    &InvoiceCategory::Services,
                    &Vec::new(&self.env),
                );
                self.client.verify_invoice(&invoice_id);
                let bid_id = self.client.place_bid(&self.investor, &invoice_id, &amount, &(amount + 100), &BytesN::from_array(&self.env, &[0u8; 32]));
                self.client.accept_bid(&invoice_id, &bid_id);
                match status {
                    InvoiceStatus::Defaulted => {
                        self.env.ledger().set_timestamp(due_date + 1);
                        self.client.expire_invoice(&invoice_id);
                        self.env.as_contract(&self.client.address, || {
                            let mut inv = InvoiceStorage::get(&self.env, &invoice_id).unwrap();
                            inv.mark_as_defaulted();
                            InvoiceStorage::store(&self.env, &inv);
                        });
                    }
                    InvoiceStatus::Refunded => {
                        self.env.ledger().set_timestamp(due_date + 1);
                        self.client.refund_escrow_funds(&invoice_id, &self.business);
                    }
                    _ => {}
                }
                let inv = self.client.get_invoice(&invoice_id);
                assert_eq!(inv.status, status);
                invoice_id
            }
            _ => {
                self.env.ledger().set_timestamp(timestamp);
                let due_date = timestamp + 86_400;
                let invoice_id = self.client.upload_invoice(
                    &self.business,
                    &amount,
                    &self.currency,
                    &due_date,
                    &String::from_str(&self.env, "Test invoice"),
                    &InvoiceCategory::Services,
                    &Vec::new(&self.env),
                );
                if status == InvoiceStatus::Verified {
                    self.client.verify_invoice(&invoice_id);
                }
                invoice_id
            }
        }
    }

    fn funded_invoice(&self, timestamp: u64) -> BytesN<32> {
        self.env.ledger().set_timestamp(timestamp);
        let due_date = timestamp + 86_400;
        let invoice_id = self.client.upload_invoice(
            &self.business,
            &1000,
            &self.currency,
            &due_date,
            &String::from_str(&self.env, "Test"),
            &InvoiceCategory::Services,
            &Vec::new(&self.env),
        );
        self.client.verify_invoice(&invoice_id);
        let bid_id = self.client.place_bid(&self.investor, &invoice_id, &1000, &1100, &BytesN::from_array(&self.env, &[0u8; 32]));
        self.client.accept_bid(&invoice_id, &bid_id);
        invoice_id

    }
}
#[test]
fn test_prune_only_terminal() {
    let fx = TestFixture::setup();
    let now = 1_000_000u64;
    let retention = 86_400u64;

    let old_paid = fx.create_invoice_with_status(1000, InvoiceStatus::Paid, now - 200_000);
    let pending = fx.create_invoice_with_status(1000, InvoiceStatus::Pending, now - 200_000);

    fx.env.ledger().set_timestamp(now);

    let report = fx
        .client
        .prune_terminal_invoices(&fx.admin, &retention, &0, &100);
    assert_eq!(report.pruned, 1, "should prune 1 terminal invoice");
    assert_eq!(report.scanned, 2, "should scan 2 invoices total");

    assert!(fx.client.try_get_invoice(&old_paid).is_err());
    let inv = fx.client.get_invoice(&pending);
    assert_eq!(inv.status, InvoiceStatus::Pending);
}

/// Test: invoices within retention window are not pruned
#[test]
fn test_within_retention_window_not_pruned() {
    let fx = TestFixture::setup();
    let now = 1_000_000u64;
    let retention = 86_400u64;

    let recent_paid = fx.create_invoice_with_status(1000, InvoiceStatus::Paid, now - 1000);

    fx.env.ledger().set_timestamp(now);

    let report = fx
        .client
        .prune_terminal_invoices(&fx.admin, &retention, &0, &100);
    assert_eq!(report.pruned, 0, "should not prune recent invoice");
    assert_eq!(report.scanned, 1);

    let inv = fx.client.get_invoice(&recent_paid);
    assert_eq!(inv.status, InvoiceStatus::Paid);
}

/// Test: funded invoice is never pruned
#[test]
fn test_funded_invoice_never_pruned() {
    let fx = TestFixture::setup();
    let now = 1_000_000u64;

    let invoice_id = fx.funded_invoice(now - 200_000);
    let inv = fx.client.get_invoice(&invoice_id);
    assert_eq!(inv.status, InvoiceStatus::Funded);

    fx.env.ledger().set_timestamp(now);

    let report = fx.client.prune_terminal_invoices(&fx.admin, &0, &0, &100);
    assert_eq!(report.pruned, 0, "funded invoice should never be pruned");
    assert_eq!(report.scanned, 1);

    let inv = fx.client.get_invoice(&invoice_id);
    assert_eq!(inv.status, InvoiceStatus::Funded);
}

/// Test: pagination works correctly (resumable via next_offset)
#[test]
fn test_pagination() {
    let fx = TestFixture::setup();
    let now = 1_000_000u64;
    let retention = 86_400u64;

    // Create invoices at different timestamps so we know which are pruned
    let _id1 = fx.create_invoice_with_status(1000, InvoiceStatus::Paid, now - 200_000);
    let _id2 = fx.create_invoice_with_status(1000, InvoiceStatus::Paid, now - 200_000);
    let _id3 = fx.create_invoice_with_status(1000, InvoiceStatus::Paid, now - 200_000);

    fx.env.ledger().set_timestamp(now);

    // Page 1: offset 0, limit 2
    let r1 = fx
        .client
        .prune_terminal_invoices(&fx.admin, &retention, &0, &2);
    assert_eq!(r1.pruned, 2);
    assert_eq!(r1.scanned, 2);
    assert!(r1.next_offset > 0);

    // Page 2: resume at next_offset (total shrank to 1, offset past end → empty)
    let r2 = fx
        .client
        .prune_terminal_invoices(&fx.admin, &retention, &r1.next_offset, &2);
    assert_eq!(r2.pruned, 0);
    assert_eq!(r2.scanned, 0);

    // Restart from offset 0 to get the remaining one
    let r3 = fx
        .client
        .prune_terminal_invoices(&fx.admin, &retention, &0, &100);
    assert_eq!(r3.pruned, 1);
    assert_eq!(r3.scanned, 1);
}

/// Test: admin auth required
#[test]
fn test_non_admin_rejected() {
    let fx = TestFixture::setup();
    let stranger = Address::generate(&fx.env);
    let err = fx
        .client
        .try_prune_terminal_invoices(&stranger, &0, &0, &100)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, QuickLendXError::NotAdmin);
}

/// Test: zero retention prunes all terminal invoices
#[test]
fn test_zero_retention_prunes_all_terminal() {
    let fx = TestFixture::setup();
    let now = 1_000_000u64;

    let _id1 = fx.create_invoice_with_status(1000, InvoiceStatus::Paid, now - 200_000);
    let _id2 = fx.create_invoice_with_status(1000, InvoiceStatus::Paid, now - 200_000);

    fx.env.ledger().set_timestamp(now);

    let report = fx.client.prune_terminal_invoices(&fx.admin, &0, &0, &100);
    assert_eq!(report.pruned, 2);
}

/// Test: defaulted and refunded invoices are pruned
#[test]
fn test_defaulted_and_refunded_pruned() {
    let fx = TestFixture::setup();
    let now = 1_000_000u64;
    let retention = 86_400u64;

    let defaulted_id = fx.create_invoice_with_status(1000, InvoiceStatus::Defaulted, now - 200_000);
    let refunded_id = fx.create_invoice_with_status(1000, InvoiceStatus::Refunded, now - 200_000);

    fx.env.ledger().set_timestamp(now);

    let report = fx
        .client
        .prune_terminal_invoices(&fx.admin, &retention, &0, &100);
    assert_eq!(
        report.pruned, 2,
        "should prune defaulted and refunded invoices"
    );
    assert!(fx.client.try_get_invoice(&defaulted_id).is_err());
    assert!(fx.client.try_get_invoice(&refunded_id).is_err());
}

/// Test: PruneReport struct fields are correct
#[test]
fn test_prune_report_fields() {
    let fx = TestFixture::setup();
    let now = 1_000_000u64;

    let _id = fx.create_invoice_with_status(1000, InvoiceStatus::Paid, now - 200_000);

    fx.env.ledger().set_timestamp(now);

    let report = fx
        .client
        .prune_terminal_invoices(&fx.admin, &86_400, &0, &100);
    assert_eq!(report.scanned, 1);
    assert_eq!(report.pruned, 1);
    assert_eq!(report.next_offset, 1);
}

/// Test: limit cap
#[test]
fn test_limit_capped_at_max() {
    let fx = TestFixture::setup();
    let now = 1_000_000u64;

    for _ in 0..5 {
        fx.create_invoice_with_status(1000, InvoiceStatus::Paid, now - 200_000);
    }

    fx.env.ledger().set_timestamp(now);

    let report = fx.client.prune_terminal_invoices(&fx.admin, &0, &0, &200);
    assert_eq!(report.pruned, 5);
    assert_eq!(report.scanned, 5);
}

/// Test: offset beyond total
#[test]
fn test_offset_beyond_total() {
    let fx = TestFixture::setup();
    let now = 1_000_000u64;

    let _id = fx.create_invoice_with_status(1000, InvoiceStatus::Paid, now - 200_000);

    fx.env.ledger().set_timestamp(now);

    let report = fx
        .client
        .prune_terminal_invoices(&fx.admin, &86_400, &100, &10);
    assert_eq!(report.pruned, 0);
    assert_eq!(report.scanned, 0);
}

/// Test: no terminal invoices
#[test]
fn test_no_terminal_invoices() {
    let fx = TestFixture::setup();
    let now = 1_000_000u64;

    fx.create_invoice_with_status(1000, InvoiceStatus::Pending, now - 200_000);

    fx.env.ledger().set_timestamp(now);

    let report = fx
        .client
        .prune_terminal_invoices(&fx.admin, &86_400, &0, &100);
    assert_eq!(report.pruned, 0);
    assert_eq!(report.scanned, 1);
}

/// Test: index cleanup - no orphan after prune
#[test]
fn test_index_cleanup_no_orphans() {
    let fx = TestFixture::setup();
    let now = 1_000_000u64;

    let invoice_id = fx.create_invoice_with_status(1000, InvoiceStatus::Paid, now - 200_000);

    fx.env.ledger().set_timestamp(now);

    let inv = fx.client.get_invoice(&invoice_id);
    assert_eq!(inv.status, InvoiceStatus::Paid);

    fx.client
        .prune_terminal_invoices(&fx.admin, &86_400, &0, &100);

    assert!(
        fx.client.try_get_invoice(&invoice_id).is_err(),
        "invoice should be deleted from persistent storage"
    );
}
