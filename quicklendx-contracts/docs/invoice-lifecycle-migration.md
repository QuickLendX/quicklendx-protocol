# Invoice Lifecycle: Storage and Migration Compatibility

> Issue #2438 (QE-2026-08) — implementation notes, invariants, and operational runbook.
>
> Implementation: [`src/invoice_lifecycle.rs`](../src/invoice_lifecycle.rs)
> Tests: [`src/test_invoice_lifecycle.rs`](../src/test_invoice_lifecycle.rs)

## 1. Design

### Lifecycle state machine

```
         create
           │
           ▼
       ┌────────┐  amend (n times, Active only)
       │ Active │◄──────────────┐
       └───┬────┘───────────────┘
    cancel │ complete
     ┌─────┴─────┐
     ▼           ▼
┌───────────┐ ┌───────────┐
│ Cancelled │ │ Completed │   terminal — no further transitions
└───────────┘ └───────────┘
```

### Invariants

| ID | Invariant | Enforced by | Proven by |
|----|-----------|-------------|-----------|
| I1 | Terminal states are final — a `Cancelled`/`Completed` invoice can never be amended, cancelled, or completed again | `status.is_terminal()` check in `amend`/`cancel`/`complete` | `rejects_amend_after_completion`, `repeated_cancel_is_rejected_and_does_not_rewrite`, `rejects_completion_after_cancellation`, `legacy_terminal_record_stays_terminal` |
| I2 | Creation is unique — an existing record is never silently overwritten | `exists()` check in `create` | `rejects_duplicate_creation_without_overwriting` |
| I3 | Only the owning business mutates its invoice | `record.business.require_auth()` against the **stored** address | all lifecycle tests |
| I4 | Monotonic bookkeeping — `updated_at` never rewinds, `amendment_count` only increases | `core::cmp::max` on `updated_at` | `amend_updates_amount_and_counts_monotonically` |
| I5 | No partial state — every precondition is validated before the first write | ordering inside each entrypoint | `rejects_invalid_amount_and_due_date_before_any_write`, `rejects_out_of_bounds_page_size` |

I1 is the financial-safety invariant the issue names: it is what prevents a
settlement landing after the underlying obligation changed.

## 2. Storage compatibility

### Record layout

`InvoiceRecordV1` (legacy, 6 fields) → `InvoiceRecord` (current, 11 fields).
v2 adds `schema_version`, `updated_at`, `amendment_count`, `cancelled_at`,
`completed_at`. **No v1 field changed meaning** — the upgrade is purely
additive, which is what makes `InvoiceRecord::from_v1` lossless and total.

### Backward compatibility (new code, old records)

`InvoiceLifecycle::load` classifies the stored map by probing for a
`schema_version` field, then decodes with the matching type and upgrades a v1
record **in memory**. A pre-migration record therefore stays readable and fully
operable before the migration runs, and a read never rewrites storage.

> **Why probe instead of speculatively decoding?** A typed `get` of the wrong
> shape does not fail softly. The host raises `Error(Object, UnexpectedSize)`
> — *"differing host map and output slice lengths when unpacking map to
> slice"* — and escalates it to a panic, so a `try current, else legacy`
> decode aborts the call rather than falling through. This was caught by the
> legacy-fixture tests during implementation.

### Forward compatibility (old code, new records)

Every stored record carries an explicit `schema_version`. A reader that
encounters a version above its own `INVOICE_SCHEMA_VERSION` refuses it with
`OperationNotAllowed` rather than silently misreading a newer layout.

**Rule for future versions:** only *add* fields, always bump
`INVOICE_SCHEMA_VERSION`, and never repurpose an existing field.

## 3. Migration

```
begin_invoice_migration(from, to)
        │
        ▼
step_invoice_migration(page_size)   ← repeat; cursor is committed per page
        │
        ├──► commit_invoice_migration()    (cursor == total → bump version)
        └──► rollback_invoice_migration()  (abandon, version untouched)
```

| Property | Mechanism | Proven by |
|----------|-----------|-----------|
| Resumable | `MigrationState.cursor` persisted after every page | `migration_is_resumable_from_cursor` |
| Observable | event per transition + `invoice_migration_state()` / `invoice_schema_version()` | all migration tests |
| Rerun-safe | already-current records are skipped, cursor still advances | `rerunning_a_page_is_idempotent` |
| Stale-safe | caller-supplied `from_version` must equal the committed version | `rejects_stale_from_version` |
| Repeat-safe | a second `begin` while one runs is rejected | `rejects_second_migration_while_one_is_running` |
| Atomic | lifecycle writes rejected while a migration is in progress | `writes_rejected_while_migration_in_progress` |
| Bounded | `page_size` ∈ (0, `MAX_MIGRATION_PAGE_SIZE`] | `rejects_out_of_bounds_page_size` |

### The write guard

While a migration is in progress every lifecycle mutation returns
`OperationNotAllowed`. Without it, a write could land on a page the cursor has
already passed, re-introducing a legacy-shaped record *behind* the cursor —
the classic partial-migration corruption. Reads stay available throughout.

### Rollback

`rollback_invoice_migration` clears migration state **without** bumping the
committed version. Records already rewritten to v2 remain readable because the
current build reads both shapes; the version is only bumped once every record
is done. That asymmetry is deliberate and is why rollback is safe without
rewriting data backwards.

### Committing early is refused

`commit_invoice_migration` fails with `OperationNotAllowed` while
`cursor < index.len()`, so the store can never declare itself migrated while
legacy records remain.

## 4. Compatibility impact

**This change is purely additive.** It introduces new entrypoints and new
storage keys. It changes no existing response shape, no existing entrypoint,
and no error code.

Error variants are **reused, not added** — `errors.rs` documents that all 50
XDR error slots are consumed and that new variants require replacing an
existing one. This module reuses `OperationNotAllowed`, `InvalidStatus`,
`InvoiceNotFound`, `InvalidAmount`, `InvalidTimestamp`, and `NotAdmin`.

## 5. Operational limitations

* Migration is operator-driven: pages are pulled by repeated
  `step_invoice_migration` calls rather than run automatically.
* `page_size` is capped at `MAX_MIGRATION_PAGE_SIZE` (50) so one invocation
  cannot exceed the resource budget; per-call cost stays flat regardless of
  total record count.
* The invoice index is an append-only `Vec<BytesN<32>>`, which bounds the
  practical record count per contract instance.
* `init_invoice_lifecycle` is one-shot; the admin cannot be rotated through
  this module.

## 6. Security assumptions

* Migration control is admin-only; every migration entrypoint calls
  `require_auth` on the admin recorded at init.
* Lifecycle mutation is business-scoped (I3) and authorised against the
  **stored** `business` address, never a caller-supplied one. Migration
  authority therefore never confers the ability to alter invoice data.
* Re-initialisation is rejected, so the admin cannot be silently replaced.

## 7. Validation

| Check | Command | Result |
|-------|---------|--------|
| Build | `cargo build -p quicklendx-contracts` | pass |
| Tests | `cargo test -p quicklendx-contracts --lib` | **40 passed, 0 failed** (23 new) |
| Format | `rustfmt --edition 2021 --check <changed files>` | clean |
| WASM contract build | `cargo build -p quicklendx-contracts --release --target wasm32v1-none` | pass |
| WASM size | release artifact | 46,106 bytes (baseline 13,017; +33,089 for the new entrypoints, records, and events) |

Events use the `#[contractevent]` macro rather than the deprecated
`env.events().publish`, so this change compiles warning-free.

`main` carries 5 pre-existing `cargo fmt` diffs in files this change does not
touch; they are neither introduced nor resolved here, and CI gates on neither
`fmt` nor `clippy`.
