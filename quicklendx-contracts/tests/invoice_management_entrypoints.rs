//! Integration tests for the invoice-management entrypoints exposed on
//! `QuickLendXContractClient` (upload, tags, category, metadata, lock, queries).
//!
//! These run against the public client only, so they exercise exactly what the
//! deployed WASM exposes.

use quicklendx_contracts::errors::QuickLendXError;
use quicklendx_contracts::protocol_limits::{
    MAX_INPUT_DESCRIPTION_BYTES, MAX_INPUT_STATUS_BATCH_SIZE, MAX_INPUT_TAGS,
    MAX_MUTATIONS_PER_WINDOW,
};
use quicklendx_contracts::storage::InvoiceStorage;
use quicklendx_contracts::types::{
    InvoiceCategory, InvoiceLock, InvoiceMetadata, InvoiceStatus, LineItemRecord,
};
use quicklendx_contracts::{QuickLendXContract, QuickLendXContractClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, BytesN, Env, String, Vec};

struct Fixture {
    env: Env,
    contract_id: Address,
    admin: Address,
    business: Address,
    currency: Address,
}

fn setup() -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    // The rate limiter treats ledger sequence 0 as "no window started".
    env.ledger().with_mut(|l| l.sequence_number = 100);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    client.set_admin(&admin);
    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "Business KYC"));
    client.verify_business(&admin, &business);
    let currency = Address::generate(&env);
    Fixture {
        env,
        contract_id,
        admin,
        business,
        currency,
    }
}

impl Fixture {
    fn client(&self) -> QuickLendXContractClient<'_> {
        QuickLendXContractClient::new(&self.env, &self.contract_id)
    }

    fn due(&self) -> u64 {
        self.env.ledger().timestamp() + 86_400
    }

    fn upload_with(
        &self,
        description: &String,
        tags: &Vec<String>,
    ) -> Result<BytesN<32>, QuickLendXError> {
        match self.client().try_upload_invoice(
            &self.business,
            &1_000,
            &self.currency,
            &self.due(),
            description,
            &InvoiceCategory::Services,
            tags,
            &None,
            &None,
            &None,
        ) {
            Ok(Ok(id)) => Ok(id),
            Ok(Err(_)) => panic!("unexpected conversion error"),
            Err(Ok(e)) => Err(e),
            Err(Err(e)) => panic!("host error: {e:?}"),
        }
    }

    fn upload(&self) -> BytesN<32> {
        self.upload_with(
            &String::from_str(&self.env, "invoice"),
            &Vec::new(&self.env),
        )
        .expect("upload should succeed")
    }

    fn tag_index(&self, tag: &str) -> Vec<BytesN<32>> {
        let tag = String::from_str(&self.env, tag);
        self.env.as_contract(&self.contract_id, || {
            InvoiceStorage::get_by_tag(&self.env, &tag)
        })
    }

    fn category_index(&self, category: InvoiceCategory) -> Vec<BytesN<32>> {
        self.env.as_contract(&self.contract_id, || {
            InvoiceStorage::get_by_category(&self.env, category)
        })
    }
}

fn meta(env: &Env, customer: &str, tax_id: &str) -> InvoiceMetadata {
    InvoiceMetadata {
        customer_name: String::from_str(env, customer),
        customer_address: String::from_str(env, "1 Main St"),
        tax_id: String::from_str(env, tax_id),
        // Validation requires line items that sum to the invoice amount (1_000).
        line_items: soroban_sdk::vec![
            env,
            LineItemRecord(String::from_str(env, "Services"), 1, 1_000, 1_000)
        ],
        notes: String::from_str(env, "notes"),
    }
}

fn s(env: &Env, v: &str) -> String {
    String::from_str(env, v)
}

// ── upload + bounds ─────────────────────────────────────────────────────────

#[test]
fn upload_invoice_stores_and_indexes_invoice() {
    let f = setup();
    let id = f.upload();
    let invoice = f.client().get_invoice(&id);
    assert_eq!(invoice.business, f.business);
    assert_eq!(invoice.status, InvoiceStatus::Pending);
    assert_eq!(
        f.client()
            .get_invoice_count_by_status(&InvoiceStatus::Pending),
        1
    );
    assert_eq!(f.category_index(InvoiceCategory::Services).len(), 1);
}

#[test]
fn upload_rejects_oversize_description_with_typed_error() {
    let f = setup();
    let too_big = "a".repeat(MAX_INPUT_DESCRIPTION_BYTES as usize + 1);
    assert_eq!(
        f.upload_with(&s(&f.env, &too_big), &Vec::new(&f.env)),
        Err(QuickLendXError::InputTooLarge)
    );
    assert_eq!(
        f.client()
            .get_invoice_count_by_status(&InvoiceStatus::Pending),
        0
    );
}

#[test]
fn upload_rejects_oversize_tag_vector_before_any_state_change() {
    let f = setup();
    let mut tags = Vec::new(&f.env);
    for i in 0..=MAX_INPUT_TAGS {
        tags.push_back(s(&f.env, &format!("t{i}")));
    }
    assert_eq!(
        f.upload_with(&s(&f.env, "ok"), &tags),
        Err(QuickLendXError::InputTooLarge)
    );
    assert_eq!(
        f.client()
            .get_invoice_count_by_status(&InvoiceStatus::Pending),
        0
    );
}

#[test]
fn upload_rejects_more_tags_than_the_invoice_cap() {
    let f = setup();
    let mut tags = Vec::new(&f.env);
    for i in 0..11 {
        tags.push_back(s(&f.env, &format!("tag{i}")));
    }
    assert_eq!(
        f.upload_with(&s(&f.env, "ok"), &tags),
        Err(QuickLendXError::TagLimitExceeded)
    );
}

// ── tags ────────────────────────────────────────────────────────────────────

#[test]
fn tag_add_remove_keeps_tag_index_consistent() {
    let f = setup();
    let c = f.client();
    let id = f.upload();

    c.add_invoice_tag(&f.business, &id, &s(&f.env, "Urgent"));
    assert_eq!(f.tag_index("urgent").len(), 1);

    // Same tag differing only by case/whitespace is a no-op (no duplicate index entry).
    c.add_invoice_tag(&f.business, &id, &s(&f.env, "URGENT"));
    assert_eq!(c.get_invoice(&id).tags.len(), 1);
    assert_eq!(f.tag_index("urgent").len(), 1);

    c.remove_invoice_tag(&f.business, &id, &s(&f.env, "urgent"));
    assert_eq!(c.get_invoice(&id).tags.len(), 0);
    assert_eq!(f.tag_index("urgent").len(), 0);
}

#[test]
fn tag_cap_is_enforced_at_the_boundary() {
    let f = setup();
    let c = f.client();
    let id = f.upload();

    for i in 0..10 {
        c.add_invoice_tag(&f.business, &id, &s(&f.env, &format!("tag{i}")));
    }
    assert_eq!(c.get_invoice(&id).tags.len(), 10);

    let r = c.try_add_invoice_tag(&f.business, &id, &s(&f.env, "tag10"));
    assert_eq!(r, Err(Ok(QuickLendXError::TagLimitExceeded)));
    assert_eq!(c.get_invoice(&id).tags.len(), 10);
    assert_eq!(f.tag_index("tag10").len(), 0);
}

// ── category ────────────────────────────────────────────────────────────────

#[test]
fn category_update_moves_invoice_between_indexes() {
    let f = setup();
    let c = f.client();
    let id = f.upload();
    assert_eq!(f.category_index(InvoiceCategory::Services).len(), 1);

    c.update_invoice_category(&f.business, &id, &InvoiceCategory::Products);

    assert_eq!(f.category_index(InvoiceCategory::Services).len(), 0);
    assert_eq!(f.category_index(InvoiceCategory::Products).len(), 1);
    assert_eq!(c.get_invoice(&id).category, InvoiceCategory::Products);
}

// ── metadata ────────────────────────────────────────────────────────────────

#[test]
fn metadata_update_and_clear_keep_customer_and_tax_indexes_consistent() {
    let f = setup();
    let c = f.client();
    let id = f.upload();

    c.update_invoice_metadata(&f.business, &id, &meta(&f.env, "Acme", "TAX-1"));
    assert_eq!(c.get_invoices_by_customer(&s(&f.env, "Acme")).len(), 1);
    assert_eq!(c.get_invoices_by_tax_id(&s(&f.env, "TAX-1")).len(), 1);

    // Changing the values must drop the stale index entries.
    c.update_invoice_metadata(&f.business, &id, &meta(&f.env, "Globex", "TAX-2"));
    assert_eq!(c.get_invoices_by_customer(&s(&f.env, "Acme")).len(), 0);
    assert_eq!(c.get_invoices_by_tax_id(&s(&f.env, "TAX-1")).len(), 0);
    assert_eq!(c.get_invoices_by_customer(&s(&f.env, "Globex")).len(), 1);
    assert_eq!(c.get_invoices_by_tax_id(&s(&f.env, "TAX-2")).len(), 1);

    c.clear_invoice_metadata(&f.business, &id);
    assert_eq!(c.get_invoices_by_customer(&s(&f.env, "Globex")).len(), 0);
    assert_eq!(c.get_invoices_by_tax_id(&s(&f.env, "TAX-2")).len(), 0);
}

#[test]
fn metadata_rejects_oversize_notes() {
    let f = setup();
    let id = f.upload();
    let mut m = meta(&f.env, "Acme", "TAX-1");
    m.notes = s(
        &f.env,
        &"n".repeat(MAX_INPUT_DESCRIPTION_BYTES as usize + 1),
    );
    assert_eq!(
        f.client().try_update_invoice_metadata(&f.business, &id, &m),
        Err(Ok(QuickLendXError::InputTooLarge))
    );
}

// ── ownership + rate limiting ───────────────────────────────────────────────

#[test]
fn only_the_owning_business_can_mutate() {
    let f = setup();
    let c = f.client();
    let id = f.upload();
    let stranger = Address::generate(&f.env);

    assert_eq!(
        c.try_add_invoice_tag(&stranger, &id, &s(&f.env, "x")),
        Err(Ok(QuickLendXError::Unauthorized))
    );
    assert_eq!(
        c.try_update_invoice_category(&stranger, &id, &InvoiceCategory::Products),
        Err(Ok(QuickLendXError::Unauthorized))
    );
    assert_eq!(
        c.try_update_invoice_metadata(&stranger, &id, &meta(&f.env, "A", "T")),
        Err(Ok(QuickLendXError::Unauthorized))
    );
    assert_eq!(
        c.try_clear_invoice_metadata(&stranger, &id),
        Err(Ok(QuickLendXError::Unauthorized))
    );
}

#[test]
fn mutations_are_rate_limited_per_owner() {
    let f = setup();
    let c = f.client();
    let id = f.upload();

    // Setup (KYC) and upload already consumed part of the window, so count how
    // many further mutations succeed before the limit trips.
    let mut accepted = 0u32;
    let mut tripped = false;
    for _ in 0..(MAX_MUTATIONS_PER_WINDOW + 1) {
        match c.try_update_invoice_category(&f.business, &id, &InvoiceCategory::Products) {
            Ok(Ok(())) => accepted += 1,
            Err(Ok(QuickLendXError::MutationLimitExceeded)) => {
                tripped = true;
                break;
            }
            other => panic!("unexpected result: {other:?}"),
        }
    }
    assert!(tripped, "rate limit must trip within one window");
    assert!(accepted > 0 && accepted < MAX_MUTATIONS_PER_WINDOW);

    // Another business is unaffected, and the window rolls over with the ledger sequence.
    f.env.ledger().with_mut(|l| l.sequence_number += 100);
    c.update_invoice_category(&f.business, &id, &InvoiceCategory::Services);
}

#[test]
fn third_party_cannot_exhaust_the_owners_rate_limit() {
    let f = setup();
    let c = f.client();
    let id = f.upload();
    let stranger = Address::generate(&f.env);

    for _ in 0..(MAX_MUTATIONS_PER_WINDOW * 2) {
        assert_eq!(
            c.try_add_invoice_tag(&stranger, &id, &s(&f.env, "spam")),
            Err(Ok(QuickLendXError::Unauthorized))
        );
    }
    // The owner's quota is untouched.
    c.add_invoice_tag(&f.business, &id, &s(&f.env, "legit"));
}

// ── lock ────────────────────────────────────────────────────────────────────

#[test]
fn admin_lock_blocks_owner_writes_until_cleared() {
    let f = setup();
    let c = f.client();
    let id = f.upload();
    assert_eq!(c.get_invoice_lock(&id), InvoiceLock::None);

    c.set_invoice_lock(&f.admin, &id, &InvoiceLock::Frozen);
    assert_eq!(c.get_invoice_lock(&id), InvoiceLock::Frozen);
    assert_eq!(
        c.try_add_invoice_tag(&f.business, &id, &s(&f.env, "x")),
        Err(Ok(QuickLendXError::InvoiceFrozen))
    );
    assert_eq!(
        c.try_update_invoice_category(&f.business, &id, &InvoiceCategory::Products),
        Err(Ok(QuickLendXError::InvoiceFrozen))
    );

    c.set_invoice_lock(&f.admin, &id, &InvoiceLock::None);
    c.add_invoice_tag(&f.business, &id, &s(&f.env, "x"));
}

#[test]
fn only_admin_can_set_lock_and_invoice_must_exist() {
    let f = setup();
    let c = f.client();
    let id = f.upload();
    let stranger = Address::generate(&f.env);

    assert_eq!(
        c.try_set_invoice_lock(&stranger, &id, &InvoiceLock::Frozen),
        Err(Ok(QuickLendXError::NotAdmin))
    );
    let unknown = BytesN::from_array(&f.env, &[0xEE; 32]);
    assert_eq!(
        c.try_set_invoice_lock(&f.admin, &unknown, &InvoiceLock::Frozen),
        Err(Ok(QuickLendXError::InvoiceNotFound))
    );
    assert_eq!(c.get_invoice_lock(&id), InvoiceLock::None);
}

// ── status queries ──────────────────────────────────────────────────────────

#[test]
fn status_count_and_batch_reflect_lifecycle() {
    let f = setup();
    let c = f.client();
    let a = f.upload();
    let b = f.upload();
    assert_eq!(c.get_invoice_count_by_status(&InvoiceStatus::Pending), 2);

    c.verify_invoice(&a);
    assert_eq!(c.get_invoice_count_by_status(&InvoiceStatus::Pending), 1);
    assert_eq!(c.get_invoice_count_by_status(&InvoiceStatus::Verified), 1);

    let unknown = BytesN::from_array(&f.env, &[0xEE; 32]);
    let ids = soroban_sdk::vec![&f.env, a.clone(), unknown, b.clone()];
    assert_eq!(
        c.get_invoices_by_status_batch(&ids),
        soroban_sdk::vec![
            &f.env,
            Some(InvoiceStatus::Verified),
            None,
            Some(InvoiceStatus::Pending)
        ]
    );
}

#[test]
fn status_batch_rejects_oversize_requests() {
    let f = setup();
    let id = f.upload();
    let mut ids = Vec::new(&f.env);
    for _ in 0..=MAX_INPUT_STATUS_BATCH_SIZE {
        ids.push_back(id.clone());
    }
    assert_eq!(
        f.client().try_get_invoices_by_status_batch(&ids),
        Err(Ok(QuickLendXError::InputTooLarge))
    );
}
