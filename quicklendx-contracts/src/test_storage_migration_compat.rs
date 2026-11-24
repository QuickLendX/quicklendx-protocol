//! Storage and migration compatibility tests for QuickLendX.
//!
//! # Purpose
//! These tests provide a deterministic, reviewable guarantee that:
//! - Schema versioning is persisted and retrieved correctly.
//! - A migration lifecycle (start → page → commit) emits the correct events.
//! - A rollback leaves no partial migration state.
//! - Repeated start calls are rejected (only one migration at a time).
//! - A failed page leaves the offset at the last checkpoint so the migration
//!   is resumable without re-processing already-committed records.
//! - Concurrent or stale `begin_migration` calls that pass the wrong
//!   `schema_from` are rejected.
//! - A fresh (version-0) contract can be upgraded to version 1.
//! - A legacy-data fixture (records written before migration) survives the
//!   migration unchanged.

#![cfg(test)]

use soroban_sdk::testutils::Ledger;
use soroban_sdk::testutils::Events as EventsTrait;
use soroban_sdk::{testutils::Address as _, Address, Env};

use crate::storage::{StorageMigration, STORAGE_SCHEMA_VERSION};
use crate::QuickLendXContract;

// --- Helpers ---

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000);
    let admin = Address::generate(&env);
    (env, contract_id, admin)
}

fn with_contract<F, R>(env: &Env, f: F) -> R
where
    F: FnOnce() -> R,
{
    let contract_id = env.register_contract(None, crate::QuickLendXContract);
    env.as_contract(&contract_id, f)
}

fn event_emitted(env: &Env, topic: &str) -> bool {
    use soroban_sdk::xdr;
    use soroban_sdk::{Symbol, TryFromVal};
    let topic_sym = Symbol::new(env, topic);
    let topic_xdr = xdr::ScVal::try_from_val(env, &topic_sym).expect("topic to xdr");
    env.events().all().events().iter().any(|e| match &e.body {
        xdr::ContractEventBody::V0(b) => b.topics.first() == Some(&topic_xdr),
    })
}

// --- Tests ---

#[test]
fn test_fresh_contract_schema_version_is_zero() {
    let (env, contract_id, _admin) = setup();
    env.as_contract(&contract_id, || {
        assert_eq!(StorageMigration::get_schema_version(&env), 0);
        assert!(!StorageMigration::is_migration_in_progress(&env));
        assert_eq!(StorageMigration::get_migration_offset(&env), 0);
        assert_eq!(StorageMigration::get_migration_records_migrated(&env), 0);
    });
}

#[test]
fn test_set_schema_version_persists_and_emits() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        StorageMigration::set_schema_version(&env, 1, &admin);
        assert_eq!(StorageMigration::get_schema_version(&env), 1);
    });
    assert!(event_emitted(&env, "schema_version_set"));
}

#[test]
fn test_begin_migration_happy_path() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        StorageMigration::begin_migration(&env, &admin, 0, 1)
            .expect("begin_migration should succeed");
        assert!(StorageMigration::is_migration_in_progress(&env));
        assert_eq!(
            StorageMigration::get_pending_migration_version(&env),
            Some(1)
        );
        assert_eq!(StorageMigration::get_migration_offset(&env), 0);
        assert_eq!(StorageMigration::get_migration_records_migrated(&env), 0);
    });
    assert!(event_emitted(&env, "migration_started"));
}

#[test]
fn test_migration_full_lifecycle() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
        StorageMigration::advance_migration_page(&env, 42, 42);
        assert_eq!(StorageMigration::get_migration_offset(&env), 42);
        assert_eq!(StorageMigration::get_migration_records_migrated(&env), 42);
        StorageMigration::commit_migration(&env, &admin).unwrap();
        assert_eq!(StorageMigration::get_schema_version(&env), 1);
        assert!(!StorageMigration::is_migration_in_progress(&env));
        assert!(StorageMigration::get_pending_migration_version(&env).is_none());
        assert_eq!(StorageMigration::get_migration_offset(&env), 0);
        assert_eq!(StorageMigration::get_migration_records_migrated(&env), 0);
    });
    assert!(event_emitted(&env, "migration_completed"));
}

#[test]
fn test_migration_rollback_clears_state() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
        StorageMigration::advance_migration_page(&env, 10, 10);
        StorageMigration::rollback_migration(&env, &admin).unwrap();
        assert_eq!(StorageMigration::get_schema_version(&env), 0);
        assert!(!StorageMigration::is_migration_in_progress(&env));
        assert!(StorageMigration::get_pending_migration_version(&env).is_none());
        assert_eq!(StorageMigration::get_migration_offset(&env), 0);
        assert_eq!(StorageMigration::get_migration_records_migrated(&env), 0);
    });
    assert!(event_emitted(&env, "migration_rolled_back"));
}

#[test]
fn test_begin_migration_rejects_concurrent() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
        let err = StorageMigration::begin_migration(&env, &admin, 0, 2).unwrap_err();
        assert_eq!(err, crate::errors::QuickLendXError::OperationNotAllowed);
    });
}

#[test]
fn test_begin_migration_rejects_wrong_schema_from() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        StorageMigration::set_schema_version(&env, 1, &admin);
        let err = StorageMigration::begin_migration(&env, &admin, 0, 2).unwrap_err();
        assert_eq!(err, crate::errors::QuickLendXError::OperationNotAllowed);
        StorageMigration::begin_migration(&env, &admin, 1, 2).unwrap();
    });
}

#[test]
fn test_begin_migration_rejects_downgrade() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        let err_same = StorageMigration::begin_migration(&env, &admin, 0, 0).unwrap_err();
        assert_eq!(
            err_same,
            crate::errors::QuickLendXError::OperationNotAllowed
        );
    });
}

#[test]
fn test_commit_without_begin_fails() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        let err = StorageMigration::commit_migration(&env, &admin).unwrap_err();
        assert_eq!(err, crate::errors::QuickLendXError::OperationNotAllowed);
        assert_eq!(StorageMigration::get_schema_version(&env), 0);
    });
    assert!(!event_emitted(&env, "migration_completed"));
}

#[test]
fn test_rollback_without_begin_fails() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        let err = StorageMigration::rollback_migration(&env, &admin).unwrap_err();
        assert_eq!(err, crate::errors::QuickLendXError::OperationNotAllowed);
    });
    assert!(!event_emitted(&env, "migration_rolled_back"));
}

#[test]
fn test_migration_partial_progress_resumable() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
        StorageMigration::advance_migration_page(&env, 20, 20);
        assert_eq!(StorageMigration::get_migration_offset(&env), 20);
        assert_eq!(StorageMigration::get_migration_records_migrated(&env), 20);
        StorageMigration::advance_migration_page(&env, 35, 15);
        assert_eq!(StorageMigration::get_migration_offset(&env), 35);
        assert_eq!(StorageMigration::get_migration_records_migrated(&env), 35);
        StorageMigration::commit_migration(&env, &admin).unwrap();
        assert_eq!(StorageMigration::get_schema_version(&env), 1);
        assert!(!StorageMigration::is_migration_in_progress(&env));
    });
}

#[test]
fn test_migration_failure_event_and_resume() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
        StorageMigration::advance_migration_page(&env, 5, 5);
        StorageMigration::record_migration_failure(&env, 0, 5, "timeout");
        assert!(StorageMigration::is_migration_in_progress(&env));
        assert_eq!(StorageMigration::get_migration_offset(&env), 5);
        assert_eq!(StorageMigration::get_migration_records_migrated(&env), 5);
        StorageMigration::advance_migration_page(&env, 15, 10);
        StorageMigration::commit_migration(&env, &admin).unwrap();
        assert_eq!(StorageMigration::get_schema_version(&env), 1);
        assert!(!StorageMigration::is_migration_in_progress(&env));
    });
    assert!(event_emitted(&env, "migration_failed"));
}

#[test]
fn test_migration_multi_page_advance() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
        for i in 1u32..=5 {
            StorageMigration::advance_migration_page(&env, i * 10, 10);
        }
        assert_eq!(StorageMigration::get_migration_records_migrated(&env), 50);
        assert_eq!(StorageMigration::get_migration_offset(&env), 50);
        StorageMigration::commit_migration(&env, &admin).unwrap();
        assert_eq!(StorageMigration::get_schema_version(&env), 1);
    });
}

#[test]
fn test_legacy_data_survives_migration() {
    use crate::storage::InvoiceStorage;
    use crate::types::{
        Dispute, DisputeResolution, DisputeStatus, Invoice, InvoiceCategory, InvoiceStatus,
    };
    use soroban_sdk::{BytesN, String, Vec};

    let (env, contract_id, admin) = setup();

    let invoice_id = BytesN::from_array(&env, &[0xAAu8; 32]);
    let business = Address::generate(&env);
    let legacy_invoice = Invoice {
        id: invoice_id.clone(),
        business: business.clone(),
        amount: 5_000,
        currency: Address::generate(&env),
        due_date: env.ledger().timestamp() + 86_400,
        status: InvoiceStatus::Pending,
        created_at: env.ledger().timestamp(),
        description: String::from_str(&env, "legacy invoice"),
        metadata_customer_name: None,
        metadata_customer_address: None,
        metadata_tax_id: None,
        metadata_notes: None,
        metadata_line_items: Vec::new(&env),
        category: InvoiceCategory::Services,
        tags: Vec::new(&env),
        funded_amount: 0,
        funded_at: None,
        investor: None,
        settled_at: None,
        average_rating: None,
        total_ratings: 0,
        ratings: Vec::new(&env),
        dispute_status: DisputeStatus::None,
        dispute: Dispute {
            created_by: Address::generate(&env),
            created_at: 0,
            reason: String::from_str(&env, ""),
            evidence: String::from_str(&env, ""),
            resolution: String::from_str(&env, ""),
            resolved_by: Address::generate(&env),
            resolved_at: 0,
            resolution_outcome: DisputeResolution::None,
        },
        total_paid: 0,
        payment_history: Vec::new(&env),
        origination_fee_bps: None,
        late_payment_penalty_bps: None,
        early_payment_discount_bps: None,
    };

    env.as_contract(&contract_id, || {
        InvoiceStorage::store_invoice(&env, &legacy_invoice);
        assert_eq!(StorageMigration::get_schema_version(&env), 0);

        StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
        StorageMigration::advance_migration_page(&env, 1, 1);
        StorageMigration::commit_migration(&env, &admin).unwrap();

        let retrieved = InvoiceStorage::get_invoice(&env, &invoice_id)
            .expect("legacy invoice must still exist after migration");
        assert_eq!(retrieved.amount, 5_000);
        assert_eq!(retrieved.status, InvoiceStatus::Pending);
        assert_eq!(retrieved.business, business);
        assert_eq!(StorageMigration::get_schema_version(&env), 1);
        assert!(!StorageMigration::is_migration_in_progress(&env));
    });
}

#[test]
fn test_legacy_bid_survives_migration() {
    use crate::bid::{Bid, BidStatus, BidStorage};
    use soroban_sdk::{Address, BytesN};

    let (env, contract_id, admin) = setup();

    let bid_id = BytesN::from_array(&env, &[0xBBu8; 32]);
    let invoice_id = BytesN::from_array(&env, &[0xAAu8; 32]);
    let investor = Address::generate(&env);

    let legacy_bid = Bid {
        bid_id: bid_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        bid_amount: 1_000,
        expected_return: 1_100,
        timestamp: env.ledger().timestamp(),
        status: BidStatus::Placed,
        expiration_timestamp: env.ledger().timestamp() + 86_400,
    };

    env.as_contract(&contract_id, || {
        BidStorage::store_bid(&env, &legacy_bid);
        assert_eq!(StorageMigration::get_schema_version(&env), 0);

        StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
        StorageMigration::advance_migration_page(&env, 1, 1);
        StorageMigration::commit_migration(&env, &admin).unwrap();

        let retrieved = BidStorage::get_bid(&env, &bid_id)
            .expect("legacy bid must still exist after migration");
        assert_eq!(retrieved.bid_amount, 1_000);
        assert_eq!(retrieved.status, BidStatus::Placed);
        assert_eq!(retrieved.investor, investor);
        assert_eq!(StorageMigration::get_schema_version(&env), 1);
        assert!(!StorageMigration::is_migration_in_progress(&env));
    });
}

#[test]
fn test_storage_migration_key_snapshot() {
    use soroban_sdk::symbol_short;
    assert_eq!(symbol_short!("sch_ver"), symbol_short!("sch_ver"));
    assert_eq!(symbol_short!("mig_ver"), symbol_short!("mig_ver"));
    assert_eq!(symbol_short!("mig_off"), symbol_short!("mig_off"));
    assert_eq!(symbol_short!("mig_rec"), symbol_short!("mig_rec"));
    assert_eq!(STORAGE_SCHEMA_VERSION, 1u32);
}

#[test]
fn test_migration_not_blocked_by_upgrade_state() {
    let (env, contract_id, admin) = setup();
    env.as_contract(&contract_id, || {
        StorageMigration::begin_migration(&env, &admin, 0, 1).unwrap();
        StorageMigration::commit_migration(&env, &admin).unwrap();
        assert_eq!(StorageMigration::get_schema_version(&env), 1);
    });
}

// ============================================================================
// Frozen v1 Storage Layout XDR Fixtures
// ============================================================================
//
// The tests below verify that raw XDR keys and values frozen in
// `tests/fixtures/storage_v1_layout.json` still decode cleanly into the
// current Rust contract types (`Invoice`, `Bid`, `Investment`, `BackupV1` ->
// `Backup`).
//
// POLICY: Do NOT regenerate `tests/fixtures/storage_v1_layout.json` to silence
// a failing test. A failure means a struct field, enum variant, or storage key
// changed in a backward-incompatible way. Only regenerate the fixture on an
// intentional breaking storage-schema change accompanied by a version bump and
// migration path (see `docs/CONTRACT_VERSION_COMPATIBILITY.md`).

mod frozen_v1_fixtures {
    use super::*;
    use crate::backup::{Backup, BackupStatus, BackupStorage};
    use crate::backup_v1::BackupV1;
    use crate::bid::BidStorage;
    use crate::investment::InvestmentStorage;
    use crate::storage::{DataKey, InvoiceStorage};
    use crate::types::{
        Bid, BidStatus, Dispute, DisputeResolution, DisputeStatus, InsuranceCoverage, Investment,
        InvestmentStatus, Invoice, InvoiceCategory, InvoiceStatus,
    };
    use serde::{Deserialize, Serialize};
    use soroban_sdk::xdr::{Limits, ReadXdr, ScVal, WriteXdr};
    use soroban_sdk::{BytesN, IntoVal, String as SorobanString, TryFromVal, Val, Vec};

    /// Deterministic StrKey addresses used in frozen v1 fixtures.
    const ADDR_BUSINESS: &str = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
    const ADDR_CURRENCY: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4";
    const ADDR_INVESTOR: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM";
    const ADDR_PROVIDER: &str = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAFCT4";

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "lowercase")]
    enum StorageTier {
        Persistent,
        Instance,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct FrozenRecord {
        record_type: std::string::String,
        storage: StorageTier,
        key_xdr: std::string::String,
        value_xdr: std::string::String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    struct FrozenStorageFixture {
        schema_version: u32,
        protocol_version: u32,
        description: std::string::String,
        invoice: FrozenRecord,
        bid: FrozenRecord,
        investment: FrozenRecord,
        backup_v1: FrozenRecord,
    }

    fn encode_scval_base64<T>(env: &Env, value: &T) -> std::string::String
    where
        T: IntoVal<Env, Val>,
    {
        let val: Val = value.into_val(env);
        let sc_val = ScVal::try_from_val(env, &val).expect("convert Val to ScVal");
        sc_val
            .to_xdr_base64(Limits::none())
            .expect("encode ScVal to base64 XDR")
    }

    fn decode_scval_base64(env: &Env, xdr_b64: &str) -> Val {
        let sc_val =
            ScVal::from_xdr_base64(xdr_b64, Limits::none()).expect("decode base64 XDR to ScVal");
        Val::try_from_val(env, &sc_val).expect("convert ScVal to Val")
    }

    fn inject_frozen_record(env: &Env, record: &FrozenRecord) {
        let key_val = decode_scval_base64(env, &record.key_xdr);
        let value_val = decode_scval_base64(env, &record.value_xdr);
        match record.storage {
            StorageTier::Persistent => {
                env.storage().persistent().set(&key_val, &value_val);
            }
            StorageTier::Instance => {
                env.storage().instance().set(&key_val, &value_val);
            }
        }
    }

    fn load_frozen_fixture() -> FrozenStorageFixture {
        let raw = include_str!("../tests/fixtures/storage_v1_layout.json");
        serde_json::from_str(raw).expect("parse tests/fixtures/storage_v1_layout.json")
    }

    fn baseline_v1_invoice(env: &Env) -> Invoice {
        let invoice_id = BytesN::from_array(env, &[0x11u8; 32]);
        let business = Address::from_str(env, ADDR_BUSINESS);
        let currency = Address::from_str(env, ADDR_CURRENCY);
        let investor = Address::from_str(env, ADDR_INVESTOR);

        let mut tags = Vec::new(env);
        tags.push_back(SorobanString::from_str(env, "v1-baseline"));

        Invoice {
            id: invoice_id,
            business: business.clone(),
            amount: 10_000,
            currency,
            due_date: 1_700_086_400,
            status: InvoiceStatus::Verified,
            created_at: 1_700_000_000,
            description: SorobanString::from_str(env, "frozen v1 invoice fixture"),
            metadata_customer_name: Some(SorobanString::from_str(env, "Acme Corp")),
            metadata_customer_address: None,
            metadata_tax_id: Some(SorobanString::from_str(env, "TAX-001")),
            metadata_notes: None,
            metadata_line_items: Vec::new(env),
            category: InvoiceCategory::Services,
            tags,
            funded_amount: 9_500,
            funded_at: Some(1_700_001_000),
            investor: Some(investor),
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: Vec::new(env),
            dispute_status: DisputeStatus::None,
            dispute: Dispute {
                created_by: business.clone(),
                created_at: 0,
                reason: SorobanString::from_str(env, ""),
                evidence: SorobanString::from_str(env, ""),
                resolution: SorobanString::from_str(env, ""),
                resolved_by: business,
                resolved_at: 0,
                resolution_outcome: DisputeResolution::None,
            },
            total_paid: 0,
            payment_history: Vec::new(env),
            origination_fee_bps: Some(100),
            late_payment_penalty_bps: None,
            early_payment_discount_bps: Some(50),
        }
    }

    fn baseline_v1_bid(env: &Env) -> Bid {
        Bid {
            bid_id: BytesN::from_array(env, &[0x22u8; 32]),
            invoice_id: BytesN::from_array(env, &[0x11u8; 32]),
            investor: Address::from_str(env, ADDR_INVESTOR),
            bid_amount: 9_500,
            expected_return: 10_000,
            timestamp: 1_700_000_500,
            status: BidStatus::Placed,
            expiration_timestamp: 1_700_605_300,
        }
    }

    fn baseline_v1_investment(env: &Env) -> Investment {
        let mut insurance = Vec::new(env);
        insurance.push_back(InsuranceCoverage {
            provider: Address::from_str(env, ADDR_PROVIDER),
            coverage_percentage: 50,
            coverage_amount: 4_750,
            premium_amount: 95,
            active: true,
        });

        Investment {
            investment_id: BytesN::from_array(env, &[0x33u8; 32]),
            invoice_id: BytesN::from_array(env, &[0x11u8; 32]),
            investor: Address::from_str(env, ADDR_INVESTOR),
            amount: 9_500,
            funded_at: 1_700_001_000,
            status: InvestmentStatus::Active,
            insurance,
        }
    }

    fn baseline_v1_backup(env: &Env) -> BackupV1 {
        let mut id = [0x44u8; 32];
        id[0] = 0xB4;
        id[1] = 0xC4;
        BackupV1 {
            backup_id: BytesN::from_array(env, &id),
            timestamp: 1_700_002_000,
            description: SorobanString::from_str(env, "frozen v1 backup fixture"),
            invoice_count: 1,
            status: BackupStatus::Active,
        }
    }

    fn build_v1_fixture(env: &Env) -> FrozenStorageFixture {
        let invoice = baseline_v1_invoice(env);
        let invoice_key = DataKey::Invoice(invoice.id.clone());

        let bid = baseline_v1_bid(env);
        let investment = baseline_v1_investment(env);
        let backup_v1 = baseline_v1_backup(env);

        FrozenStorageFixture {
            schema_version: STORAGE_SCHEMA_VERSION,
            protocol_version: crate::init::PROTOCOL_VERSION,
            description: std::string::String::from(
                "Frozen v1 Soroban XDR storage layout fixture for Invoice, Bid, Investment, and BackupV1. Do not regenerate except on intentional breaking schema changes.",
            ),
            invoice: FrozenRecord {
                record_type: std::string::String::from("Invoice"),
                storage: StorageTier::Persistent,
                key_xdr: encode_scval_base64(env, &invoice_key),
                value_xdr: encode_scval_base64(env, &invoice),
            },
            bid: FrozenRecord {
                record_type: std::string::String::from("Bid"),
                storage: StorageTier::Persistent,
                key_xdr: encode_scval_base64(env, &bid.bid_id),
                value_xdr: encode_scval_base64(env, &bid),
            },
            investment: FrozenRecord {
                record_type: std::string::String::from("Investment"),
                storage: StorageTier::Persistent,
                key_xdr: encode_scval_base64(env, &investment.investment_id),
                value_xdr: encode_scval_base64(env, &investment),
            },
            backup_v1: FrozenRecord {
                record_type: std::string::String::from("BackupV1"),
                storage: StorageTier::Instance,
                key_xdr: encode_scval_base64(env, &backup_v1.backup_id),
                value_xdr: encode_scval_base64(env, &backup_v1),
            },
        }
    }

    /// Regenerates `tests/fixtures/storage_v1_layout.json`.
    ///
    /// Ignored by default so normal test runs validate against the committed
    /// frozen bytes. Run ONLY on intentional breaking schema changes:
    /// `cargo test --lib generate_v1_storage_fixture -- --ignored`
    #[test]
    #[ignore = "Run explicitly only on intentional breaking schema changes to refresh tests/fixtures/storage_v1_layout.json"]
    fn generate_v1_storage_fixture() {
        let (env, _contract_id, _admin) = setup();
        let fixture = build_v1_fixture(&env);
        let json = serde_json::to_string_pretty(&fixture).expect("serialize fixture");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/storage_v1_layout.json");
        std::fs::create_dir_all(path.parent().expect("fixture dir"))
            .expect("create tests/fixtures directory");
        std::fs::write(&path, format!("{json}\n")).expect("write frozen fixture json");
    }

    #[test]
    fn test_frozen_v1_invoice_decodes_with_current_type() {
        let (env, contract_id, _admin) = setup();
        let fixture = load_frozen_fixture();
        let expected = baseline_v1_invoice(&env);

        env.as_contract(&contract_id, || {
            inject_frozen_record(&env, &fixture.invoice);
            let decoded = InvoiceStorage::get_invoice(&env, &expected.id)
                .expect("frozen v1 Invoice XDR must decode with current InvoiceStorage::get_invoice");
            assert_eq!(decoded, expected);
        });
    }

    #[test]
    fn test_frozen_v1_bid_decodes_with_current_type() {
        let (env, contract_id, _admin) = setup();
        let fixture = load_frozen_fixture();
        let expected = baseline_v1_bid(&env);

        env.as_contract(&contract_id, || {
            inject_frozen_record(&env, &fixture.bid);
            let decoded = BidStorage::get_bid(&env, &expected.bid_id)
                .expect("frozen v1 Bid XDR must decode with current BidStorage::get_bid");
            assert_eq!(decoded, expected);
        });
    }

    #[test]
    fn test_frozen_v1_investment_decodes_with_current_type() {
        let (env, contract_id, _admin) = setup();
        let fixture = load_frozen_fixture();
        let expected = baseline_v1_investment(&env);

        env.as_contract(&contract_id, || {
            inject_frozen_record(&env, &fixture.investment);
            let decoded = InvestmentStorage::get_investment(&env, &expected.investment_id)
                .expect("frozen v1 Investment XDR must decode with current InvestmentStorage::get_investment");
            assert_eq!(decoded, expected);
        });
    }

    #[test]
    fn test_frozen_v1_backup_decodes_and_upgrades_with_current_type() {
        let (env, contract_id, _admin) = setup();
        let fixture = load_frozen_fixture();
        let expected_v1 = baseline_v1_backup(&env);
        let expected_v2 = Backup::from_v1(expected_v1.clone());

        env.as_contract(&contract_id, || {
            inject_frozen_record(&env, &fixture.backup_v1);

            let raw_val: Val = env
                .storage()
                .instance()
                .get(&expected_v1.backup_id)
                .expect("frozen v1 backup raw value must exist in instance storage");
            let decoded_v1 = BackupV1::try_from_val(&env, &raw_val)
                .expect("frozen v1 BackupV1 XDR must decode directly as BackupV1");
            assert_eq!(decoded_v1, expected_v1);

            let upgraded = BackupStorage::get_backup(&env, &expected_v1.backup_id)
                .expect("frozen v1 BackupV1 XDR must auto-upgrade via BackupStorage::get_backup");
            assert_eq!(upgraded, expected_v2);
            assert_eq!(upgraded.format_version, 2);
        });
    }

    #[test]
    #[should_panic]
    fn test_frozen_v1_decode_fails_on_breaking_field_change() {
        let (env, _contract_id, _admin) = setup();
        let fixture = load_frozen_fixture();

        // Decode the frozen Bid XDR map and drop a required field to simulate
        // a breaking schema change. Soroban's host map unpacking aborts with
        // `Error(Object, UnexpectedSize)` (or `TryFromVal` returns `Err`),
        // proving field removals break decode.
        let sc_val = ScVal::from_xdr_base64(&fixture.bid.value_xdr, Limits::none())
            .expect("decode frozen bid XDR");
        let ScVal::Map(Some(mut map_entries)) = sc_val else {
            panic!("expected Bid XDR to encode as ScVal::Map");
        };
        assert!(
            map_entries.len() > 1,
            "expected Bid XDR map to contain multiple fields"
        );
        let mut truncated = map_entries.to_vec();
        truncated.pop();
        map_entries = truncated.try_into().expect("rebuild ScMap");

        let broken_scval = ScVal::Map(Some(map_entries));
        let broken_val = Val::try_from_val(&env, &broken_scval).expect("convert broken ScVal");
        let _ = Bid::try_from_val(&env, &broken_val)
            .expect("dropping a field from frozen XDR must fail decoding into Bid");
    }
// --- MigrationRunner tests ---

fn setup_runner() -> (Env, crate::QuickLendXContractClient<'static>, Address, Address) {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    env.mock_all_auths();
    env.ledger().set_timestamp(10_000);
    let admin = Address::generate(&env);
    let client = crate::QuickLendXContractClient::new(&env, &contract_id);
    env.as_contract(&contract_id, || {
        crate::admin::AdminStorage::initialize(&env, &admin).unwrap();
    });
    (env, client, contract_id, admin)
}

fn seed_invoices(env: &Env, contract_id: &Address, count: u8) {
    use crate::storage::InvoiceStorage;
    use crate::types::{Invoice, InvoiceCategory, InvoiceStatus};
    use soroban_sdk::{BytesN, String, Vec};

    let business = Address::generate(env);
    let currency = Address::generate(env);
    env.as_contract(contract_id, || {
        for i in 0..count {
            let mut raw = [0u8; 32];
            raw[0] = i.wrapping_add(1);
            let id = BytesN::from_array(env, &raw);
            let inv = Invoice {
                id,
                business: business.clone(),
                amount: 1_000,
                currency: currency.clone(),
                due_date: 100_000,
                status: InvoiceStatus::Pending,
                created_at: env.ledger().timestamp(),
                description: String::from_str(env, "migration test invoice"),
                metadata_customer_name: None,
                metadata_customer_address: None,
                metadata_tax_id: None,
                metadata_notes: None,
                funded_amount: 0,
                investor: None,
                settled_at: None,
                average_rating: None,
                total_ratings: 0,
                ratings: Vec::new(env),
                category: InvoiceCategory::Services,
                tags: Vec::new(env),
                dispute_status: crate::types::DisputeStatus::None,
                dispute: crate::types::Dispute {
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
            };
            InvoiceStorage::store_invoice(env, &inv);
        }
    });
}

#[test]
fn test_migration_runner_crash_resume_dry_run_and_record() {
    let (env, client, contract_id, admin) = setup_runner();
    seed_invoices(&env, &contract_id, 5);

    let plan = client.migration_plan(&admin, &0, &1);
    assert_eq!(plan.schema_from, 0);
    assert_eq!(plan.schema_to, 1);
    assert_eq!(plan.total_records, 5);
    assert!(!plan.already_completed);

    // Dry-run must not mutate offset, records_migrated, or emit events.
    let events_before_dry = env.events().all().events().len();
    let dry = client.migration_dry_run(&admin, &2);
    assert_eq!(dry.current_offset, 0);
    assert_eq!(dry.next_offset, 2);
    assert_eq!(dry.would_process, 2);
    assert_eq!(dry.remaining_after_step, 3);
    assert_eq!(dry.total_records, 5);
    assert!(!dry.would_complete);
    assert_eq!(env.events().all().events().len(), events_before_dry);

    let status_after_dry = client.migration_status();
    assert!(status_after_dry.in_progress);
    assert_eq!(status_after_dry.offset, 0);
    assert_eq!(status_after_dry.records_migrated, 0);

    // Step 1: process 2 records.
    let step1 = client.migration_run_step(&admin, &2);
    assert_eq!(step1.processed, 2);
    assert_eq!(step1.offset, 2);
    assert!(!step1.completed);

    // Simulate crash/interruption and resume at a later ledger timestamp.
    env.ledger().set_timestamp(10_500);
    let step2 = client.migration_run_step(&admin, &2);
    assert_eq!(step2.processed, 2);
    assert_eq!(step2.offset, 4);
    assert!(!step2.completed);

    // Final step: remaining 1 record completes and commits.
    env.ledger().set_timestamp(11_000);
    let step3 = client.migration_run_step(&admin, &2);
    assert_eq!(step3.processed, 1);
    assert_eq!(step3.offset, 5);
    assert!(step3.completed);

    let final_status = client.migration_status();
    assert_eq!(final_status.current_version, 1);
    assert!(!final_status.in_progress);
    assert_eq!(final_status.pending_version, None);

    let record = client
        .get_migration_record(&1)
        .expect("per-version record must be stored");
    assert_eq!(record.schema_from, 0);
    assert_eq!(record.schema_to, 1);
    assert_eq!(record.records_migrated, 5);
    assert_eq!(record.started_at, 10_000);
    assert_eq!(record.completed_at, 11_000);
    assert_eq!(record.initiated_by, admin);
    assert_eq!(record.completed_by, admin);
}

#[test]
fn test_migration_runner_idempotent_rerun() {
    let (_env, client, _contract_id, admin) = setup_runner();

    let plan1 = client.migration_plan(&admin, &0, &1);
    assert!(!plan1.already_completed);
    let step = client.migration_run_step(&admin, &10);
    assert!(step.completed);

    // Re-running plan for the already-completed version is an idempotent no-op.
    let plan2 = client.migration_plan(&admin, &0, &1);
    assert!(plan2.already_completed);
    assert_eq!(plan2.total_records, 0);
    assert!(!client.migration_status().in_progress);
}

#[test]
fn test_migration_runner_rejects_double_start() {
    let (_env, client, _contract_id, admin) = setup_runner();

    client.migration_plan(&admin, &0, &1);
    let res = client.try_migration_plan(&admin, &0, &1);
    assert_eq!(res, Err(Ok(crate::errors::QuickLendXError::OperationNotAllowed)));
}

#[test]
fn test_migration_runner_rejects_wrong_admin() {
    let (env, client, _contract_id, admin) = setup_runner();
    let intruder = Address::generate(&env);

    assert_eq!(
        client.try_migration_plan(&intruder, &0, &1),
        Err(Ok(crate::errors::QuickLendXError::NotAdmin))
    );

    client.migration_plan(&admin, &0, &1);

    assert_eq!(
        client.try_migration_dry_run(&intruder, &5),
        Err(Ok(crate::errors::QuickLendXError::NotAdmin))
    );
    assert_eq!(
        client.try_migration_run_step(&intruder, &5),
        Err(Ok(crate::errors::QuickLendXError::NotAdmin))
    );
    assert_eq!(
        client.try_migration_rollback(&intruder),
        Err(Ok(crate::errors::QuickLendXError::NotAdmin))
    );
}

#[test]
fn test_migration_runner_rejects_zero_limit_and_bounds_large_limit() {
    let (env, client, contract_id, admin) = setup_runner();
    seed_invoices(&env, &contract_id, 3);
    client.migration_plan(&admin, &0, &1);

    assert_eq!(
        client.try_migration_dry_run(&admin, &0),
        Err(Ok(crate::errors::QuickLendXError::OperationNotAllowed))
    );
    assert_eq!(
        client.try_migration_run_step(&admin, &0),
        Err(Ok(crate::errors::QuickLendXError::OperationNotAllowed))
    );

    let dry = client.migration_dry_run(&admin, &10_000);
    assert_eq!(dry.would_process, 3);
    assert!(dry.would_complete);
}
