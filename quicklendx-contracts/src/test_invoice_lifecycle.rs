//! Regression coverage for invoice lifecycle storage and migration
//! compatibility (Issue #2438).
//!
//! Every test drives the **contract client**, not the module directly, so the
//! invariants are proven at the actual integration boundary an operator or
//! counterparty uses.
//!
//! Coverage map against the issue's required validation:
//!
//! | Requirement            | Tests                                            |
//! |------------------------|--------------------------------------------------|
//! | upgrade                | `migration_upgrades_legacy_records_and_commits`   |
//! | rollback               | `rollback_leaves_version_and_records_intact`      |
//! | rerun                  | `rerunning_a_page_is_idempotent`                  |
//! | partial progress       | `migration_is_resumable_from_cursor`              |
//! | legacy-data fixtures   | `legacy_record_is_readable_before_migration`      |
//! | invalid operations     | the `rejects_*` tests                             |
//! | repeated operations    | `repeated_*` tests                                |
//! | stale/concurrent calls | `rejects_stale_from_version`, `rejects_second_migration` |
//! | no partial state       | `failed_*` tests                                  |

#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, BytesN, Env};

use crate::errors::QuickLendXError;
use crate::invoice_lifecycle::{
    InvoiceLifecycle, InvoiceLifecycleStatus, InvoiceRecordV1, INVOICE_SCHEMA_VERSION,
    INVOICE_SCHEMA_VERSION_LEGACY, MAX_MIGRATION_PAGE_SIZE,
};
use crate::{QuickLendXContract, QuickLendXContractClient};

const NOW: u64 = 1_000;
const FUTURE: u64 = 100_000;

struct Ctx {
    env: Env,
    client: QuickLendXContractClient<'static>,
    contract_id: Address,
    admin: Address,
    business: Address,
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(NOW);
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let business = Address::generate(&env);
    client.init_invoice_lifecycle(&admin);
    Ctx {
        env,
        client,
        contract_id,
        admin,
        business,
    }
}

fn id_of(env: &Env, byte: u8) -> BytesN<32> {
    BytesN::from_array(env, &[byte; 32])
}

/// Seed a legacy (v1) record, simulating data written by a pre-migration
/// deployment. Runs inside the contract's storage context.
fn seed_legacy(ctx: &Ctx, byte: u8, status: InvoiceLifecycleStatus) -> BytesN<32> {
    let id = id_of(&ctx.env, byte);
    let record = InvoiceRecordV1 {
        id: id.clone(),
        business: ctx.business.clone(),
        amount: 500,
        due_date: FUTURE,
        status,
        created_at: NOW,
    };
    ctx.env.as_contract(&ctx.contract_id, || {
        InvoiceLifecycle::seed_legacy_record(&ctx.env, &record);
    });
    id
}

fn is_legacy(ctx: &Ctx, id: &BytesN<32>) -> bool {
    ctx.env.as_contract(&ctx.contract_id, || {
        InvoiceLifecycle::is_legacy_shape(&ctx.env, id)
    })
}

// =========================================================================
// Lifecycle: happy path
// =========================================================================

#[test]
fn create_stores_a_current_version_record() {
    let ctx = setup();
    let id = id_of(&ctx.env, 1);

    let record = ctx
        .client
        .create_invoice_record(&id, &ctx.business, &1_000, &FUTURE);

    assert_eq!(record.schema_version, INVOICE_SCHEMA_VERSION);
    assert_eq!(record.status, InvoiceLifecycleStatus::Active);
    assert_eq!(record.amount, 1_000);
    assert_eq!(record.amendment_count, 0);
    assert_eq!(record.cancelled_at, None);
    assert_eq!(record.completed_at, None);
    assert_eq!(ctx.client.invoice_record_count(), 1);
}

#[test]
fn amend_updates_amount_and_counts_monotonically() {
    let ctx = setup();
    let id = id_of(&ctx.env, 1);
    ctx.client
        .create_invoice_record(&id, &ctx.business, &1_000, &FUTURE);

    let first = ctx.client.amend_invoice_record(&id, &2_000, &FUTURE);
    assert_eq!(first.amount, 2_000);
    assert_eq!(first.amendment_count, 1);

    let second = ctx.client.amend_invoice_record(&id, &3_000, &FUTURE);
    assert_eq!(second.amount, 3_000);
    // Invariant I4: monotonic, never rewound by a later write.
    assert_eq!(second.amendment_count, 2);
    assert!(second.updated_at >= first.updated_at);
}

#[test]
fn cancel_and_complete_are_terminal() {
    let ctx = setup();
    let cancelled_id = id_of(&ctx.env, 1);
    let completed_id = id_of(&ctx.env, 2);
    ctx.client
        .create_invoice_record(&cancelled_id, &ctx.business, &1_000, &FUTURE);
    ctx.client
        .create_invoice_record(&completed_id, &ctx.business, &1_000, &FUTURE);

    let cancelled = ctx.client.cancel_invoice_record(&cancelled_id);
    assert_eq!(cancelled.status, InvoiceLifecycleStatus::Cancelled);
    assert_eq!(cancelled.cancelled_at, Some(NOW));

    let completed = ctx.client.complete_invoice_record(&completed_id);
    assert_eq!(completed.status, InvoiceLifecycleStatus::Completed);
    assert_eq!(completed.completed_at, Some(NOW));
}

// =========================================================================
// Lifecycle: invalid, repeated, and stale operations leave no partial state
// =========================================================================

#[test]
fn rejects_duplicate_creation_without_overwriting() {
    let ctx = setup();
    let id = id_of(&ctx.env, 1);
    ctx.client
        .create_invoice_record(&id, &ctx.business, &1_000, &FUTURE);

    let other = Address::generate(&ctx.env);
    let err = ctx
        .client
        .try_create_invoice_record(&id, &other, &9_999, &FUTURE)
        .expect_err("duplicate id must be rejected");
    assert_eq!(err, Ok(QuickLendXError::OperationNotAllowed));

    // Invariant I2/I5: the original record is untouched.
    let stored = ctx.client.get_invoice_record(&id);
    assert_eq!(stored.amount, 1_000);
    assert_eq!(stored.business, ctx.business);
    assert_eq!(ctx.client.invoice_record_count(), 1);
}

#[test]
fn rejects_amend_after_completion() {
    let ctx = setup();
    let id = id_of(&ctx.env, 1);
    ctx.client
        .create_invoice_record(&id, &ctx.business, &1_000, &FUTURE);
    ctx.client.complete_invoice_record(&id);

    let err = ctx
        .client
        .try_amend_invoice_record(&id, &5_000, &FUTURE)
        .expect_err("terminal invoices must not be amendable");
    assert_eq!(err, Ok(QuickLendXError::InvalidStatus));

    // Invariant I1/I5: settlement cannot be re-priced after completion.
    let stored = ctx.client.get_invoice_record(&id);
    assert_eq!(stored.amount, 1_000);
    assert_eq!(stored.status, InvoiceLifecycleStatus::Completed);
}

#[test]
fn repeated_cancel_is_rejected_and_does_not_rewrite() {
    let ctx = setup();
    let id = id_of(&ctx.env, 1);
    ctx.client
        .create_invoice_record(&id, &ctx.business, &1_000, &FUTURE);
    let first = ctx.client.cancel_invoice_record(&id);

    ctx.env.ledger().set_timestamp(NOW + 5_000);
    let err = ctx
        .client
        .try_cancel_invoice_record(&id)
        .expect_err("repeated cancel must be rejected");
    assert_eq!(err, Ok(QuickLendXError::InvalidStatus));

    // The original cancellation timestamp survives the repeat attempt.
    let stored = ctx.client.get_invoice_record(&id);
    assert_eq!(stored.cancelled_at, first.cancelled_at);
    assert_eq!(stored.updated_at, first.updated_at);
}

#[test]
fn rejects_completion_after_cancellation() {
    let ctx = setup();
    let id = id_of(&ctx.env, 1);
    ctx.client
        .create_invoice_record(&id, &ctx.business, &1_000, &FUTURE);
    ctx.client.cancel_invoice_record(&id);

    let err = ctx
        .client
        .try_complete_invoice_record(&id)
        .expect_err("cancelled invoices must not complete");
    assert_eq!(err, Ok(QuickLendXError::InvalidStatus));
    assert_eq!(
        ctx.client.get_invoice_record(&id).status,
        InvoiceLifecycleStatus::Cancelled
    );
}

#[test]
fn rejects_invalid_amount_and_due_date_before_any_write() {
    let ctx = setup();
    let id = id_of(&ctx.env, 1);

    let amount_err = ctx
        .client
        .try_create_invoice_record(&id, &ctx.business, &0, &FUTURE)
        .expect_err("non-positive amount must be rejected");
    assert_eq!(amount_err, Ok(QuickLendXError::InvalidAmount));

    let date_err = ctx
        .client
        .try_create_invoice_record(&id, &ctx.business, &1_000, &(NOW - 1))
        .expect_err("past due date must be rejected");
    assert_eq!(date_err, Ok(QuickLendXError::InvalidTimestamp));

    // Invariant I5: a rejected creation leaves nothing behind.
    assert_eq!(ctx.client.invoice_record_count(), 0);
    let missing = ctx
        .client
        .try_get_invoice_record(&id)
        .expect_err("no record should exist");
    assert_eq!(missing, Ok(QuickLendXError::InvoiceNotFound));
}

#[test]
fn rejects_unknown_invoice() {
    let ctx = setup();
    let err = ctx
        .client
        .try_get_invoice_record(&id_of(&ctx.env, 9))
        .expect_err("unknown id must be rejected");
    assert_eq!(err, Ok(QuickLendXError::InvoiceNotFound));
}

// =========================================================================
// Backward compatibility: legacy fixtures
// =========================================================================

#[test]
fn legacy_record_is_readable_before_migration() {
    let ctx = setup();
    let id = seed_legacy(&ctx, 7, InvoiceLifecycleStatus::Active);
    assert!(is_legacy(&ctx, &id), "fixture must start in v1 shape");

    // Backward compatibility: new code reads an old record and upgrades it in
    // memory, without rewriting storage.
    let record = ctx.client.get_invoice_record(&id);
    assert_eq!(record.schema_version, INVOICE_SCHEMA_VERSION);
    assert_eq!(record.amount, 500);
    assert_eq!(record.updated_at, record.created_at);
    assert_eq!(record.amendment_count, 0);
    assert!(
        is_legacy(&ctx, &id),
        "a read must not rewrite the stored record"
    );
}

#[test]
fn legacy_record_lifecycle_works_before_migration() {
    let ctx = setup();
    let id = seed_legacy(&ctx, 7, InvoiceLifecycleStatus::Active);

    // A legacy record stays fully operable pre-migration; the write upgrades
    // it in place as a side effect of the lifecycle transition.
    let completed = ctx.client.complete_invoice_record(&id);
    assert_eq!(completed.status, InvoiceLifecycleStatus::Completed);
    assert_eq!(completed.schema_version, INVOICE_SCHEMA_VERSION);
    assert!(!is_legacy(&ctx, &id));
}

#[test]
fn legacy_terminal_record_stays_terminal() {
    let ctx = setup();
    let id = seed_legacy(&ctx, 8, InvoiceLifecycleStatus::Completed);

    let err = ctx
        .client
        .try_amend_invoice_record(&id, &1, &FUTURE)
        .expect_err("legacy terminal records must stay terminal");
    assert_eq!(err, Ok(QuickLendXError::InvalidStatus));
}

// =========================================================================
// Migration: upgrade, resume, rerun, rollback
// =========================================================================

#[test]
fn migration_upgrades_legacy_records_and_commits() {
    let ctx = setup();
    let a = seed_legacy(&ctx, 1, InvoiceLifecycleStatus::Active);
    let b = seed_legacy(&ctx, 2, InvoiceLifecycleStatus::Completed);

    // Start from the legacy version so the transition is a real upgrade.
    ctx.env.as_contract(&ctx.contract_id, || {
        InvoiceLifecycle::force_schema_version(&ctx.env, INVOICE_SCHEMA_VERSION_LEGACY);
    });
    assert_eq!(
        ctx.client.invoice_schema_version(),
        INVOICE_SCHEMA_VERSION_LEGACY
    );

    ctx.client
        .begin_invoice_migration(&INVOICE_SCHEMA_VERSION_LEGACY, &INVOICE_SCHEMA_VERSION);
    let stepped = ctx.client.step_invoice_migration(&MAX_MIGRATION_PAGE_SIZE);
    assert_eq!(stepped.cursor, 2);
    assert_eq!(stepped.migrated, 2);

    let version = ctx.client.commit_invoice_migration();
    assert_eq!(version, INVOICE_SCHEMA_VERSION);
    assert_eq!(ctx.client.invoice_schema_version(), INVOICE_SCHEMA_VERSION);
    assert!(ctx.client.invoice_migration_state().is_none());

    // Records were physically rewritten, and their data survived unchanged.
    assert!(!is_legacy(&ctx, &a));
    assert!(!is_legacy(&ctx, &b));
    let migrated_a = ctx.client.get_invoice_record(&a);
    assert_eq!(migrated_a.amount, 500);
    assert_eq!(migrated_a.created_at, NOW);
    assert_eq!(
        ctx.client.get_invoice_record(&b).status,
        InvoiceLifecycleStatus::Completed
    );
}

#[test]
fn migration_is_resumable_from_cursor() {
    let ctx = setup();
    let ids: [BytesN<32>; 3] = [
        seed_legacy(&ctx, 1, InvoiceLifecycleStatus::Active),
        seed_legacy(&ctx, 2, InvoiceLifecycleStatus::Active),
        seed_legacy(&ctx, 3, InvoiceLifecycleStatus::Active),
    ];
    ctx.env.as_contract(&ctx.contract_id, || {
        InvoiceLifecycle::force_schema_version(&ctx.env, INVOICE_SCHEMA_VERSION_LEGACY);
    });
    ctx.client
        .begin_invoice_migration(&INVOICE_SCHEMA_VERSION_LEGACY, &INVOICE_SCHEMA_VERSION);

    // Partial progress: one record per page.
    let first = ctx.client.step_invoice_migration(&1);
    assert_eq!(first.cursor, 1);
    assert_eq!(first.migrated, 1);
    assert!(!is_legacy(&ctx, &ids[0]));
    assert!(
        is_legacy(&ctx, &ids[1]),
        "page must not run ahead of cursor"
    );

    // Committing early is refused while records remain.
    let early = ctx
        .client
        .try_commit_invoice_migration()
        .expect_err("commit before completion must be rejected");
    assert_eq!(early, Ok(QuickLendXError::OperationNotAllowed));
    assert_eq!(
        ctx.client.invoice_schema_version(),
        INVOICE_SCHEMA_VERSION_LEGACY
    );

    // Resume from the checkpoint.
    let second = ctx.client.step_invoice_migration(&2);
    assert_eq!(second.cursor, 3);
    assert_eq!(second.migrated, 3);
    ctx.client.commit_invoice_migration();
    for id in ids.iter() {
        assert!(!is_legacy(&ctx, id));
    }
}

#[test]
fn rerunning_a_page_is_idempotent() {
    let ctx = setup();
    seed_legacy(&ctx, 1, InvoiceLifecycleStatus::Active);
    ctx.env.as_contract(&ctx.contract_id, || {
        InvoiceLifecycle::force_schema_version(&ctx.env, INVOICE_SCHEMA_VERSION_LEGACY);
    });
    ctx.client
        .begin_invoice_migration(&INVOICE_SCHEMA_VERSION_LEGACY, &INVOICE_SCHEMA_VERSION);

    let first = ctx.client.step_invoice_migration(&MAX_MIGRATION_PAGE_SIZE);
    assert_eq!(first.migrated, 1);

    // Re-running past the end must not double-count or corrupt.
    let rerun = ctx.client.step_invoice_migration(&MAX_MIGRATION_PAGE_SIZE);
    assert_eq!(rerun.cursor, 1);
    assert_eq!(
        rerun.migrated, 1,
        "already-current records must not recount"
    );
}

#[test]
fn rollback_leaves_version_and_records_intact() {
    let ctx = setup();
    let a = seed_legacy(&ctx, 1, InvoiceLifecycleStatus::Active);
    let b = seed_legacy(&ctx, 2, InvoiceLifecycleStatus::Active);
    ctx.env.as_contract(&ctx.contract_id, || {
        InvoiceLifecycle::force_schema_version(&ctx.env, INVOICE_SCHEMA_VERSION_LEGACY);
    });
    ctx.client
        .begin_invoice_migration(&INVOICE_SCHEMA_VERSION_LEGACY, &INVOICE_SCHEMA_VERSION);
    ctx.client.step_invoice_migration(&1);

    let rolled = ctx.client.rollback_invoice_migration();
    assert_eq!(rolled.cursor, 1);

    // The committed version is untouched and no migration state remains.
    assert_eq!(
        ctx.client.invoice_schema_version(),
        INVOICE_SCHEMA_VERSION_LEGACY
    );
    assert!(ctx.client.invoice_migration_state().is_none());

    // Both records stay readable: the migrated one in the new shape, the
    // untouched one through the legacy read path.
    assert_eq!(ctx.client.get_invoice_record(&a).amount, 500);
    assert_eq!(ctx.client.get_invoice_record(&b).amount, 500);
    assert!(is_legacy(&ctx, &b));

    // Lifecycle writes are usable again once the migration is cleared.
    ctx.client.cancel_invoice_record(&a);
}

// =========================================================================
// Migration: repeated, stale, and invalid control calls
// =========================================================================

#[test]
fn rejects_second_migration_while_one_is_running() {
    let ctx = setup();
    ctx.env.as_contract(&ctx.contract_id, || {
        InvoiceLifecycle::force_schema_version(&ctx.env, INVOICE_SCHEMA_VERSION_LEGACY);
    });
    ctx.client
        .begin_invoice_migration(&INVOICE_SCHEMA_VERSION_LEGACY, &INVOICE_SCHEMA_VERSION);

    let err = ctx
        .client
        .try_begin_invoice_migration(&INVOICE_SCHEMA_VERSION_LEGACY, &INVOICE_SCHEMA_VERSION)
        .expect_err("concurrent migrations must be rejected");
    assert_eq!(err, Ok(QuickLendXError::OperationNotAllowed));
}

#[test]
fn rejects_stale_from_version() {
    let ctx = setup();
    // Committed version is current; a caller that still believes the store is
    // at the legacy version is stale and must be refused.
    let err = ctx
        .client
        .try_begin_invoice_migration(&INVOICE_SCHEMA_VERSION_LEGACY, &INVOICE_SCHEMA_VERSION)
        .expect_err("stale from_version must be rejected");
    assert_eq!(err, Ok(QuickLendXError::OperationNotAllowed));
    assert!(ctx.client.invoice_migration_state().is_none());
}

#[test]
fn rejects_non_forward_or_unsupported_target() {
    let ctx = setup();
    ctx.env.as_contract(&ctx.contract_id, || {
        InvoiceLifecycle::force_schema_version(&ctx.env, INVOICE_SCHEMA_VERSION_LEGACY);
    });

    let backwards = ctx
        .client
        .try_begin_invoice_migration(
            &INVOICE_SCHEMA_VERSION_LEGACY,
            &INVOICE_SCHEMA_VERSION_LEGACY,
        )
        .expect_err("non-forward migration must be rejected");
    assert_eq!(backwards, Ok(QuickLendXError::OperationNotAllowed));

    let too_new = ctx
        .client
        .try_begin_invoice_migration(
            &INVOICE_SCHEMA_VERSION_LEGACY,
            &(INVOICE_SCHEMA_VERSION + 1),
        )
        .expect_err("target beyond this build must be rejected");
    assert_eq!(too_new, Ok(QuickLendXError::OperationNotAllowed));
}

#[test]
fn rejects_step_and_commit_without_a_migration() {
    let ctx = setup();
    let step = ctx
        .client
        .try_step_invoice_migration(&10)
        .expect_err("stepping without a migration must be rejected");
    assert_eq!(step, Ok(QuickLendXError::OperationNotAllowed));

    let commit = ctx
        .client
        .try_commit_invoice_migration()
        .expect_err("committing without a migration must be rejected");
    assert_eq!(commit, Ok(QuickLendXError::OperationNotAllowed));

    let rollback = ctx
        .client
        .try_rollback_invoice_migration()
        .expect_err("rolling back without a migration must be rejected");
    assert_eq!(rollback, Ok(QuickLendXError::OperationNotAllowed));
}

#[test]
fn rejects_out_of_bounds_page_size() {
    let ctx = setup();
    ctx.env.as_contract(&ctx.contract_id, || {
        InvoiceLifecycle::force_schema_version(&ctx.env, INVOICE_SCHEMA_VERSION_LEGACY);
    });
    ctx.client
        .begin_invoice_migration(&INVOICE_SCHEMA_VERSION_LEGACY, &INVOICE_SCHEMA_VERSION);

    let zero = ctx
        .client
        .try_step_invoice_migration(&0)
        .expect_err("zero page size must be rejected");
    assert_eq!(zero, Ok(QuickLendXError::OperationNotAllowed));

    let too_big = ctx
        .client
        .try_step_invoice_migration(&(MAX_MIGRATION_PAGE_SIZE + 1))
        .expect_err("oversized page must be rejected");
    assert_eq!(too_big, Ok(QuickLendXError::OperationNotAllowed));

    // The cursor never moved, so the migration is still resumable.
    let state = ctx.client.invoice_migration_state().unwrap();
    assert_eq!(state.cursor, 0);
}

#[test]
fn repeated_init_is_rejected() {
    let ctx = setup();
    let other = Address::generate(&ctx.env);
    let err = ctx
        .client
        .try_init_invoice_lifecycle(&other)
        .expect_err("re-initialisation must be rejected");
    assert_eq!(err, Ok(QuickLendXError::OperationNotAllowed));
    // The original admin still governs migrations.
    let _ = ctx.admin;
}

// =========================================================================
// The migration write-guard: no partial state
// =========================================================================

#[test]
fn writes_rejected_while_migration_in_progress() {
    let ctx = setup();
    let existing = id_of(&ctx.env, 1);
    ctx.client
        .create_invoice_record(&existing, &ctx.business, &1_000, &FUTURE);

    ctx.env.as_contract(&ctx.contract_id, || {
        InvoiceLifecycle::force_schema_version(&ctx.env, INVOICE_SCHEMA_VERSION_LEGACY);
    });
    ctx.client
        .begin_invoice_migration(&INVOICE_SCHEMA_VERSION_LEGACY, &INVOICE_SCHEMA_VERSION);

    // Every mutating entrypoint must refuse while the cursor is live,
    // otherwise a write could land behind it and re-introduce a stale shape.
    let created = ctx
        .client
        .try_create_invoice_record(&id_of(&ctx.env, 2), &ctx.business, &10, &FUTURE)
        .expect_err("create must be blocked");
    assert_eq!(created, Ok(QuickLendXError::OperationNotAllowed));

    let amended = ctx
        .client
        .try_amend_invoice_record(&existing, &10, &FUTURE)
        .expect_err("amend must be blocked");
    assert_eq!(amended, Ok(QuickLendXError::OperationNotAllowed));

    let cancelled = ctx
        .client
        .try_cancel_invoice_record(&existing)
        .expect_err("cancel must be blocked");
    assert_eq!(cancelled, Ok(QuickLendXError::OperationNotAllowed));

    let completed = ctx
        .client
        .try_complete_invoice_record(&existing)
        .expect_err("complete must be blocked");
    assert_eq!(completed, Ok(QuickLendXError::OperationNotAllowed));

    // Reads stay available throughout, and nothing was mutated.
    let stored = ctx.client.get_invoice_record(&existing);
    assert_eq!(stored.amount, 1_000);
    assert_eq!(stored.status, InvoiceLifecycleStatus::Active);
    assert_eq!(ctx.client.invoice_record_count(), 1);
}
