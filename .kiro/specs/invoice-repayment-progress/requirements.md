# Requirements Document

## Introduction

QuickLendX already computes settlement progress internally via `settlement::get_invoice_progress`,
which produces a `Progress` struct containing a `progress_percent: u32` field in the range `0..=100`.
This internal calculation is not yet surfaced as a public contract function. Frontend dashboards
therefore have no clean path to display repayment progress without fetching the full `Invoice` struct
and repeating the division themselves — which is error-prone (division by zero when `amount == 0`)
and produces inconsistent results across clients.

This feature exposes two new additive, read-only public getter functions on `QuickLendXContract`:

1. **`get_invoice_repayment_pct(invoice_id: BytesN<32>) -> u32`** — per-invoice repayment progress
   as a plain integer percentage in `0..=100`, used by invoice detail cards.
2. **`get_platform_success_rate_pct() -> u32`** — platform-wide funded-invoice success rate as a
   plain integer percentage in `0..=100`, used by the platform overview dashboard.

Both functions are infallible (return `u32`, not `Result`), write no state, require no authorization,
and degrade gracefully to `0` on every error condition so dashboards remain functional under partial
data. The shared percentage base constant `PERCENT_BASE: u32 = 100` is introduced in
`protocol_limits.rs` alongside the existing protocol constants and imported wherever used.

No existing function signatures are changed.

---

## Glossary

- **Contract**: The `QuickLendXContract` Soroban smart contract implemented in the
  `quicklendx-contracts` crate.
- **Invoice**: An on-chain record of a business's receivable, identified by a `BytesN<32>` ID, with
  fields `amount: i128` (total due), `total_paid: i128` (aggregate paid), and
  `status: InvoiceStatus`.
- **InvoiceStatus**: The enumeration of valid invoice lifecycle states —
  `Pending | Verified | Funded | Paid | Defaulted | Cancelled | Refunded`.
- **Terminal_Funded_Status**: The subset of `InvoiceStatus` values that represent invoices that were
  once funded and have since reached a terminal outcome: `Paid` or `Defaulted`.
- **Progress**: The existing `settlement::Progress` struct returned by
  `settlement::get_invoice_progress`. Its `progress_percent: u32` field already encodes a
  `0..=100` percentage with a division-by-zero guard.
- **PERCENT_BASE**: The constant `pub const PERCENT_BASE: u32 = 100` to be declared in
  `protocol_limits.rs`. It is the exclusive source of the value `100` used in percentage
  calculations across the two new functions.
- **get_invoice_repayment_pct**: The new public getter that returns the repayment percentage for a
  single invoice, delegating the core calculation to `settlement::get_invoice_progress`.
- **get_platform_success_rate_pct**: The new public getter that returns the platform-wide success
  rate derived live from `InvoiceStorage` status index counts.
- **Soroban_Env**: The `soroban_sdk::Env` passed as the first argument to every contract function;
  provides storage, ledger metadata, and crypto primitives in the `#![no_std]` environment.
- **InvoiceStorage**: The `crate::invoice::InvoiceStorage` struct whose
  `get_invoices_by_status(env, &status)` method returns a `Vec<BytesN<32>>` for each status bucket.

---

## Requirements

---

### Requirement 1: Introduce `PERCENT_BASE` Constant in `protocol_limits.rs`

**User Story:** As a contract developer, I want a single canonical constant for the percentage base
value `100`, so that every percentage computation in the contract uses the same named constant and
the value is never duplicated as a magic number.

#### Acceptance Criteria

1. THE `protocol_limits` Module SHALL declare `pub const PERCENT_BASE: u32 = 100` at module scope,
   adjacent to the other existing protocol constants.
2. WHEN `get_invoice_repayment_pct` computes or delegates a percentage result, THE Contract SHALL
   reference `PERCENT_BASE` from `protocol_limits` rather than an inline literal `100u32`.
3. WHEN `get_platform_success_rate_pct` computes the platform percentage, THE Contract SHALL
   reference `PERCENT_BASE` from `protocol_limits` rather than an inline literal `100u32`.
4. THE `protocol_limits` Module SHALL NOT define any other constant named `PERCENT_BASE` in the same
   scope, ensuring the name is unique within the module.

---

### Requirement 2: Add `get_invoice_repayment_pct` Public Getter

**User Story:** As a frontend developer building an invoice detail card, I want a single public
function that returns the repayment progress of a specific invoice as an integer `0..=100`, so that
I can display a progress bar without fetching the full `Invoice` struct or repeating division logic
that is already correct and tested inside the contract.

#### Acceptance Criteria

1. THE Contract SHALL expose a public function with the exact signature
   `pub fn get_invoice_repayment_pct(env: Env, invoice_id: BytesN<32>) -> u32`.
2. WHEN `get_invoice_repayment_pct` is called, THE Contract SHALL delegate the progress calculation
   to `settlement::get_invoice_progress(env, invoice_id)` rather than re-implementing the
   `total_paid / amount * 100` arithmetic.
3. WHEN `settlement::get_invoice_progress` returns `Ok(progress)`, THE Contract SHALL return
   `progress.progress_percent`, which is already bounded to `0..=100` by the existing
   implementation.
4. WHEN `settlement::get_invoice_progress` returns any `Err(_)` — including
   `QuickLendXError::InvoiceNotFound` for a non-existent invoice — THE Contract SHALL return `0u32`
   without panicking or propagating the error.
5. WHEN the invoice identified by `invoice_id` exists but has `total_paid == 0`, THE Contract SHALL
   return `0`.
6. WHEN the invoice identified by `invoice_id` exists and `total_paid >= amount`, THE Contract SHALL
   return `100`.
7. WHEN the invoice identified by `invoice_id` exists and `0 < total_paid < amount`, THE Contract
   SHALL return the floor-division result `(total_paid * 100) / amount`, bounded to `0..=100`.
8. WHEN the invoice identified by `invoice_id` exists and `amount == 0`, THE Contract SHALL return
   `0` (division-by-zero guard, already handled by the delegated
   `settlement::get_invoice_progress`).
9. THE `get_invoice_repayment_pct` function SHALL NOT write to any storage entry.
10. THE `get_invoice_repayment_pct` function SHALL NOT call `require_auth` on any address.
11. THE `get_invoice_repayment_pct` function SHALL NOT use any type or function from the `std`
    namespace; all dependencies SHALL come from `soroban_sdk` or existing crate modules.

---

### Requirement 3: Add `get_platform_success_rate_pct` Public Getter

**User Story:** As a frontend developer building a platform overview dashboard, I want a single
public function that returns the platform-wide invoice success rate as an integer `0..=100`, so that
I can render a summary widget without fetching the full `PlatformMetrics` struct or reproducing the
basis-point conversion that `analytics::AnalyticsCalculator` uses internally.

#### Acceptance Criteria

1. THE Contract SHALL expose a public function with the exact signature
   `pub fn get_platform_success_rate_pct(env: Env) -> u32`.
2. WHEN `get_platform_success_rate_pct` is called, THE Contract SHALL compute the result live from
   current `InvoiceStorage` status index counts, specifically:
   - `paid_count`: the number of invoices with status `InvoiceStatus::Paid`,
     obtained via `InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Paid).len()`.
   - `defaulted_count`: the number of invoices with status `InvoiceStatus::Defaulted`,
     obtained via `InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Defaulted).len()`.
3. WHEN `get_platform_success_rate_pct` computes the rate, THE Contract SHALL use the formula:
   `paid_count * PERCENT_BASE / max(paid_count + defaulted_count, 1)`, where `PERCENT_BASE` is the
   constant from `protocol_limits.rs` and all arithmetic uses overflow-safe (`saturating_*` or
   `checked_*`) operations.
4. WHEN `paid_count + defaulted_count == 0` (no terminal funded outcomes exist), THE Contract SHALL
   return `0`.
5. WHEN `defaulted_count == 0` and `paid_count > 0` (all terminal outcomes are paid), THE Contract
   SHALL return `100`.
6. WHEN `paid_count == 0` and `defaulted_count > 0` (all terminal outcomes are defaulted), THE
   Contract SHALL return `0`.
7. WHEN `paid_count == defaulted_count` and both are greater than `0` (equal paid and defaulted
   counts), THE Contract SHALL return `50`.
8. THE result of `get_platform_success_rate_pct` SHALL be in the range `0..=100` for all possible
   storage states; the implementation SHALL NOT produce a result greater than `100` or use
   basis-point arithmetic (a denominator of `10000`) for this function.
9. THE `get_platform_success_rate_pct` function SHALL NOT read cached `PlatformMetrics` from
   storage; it SHALL compute live from the `InvoiceStatus::Paid` and `InvoiceStatus::Defaulted`
   index counts only.
10. THE `get_platform_success_rate_pct` function SHALL NOT write to any storage entry.
11. THE `get_platform_success_rate_pct` function SHALL NOT call `require_auth` on any address.
12. THE `get_platform_success_rate_pct` function SHALL NOT use any type or function from the `std`
    namespace; all dependencies SHALL come from `soroban_sdk` or existing crate modules.

---

### Requirement 4: Backward Compatibility and Build Integrity

**User Story:** As a contract maintainer, I want the two new getter functions added without
modifying any existing function signature, so that deployed clients and existing tests continue to
work without any migration.

#### Acceptance Criteria

1. THE Contract SHALL NOT change the signature, return type, or behavior of any existing public
   function when this feature is implemented.
2. WHEN the contract is compiled with `cargo build --target wasm32-unknown-unknown --release`, THE
   build SHALL succeed with zero errors.
3. WHEN `cargo clippy --workspace --all-targets -- -D warnings` is executed, THE workspace SHALL
   produce zero clippy warnings that would be treated as errors.
4. WHEN `cargo test -p quicklendx-contracts` is executed, THE existing test suite SHALL pass with
   zero regressions; IF any existing test fails after the new functions are added, THEN THE
   implementation SHALL be rejected regardless of whether the new functions themselves pass their
   own tests.
5. THE new functions SHALL compile under `#![no_std]` with no references to `std::` symbols,
   consistent with the existing crate attribute.

---

### Requirement 5: Dedicated Test Coverage in `test_repayment_progress.rs`

**User Story:** As a contract developer, I want a dedicated test file covering all boundary
conditions for both new getters, so that edge-case behavior (non-existent invoice, unfunded
invoice, partial payment, full payment, division-by-zero, empty platform, all-paid, all-defaulted,
mixed) is verified using the Soroban mock `Env` exclusively.

#### Acceptance Criteria

1. THE test file `quicklendx-contracts/src/test_repayment_progress.rs` SHALL be created and SHALL
   be registered as a test module in `lib.rs` (or the appropriate crate entry point) so that
   `cargo test -p quicklendx-contracts` discovers and executes it.

2. WHEN `get_invoice_repayment_pct` is called with an invoice ID that has never been stored, THE
   test SHALL assert the return value equals `0`.

3. WHEN `get_invoice_repayment_pct` is called on a `Pending` invoice with `total_paid == 0`, THE
   test SHALL assert the return value equals `0`.

4. WHEN `get_invoice_repayment_pct` is called on a `Funded` invoice where `total_paid` equals
   exactly half of `amount`, THE test SHALL assert the return value equals `50`.

5. WHEN `get_invoice_repayment_pct` is called on a `Paid` invoice with `total_paid >= amount`, THE
   test SHALL assert the return value equals `100`.

6. IF a test invoice can be constructed with `amount == 0` in the mock environment, THEN THE test
   SHALL call `get_invoice_repayment_pct` on that invoice and assert the return value equals `0`
   (division-by-zero guard).

7. WHEN `get_platform_success_rate_pct` is called on a freshly initialized contract with no
   invoices in storage, THE test SHALL assert the return value equals `0`.

8. WHEN `get_platform_success_rate_pct` is called and all terminal invoices have status `Paid` with
   zero `Defaulted` invoices, THE test SHALL assert the return value equals `100`.

9. WHEN `get_platform_success_rate_pct` is called and all terminal invoices have status `Defaulted`
   with zero `Paid` invoices, THE test SHALL assert the return value equals `0`.

10. WHEN `get_platform_success_rate_pct` is called and exactly half of the terminal invoices are
    `Paid` and the other half are `Defaulted`, THE test SHALL assert the return value equals `50`.

11. ALL tests in `test_repayment_progress.rs` SHALL use the Soroban mock `Env`
    (`soroban_sdk::Env::default()`) exclusively; they SHALL NOT make real ledger, network, or
    external service calls.

12. THE test file SHALL follow the naming and module conventions already established by
    `test_bid.rs`, `test.rs`, and sibling test modules in the `quicklendx-contracts/src/`
    directory.

---

### Requirement 6: Documentation Update for `docs/contracts/settlement.md`

**User Story:** As a protocol integrator reading the settlement documentation, I want both new
getter functions documented with their signatures, return semantics, and the division-by-zero
safeguard explained, so that I can integrate them into a frontend without reading Rust source code.

#### Acceptance Criteria

1. THE file `docs/contracts/settlement.md` SHALL be updated to include a new top-level section
   titled `## Repayment Progress Getters`.

2. WITHIN the `## Repayment Progress Getters` section, THE documentation SHALL describe
   `get_invoice_repayment_pct` with:
   - Its full function signature as it appears in the contract.
   - A description of what the return value represents (`0..=100` integer percentage).
   - The behavior when the invoice does not exist (returns `0`).
   - The behavior when `amount == 0` (returns `0`, division-by-zero guard).
   - The behavior for partial, full, and zero-payment states.

3. WITHIN the `## Repayment Progress Getters` section, THE documentation SHALL describe
   `get_platform_success_rate_pct` with:
   - Its full function signature as it appears in the contract.
   - A description of what the return value represents (`0..=100` integer percentage of
     terminal funded invoices that are `Paid`).
   - The formula used: `paid_count * 100 / max(paid_count + defaulted_count, 1)`.
   - The behavior when no terminal funded invoices exist (returns `0`).
   - A note clarifying that the result is a direct `0..=100` percentage, NOT basis points.

4. THE updated `docs/contracts/settlement.md` SHALL preserve all existing content without removing
   or altering any section already present in the file.
