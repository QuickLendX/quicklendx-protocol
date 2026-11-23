# QuickLendX Contracts — Contributor Guide

**Audience:** contributors making changes to `quicklendx-contracts/`.  
**Goal:** capture the intent and constraints that live in engineers' heads so you
can get productive without reading every commit.

---

## Table of contents

1. [Repository layout](#1-repository-layout)
2. [Build and test commands](#2-build-and-test-commands)
3. [No-std discipline](#3-no-std-discipline)
4. [Invoice lifecycle](#4-invoice-lifecycle)
5. [Bidding system](#5-bidding-system)
6. [Escrow and payments](#6-escrow-and-payments)
7. [KYC / verification gates](#7-kyc--verification-gates)
8. [Pause and emergency-withdraw circuit breakers](#8-pause-and-emergency-withdraw-circuit-breakers)
9. [Protocol limits and string caps](#9-protocol-limits-and-string-caps)
10. [Error codes — stability contract](#10-error-codes--stability-contract)
11. [Events — topic stability contract](#11-events--topic-stability-contract)
12. [Test harness authorization pattern](#12-test-harness-authorization-pattern)
13. [WASM size budget](#13-wasm-size-budget)
14. [Storage TTL and index layout](#14-storage-ttl-and-index-layout)
15. [Adding a new entry-point](#15-adding-a-new-entry-point)
16. [Checklist before opening a PR](#16-checklist-before-opening-a-pr)

---

## 1. Repository layout

```
quicklendx-contracts/
├── src/
│   ├── lib.rs          ← workspace entry-point; wires feature-gated modules
│   ├── contract.rs     ← secondary contract impl (legacy/alternate wiring)
│   ├── types.rs        ← #[contracttype] structs and enums
│   ├── errors.rs       ← QuickLendXError enum (stable numeric codes)
│   ├── events.rs       ← #[contractevent] structs and TOPIC_* constants
│   ├── invoice.rs      ← Invoice struct methods
│   ├── bid.rs          ← BidStorage, ranking, TTL
│   ├── escrow.rs       ← escrow accept / refund
│   ├── payments.rs     ← EscrowStorage, create_escrow, release_escrow
│   ├── settlement.rs   ← settle_invoice, process_partial_payment
│   ├── defaults.rs     ← overdue scan, mark_invoice_defaulted
│   ├── verification.rs ← KYC, BusinessVerification, InvestorVerification
│   ├── init.rs         ← one-time initialization, ProtocolInitializer
│   ├── protocol_limits.rs  ← ProtocolLimits, string-length constants
│   ├── admin.rs        ← AdminStorage, require_admin
│   ├── pause.rs        ← PauseControl circuit breaker
│   ├── emergency.rs    ← EmergencyWithdraw timelock
│   ├── reentrancy.rs   ← with_payment_guard
│   ├── storage.rs      ← InvoiceStorage, BidStorage, index helpers
│   ├── analytics.rs    ← PlatformMetrics, InvestorAnalytics
│   ├── audit.rs        ← AuditStorage, hash chain
│   ├── backup.rs       ← Backup, BackupRetentionPolicy
│   ├── fees.rs         ← FeeManager, revenue distribution
│   ├── invariants.rs   ← invariant_self_check
│   ├── health.rs       ← ProtocolHealth heartbeat
│   └── test_*.rs       ← unit tests (cfg(test) or legacy-tests feature)
├── tests/              ← integration tests (e2e, WASM size, etc.)
├── docs/               ← per-module reference docs
├── fuzz/               ← libFuzzer targets
└── Cargo.toml
```

The main contract implementation is in `src/lib.rs` under `#[contractimpl] impl QuickLendXContract`.
`src/contract.rs` is a legacy alternate wiring kept for reference; it is not the active entry-point.

---

## 2. Build and test commands

All commands run from `quicklendx-contracts/` unless noted.

```bash
# Regular build (native, for test runs)
cargo build

# WASM release build — what goes on-chain
cargo build --target wasm32-unknown-unknown --release
# Or, preferred (produces wasm32v1-none, smaller):
stellar contract build

# Unit + integration tests
cargo test

# Include legacy tests gated behind the feature flag
cargo test --features legacy-tests

# Fuzz / proptest tests
cargo test --features fuzz-tests fuzz_

# Lint — must pass before PR
cargo clippy --workspace --all-targets -- -D warnings

# Check WASM size budget (must be < 256 KiB)
./scripts/check-wasm-size.sh
```

---

## 3. No-std discipline

The crate begins with `#![no_std]`. **Do not introduce `std::` calls.**

| Allowed | Forbidden |
|---------|-----------|
| `soroban_sdk::{Address, BytesN, Env, Map, String, Vec}` | `std::vec::Vec`, `std::string::String`, `std::collections::*` |
| `soroban_sdk::contracttype`, `contractimpl`, `contracterror` | `println!`, `eprintln!`, `format!` |
| `core::cmp`, `core::str`, stack-allocated arrays | Any heap allocation not routed through `soroban_sdk` |
| `extern crate alloc;` + `alloc::*` where needed | `std::io`, `std::fs`, `std::thread` |

Logging uses the `crate::qlx_log!` macro, which maps to `soroban_sdk::log!` under
`cfg(test)` and is a no-op in release WASM.

---

## 4. Invoice lifecycle

```
Pending ──► Verified ──► Funded ──► Paid
   │            │            │
   └──────────► Cancelled    ├──► Defaulted
                             └──► Refunded
```

| Status | How you reach it | Who |
|--------|-----------------|-----|
| `Pending` | `upload_invoice` / `store_invoice` | Business |
| `Verified` | `verify_invoice` | Admin |
| `Funded` | `accept_bid` / `accept_bid_and_fund` | Business (bid accepted) |
| `Paid` | `settle_invoice` (full payment) | Settlement engine |
| `Defaulted` | `mark_invoice_defaulted` after grace period | Admin / overdue scanner |
| `Cancelled` | `cancel_invoice` (Pending or Verified only) | Business |
| `Refunded` | `refund_escrow_funds` (Funded only) | Admin or Business |

**Investment status** mirrors invoice lifecycle and is updated atomically:

```
Active ──► Completed   (full settlement)
Active ──► Defaulted   (grace period exceeded)
Active ──► Refunded    (escrow refund)
Active ──► Withdrawn   (investor withdrawal before funding)
```

All states other than `Active` are terminal. Attempting a second transition from a
terminal investment returns `QuickLendXError::InvalidStatus`.

### Real entry-point sequence

```rust
// 1. Business uploads
let invoice_id = client.upload_invoice(&business, amount, &currency,
    due_date, &description, &category, &tags)?;

// 2. Admin verifies
client.verify_invoice(&invoice_id)?;

// 3. Investor bids
let bid_id = client.place_bid(&investor, &invoice_id, bid_amount, expected_return)?;

// 4. Business accepts → creates escrow + investment
client.accept_bid(&invoice_id, &bid_id)?;

// 5. Business settles (partial or full)
client.settle_invoice(&invoice_id, payment_amount)?;
// Full payment → InvoiceStatus::Paid, InvestmentStatus::Completed
```

---

## 5. Bidding system

**Key limits (from `src/bid.rs`):**

| Constant | Value | Meaning |
|----------|-------|---------|
| `MAX_BIDS_PER_INVOICE` | 50 | Active (`Placed`) bids per invoice |
| `DEFAULT_BID_TTL_DAYS` | 7 | Default expiry after placement |
| `MIN_BID_TTL_DAYS` | 1 | Admin-configurable floor |
| `MAX_BID_TTL_DAYS` | 30 | Admin-configurable ceiling |
| `DEFAULT_MAX_ACTIVE_BIDS_PER_INVESTOR` | 20 | Active bids per investor across all invoices |

**Storage layout** — bids use an indexed key design to avoid O(n) reads on every
mutation. The per-invoice index is:

```
BidIndexKey::Count(invoice_id)        → u32
BidIndexKey::Entry(invoice_id, idx)   → BytesN<32>  (bid_id)
```

Adding a bid is O(1): write one `Entry` + increment `Count`.

**Ranking** (`get_ranked_bids`, `get_best_bid`) is deterministic:
1. Highest `expected_return / bid_amount` ratio (yield).
2. Tiebreak: earliest `timestamp`.

**Race safety** — `cancel_bid` and `withdraw_bid` validate `BidStatus::Placed`
before mutating. If a concurrent transition has already moved the bid to a
terminal status, the call returns without error (`cancel_bid`) or
`OperationNotAllowed` (`withdraw_bid`), preventing double-action execution.

**Cleanup** — `cleanup_expired_bids(invoice_id)` removes `Expired` bids from the
index. It is called automatically inside `place_bid` and `accept_bid_impl` so the
index stays bounded. For large bid lists use `cleanup_expired_bids_paged` to
stay within the instruction budget.

---

## 6. Escrow and payments

Escrow is created by `payments::create_escrow` when `accept_bid` runs. It holds
investor funds until the invoice is released or refunded.

| Function | Who calls it | Effect |
|----------|-------------|--------|
| `accept_bid` | Business | Creates escrow (Held), marks invoice Funded |
| `release_escrow_funds` | Admin / settlement | Releases escrow to business |
| `refund_escrow_funds(invoice_id, caller)` | Admin or Business | Refunds investor, sets status Refunded |
| `refund_escrow(invoice_id)` | Admin (convenience) | Alias that resolves `caller` from stored admin |

All fund-moving entry-points are wrapped in `reentrancy::with_payment_guard`.
This sets a re-entrancy flag in instance storage at the start of the call and
clears it on exit. A nested call on the same ledger returns
`QuickLendXError::OperationNotAllowed`. Do not call payment functions directly
from other payment functions — always go through the public entry-point.

---

## 7. KYC / verification gates

Two independent KYC stores exist:

- **`BusinessVerificationStorage`** — tracks `BusinessVerification` records with
  statuses `Pending | Verified | Rejected`.
- **`InvestorVerificationStorage`** — tracks `InvestorVerification` records with
  `investment_limit`, tier, and risk level.

**Where the gates fire:**

| Entry-point | Gate |
|-------------|------|
| `upload_invoice` | `require_business_not_pending` → `BusinessNotVerified` (no record / rejected) or `KYCAlreadyPending` (pending) |
| `cancel_invoice` | `require_business_not_pending` |
| `accept_bid` | `require_business_not_pending` |
| `place_bid` | Investor record must exist and have `Verified` status; `bid_amount ≤ investment_limit` |
| `withdraw_bid` | `require_investor_not_pending` |

`store_invoice` (unauthenticated path) does **not** enforce the business KYC gate
by design — it is an internal / migration helper. Use `upload_invoice` for the
normal business flow.

**Admin flow:**

```rust
// Business submits KYC
client.submit_kyc_application(&business, &kyc_data)?;

// Admin approves
client.verify_business(&admin, &business)?;

// Investor submits KYC
client.submit_investor_kyc(&investor, &kyc_data)?;

// Admin approves with investment limit
client.verify_investor(&investor, investment_limit)?;
```

---

## 8. Pause and emergency-withdraw circuit breakers

### Pause

`PauseControl` stores a single boolean in persistent storage under `"paused"`.

- **Setter:** `pause(admin)` / `unpause(admin)` — admin only.
- **Gate:** `PauseControl::require_not_paused(&env)?` returns
  `QuickLendXError::ContractPaused (2100)`.
- **Scope:** All mutating entry-points check pause first. Read-only entry-points
  (`get_invoice`, `get_protocol_health`, etc.) remain available while paused.

### Emergency withdraw

Three-step protocol for moving stuck non-escrow funds:

1. **Initiate** — `initiate_emergency_withdraw(admin, token, amount, target)` starts a
   24-hour timelock.
2. **Wait** — `emg_time_until_unlock` shows seconds remaining.
3. **Execute** — `execute_emergency_withdraw(admin)` after the timelock. Protected
   by the reentrancy guard.

The function validates that `amount ≤ (token_balance − held_escrow_reserve)` so
investor-held escrow can never be drained via the emergency path.

`cancel_emergency_withdraw(admin)` can abort any pending withdrawal before it executes.

---

## 9. Protocol limits and string caps

Hard limits live in `src/protocol_limits.rs`.

### Numeric limits (runtime-configurable by admin)

| Field | Default (non-test) | Meaning |
|-------|--------------------|---------|
| `min_invoice_amount` | 1_000_000 (1 token, 6dp) | Floor for invoice face value |
| `min_bid_amount` | 10 | Floor for bid amount |
| `min_bid_bps` | 100 (1%) | Minimum yield rate |
| `max_due_date_days` | 365 | Maximum days until due date |
| `grace_period_seconds` | 604_800 (7 days) | Grace before default |
| `max_invoices_per_business` | 100 (0 = unlimited) | Active invoice cap per business |

### String length caps (compile-time)

| Constant | Max bytes |
|----------|-----------|
| `MAX_DESCRIPTION_LENGTH` | 1 024 |
| `MAX_NAME_LENGTH` | 150 |
| `MAX_ADDRESS_LENGTH` | 300 |
| `MAX_TAX_ID_LENGTH` | 50 |
| `MAX_NOTES_LENGTH` | 2 000 |
| `MAX_TAG_LENGTH` | 50 |
| `MAX_TRANSACTION_ID_LENGTH` | 124 |
| `MAX_DISPUTE_REASON_LENGTH` | 1 000 |
| `MAX_DISPUTE_EVIDENCE_LENGTH` | 2 000 |
| `MAX_DISPUTE_RESOLUTION_LENGTH` | 2 000 |
| `MAX_KYC_DATA_LENGTH` | 5 000 |
| `MAX_REJECTION_REASON_LENGTH` | 500 |
| `MAX_FEEDBACK_LENGTH` | 1 000 |

Use `protocol_limits::check_string_length(s, max)` to validate before storing.
It returns `QuickLendXError::InvalidDescription` on overflow (reused for all
string-too-long cases).

---

## 10. Error codes — stability contract

Error codes in `QuickLendXError` are **stable integers**. The `#[contracterror]`
attribute serialises them as XDR; renumbering is a breaking ABI change that
breaks every off-chain integration without warning.

```rust
// errors.rs — do NOT renumber these
InvoiceNotFound = 1000,
Unauthorized    = 1100,
InvalidAmount   = 1200,
StorageError    = 1300,
...
ContractPaused  = 2100,
```

When adding a new error:
1. Use the next free number in an existing category, or a new category block.
2. Add the `// BREAKING: Do not renumber` comment.
3. Add the `Symbol` mapping in the `impl From<QuickLendXError> for Symbol` block.
4. Document it in `docs/contracts/errors.md`.

---

## 11. Events — topic stability contract

Event topics are pinned by `TOPIC_*` constants in `src/events.rs`. Off-chain
indexers subscribe to these exact strings.

```rust
pub const TOPIC_INVOICE_UPLOADED:  &str = "invoice_uploaded";
pub const TOPIC_BID_PLACED:        &str = "bid_placed";
pub const TOPIC_ESCROW_CREATED:    &str = "escrow_created";
pub const TOPIC_DISPUTE_CREATED:   &str = "dispute_created";
// ... etc.
```

**Semantic aliases** map domain names to canonical types:

| Alias | Canonical type | Topic constant |
|-------|---------------|----------------|
| `InvoiceCreated` | `InvoiceUploaded` | `TOPIC_INVOICE_UPLOADED` |
| `FundsLocked` | `EscrowCreated` | `TOPIC_ESCROW_CREATED` |
| `LoanSettled` | `InvoiceSettled` | `TOPIC_INVOICE_SETTLED` |
| `DisputeOpened` | `DisputeCreated` | `TOPIC_DISPUTE_CREATED` |

Renaming a `TOPIC_*` constant or changing the field order of a `#[contractevent]`
struct is a **breaking event-schema change**. Any such change must be coordinated
with the indexer team and backend before merge.

---

## 12. Test harness authorization pattern

Soroban's `Address::require_auth()` is enforced by the host in production. In
tests, use `env.mock_all_auths()` — it tells the host to accept every auth check
without a real signature. **This does not weaken production security.**
`mock_all_auths()` is only available under the `testutils` feature and has no
effect in deployed WASM.

### Canonical test setup pattern

```rust
#[test]
fn test_full_lifecycle() {
    let env = Env::default();
    // 1. Enable auth mocking FIRST, before any contract call.
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // 2. Set admin.
    let admin = Address::generate(&env);
    client.initialize_admin(&admin).unwrap();

    // 3. KYC-verify the business.
    let business = Address::generate(&env);
    client.submit_kyc_application(&business,
        &String::from_str(&env, r#"{"name":"Acme Corp"}"#)).unwrap();
    client.verify_business(&admin, &business).unwrap();

    // 4. KYC-verify the investor.
    let investor = Address::generate(&env);
    client.submit_investor_kyc(&investor,
        &String::from_str(&env, r#"{"name":"Alice"}"#)).unwrap();
    client.verify_investor(&investor, 1_000_000_i128).unwrap();

    // 5. Register a token, mint, and approve for the investor (required for
    //    accept_bid / place_bid token transfers).
    // ... (use the setup_token helper from src/test.rs)

    // 6. Run the scenario.
    let invoice_id = client.upload_invoice(
        &business,
        50_000_i128,
        &currency,
        env.ledger().timestamp() + 86_400,
        &String::from_str(&env, "Web dev services"),
        &InvoiceCategory::Services,
        &Vec::new(&env),
    ).unwrap();

    client.verify_invoice(&invoice_id).unwrap();
    // ... etc.
}
```

### Rules

| Rule | Why |
|------|-----|
| Call `env.mock_all_auths()` **before** `env.register(...)` or any client call | The mock must be active before the first host call |
| Set up admin before any admin-only operation | `require_admin` checks the stored address, not just auth |
| KYC-verify actors before `upload_invoice` / `place_bid` | Production code enforces KYC independently of auth |
| `mock_all_auths()` does **not** bypass business-logic checks | `NotAdmin`, `BusinessNotVerified`, `DisputeNotAuthorized` still fire |

### Shared test helpers (`src/test.rs`)

```rust
// Sets up env + contract + admin (calls mock_all_auths internally)
pub fn setup_env() -> (Env, QuickLendXContractClient<'static>, Address, Address);

// KYC-verifies a new business address
pub fn setup_verified_business(env, client, admin) -> Address;

// KYC-verifies a new investor address with a given limit
pub fn setup_verified_investor(env, client, limit) -> Address;

// Registers a Stellar asset contract, mints tokens, and approves the contract
pub fn setup_token(env, business, investor, contract_id) -> Address;

// Creates a fully funded invoice (business + investor + token + bid + accept)
pub fn create_funded_invoice(env, client, admin)
    -> (BytesN<32>, Address, Address, Address, Address);
```

---

## 13. WASM size budget

The contract **must** stay within 256 KiB (262 144 bytes) to be accepted by the
Stellar network.

| Tier | Size range | CI behaviour |
|------|-----------|--------------|
| OK | ≤ 235 929 B (90% of budget) | Green |
| Warning | 235 930 – 262 144 B | Yellow — diagnostic only, does not fail |
| Over budget | > 262 144 B | Red — **CI fails** |

A second gate rejects builds that grow more than **5%** above the recorded
baseline (217 668 B) even if still under budget.

```bash
# Check locally before pushing
./scripts/check-wasm-size.sh

# Or via Rust integration test
cargo test --test wasm_build_size_budget
```

When adding features that legitimately increase the binary, update the baseline
in **all three locations** in the same PR:

1. `tests/wasm_build_size_budget.rs` → `WASM_SIZE_BASELINE_BYTES`
2. `scripts/check-wasm-size.sh` → `BASELINE_BYTES`
3. `scripts/wasm-size-baseline.toml` → `[baseline].bytes` and `recorded`

---

## 14. Storage TTL and index layout

Soroban storage entries expire if their TTL is not extended. The protocol uses:

- **`Persistent`** storage for all long-lived data (invoices, bids, KYC records,
  escrow, investments). Persistent entries survive indefinitely but need TTL
  extension as they approach expiry.
- **`Instance`** storage for contract-wide config (admin, pause flag, protocol
  version, fee config). Instance storage TTL is extended with the contract
  instance itself.

`InvoiceStorage` maintains several secondary indexes for efficient filtering:

| Index key | Purpose |
|-----------|---------|
| `invoices_by_status(status)` | `get_invoices_by_status` |
| `invoices_by_business(addr)` | `get_business_invoices` |
| `invoices_by_tag(tag)` | `get_invoices_by_tag` |
| `invoices_by_category(cat)` | `get_invoice_count_by_category` |
| `invoices_by_customer(name)` | `get_invoices_by_customer` |
| `invoices_by_tax_id(id)` | `get_invoices_by_tax_id` |

**Index consistency rule:** any function that changes `invoice.status` must
call `InvoiceStorage::remove_from_status_invoices(old)` then
`InvoiceStorage::add_to_status_invoices(new)` in the same call. See
`verify_invoice` and `accept_bid_impl` for the canonical pattern.

If an index drifts (e.g. after a backup restore), use
`rebuild_invoice_indexes(admin, offset, limit)` to rebuild secondary indexes
from canonical `Invoice` records in paginated batches without touching primary
records.

---

## 15. Adding a new entry-point

1. **Implement** the function logic in the relevant `src/<module>.rs` file.
2. **Expose** it in `#[contractimpl] impl QuickLendXContract` in `src/lib.rs`:
   - Add `pause::PauseControl::require_not_paused(&env)?;` at the top of any
     mutating function.
   - Wrap token-transfer paths in
     `reentrancy::with_payment_guard(&env, || { ... })`.
   - Validate all string arguments with `protocol_limits::check_string_length`.
   - Emit a structured `#[contractevent]` from `src/events.rs` at the end.
3. **Error code:** use an existing code or register a new one (see
   [§10](#10-error-codes--stability-contract)).
4. **Event:** add a `TOPIC_*` constant and `#[contractevent]` struct if needed
   (see [§11](#11-events--topic-stability-contract)).
5. **Test:** add a test in `src/test_<module>.rs`. Follow the auth pattern in
   [§12](#12-test-harness-authorization-pattern).
6. **Docs:** add or update the relevant doc in `docs/contracts/`.
7. **Build checks:** run the full checklist in [§16](#16-checklist-before-opening-a-pr).

---

## 16. Checklist before opening a PR

```
□  cargo build                                       # compiles
□  cargo build --target wasm32-unknown-unknown \
     --release                                       # WASM compiles
□  cargo test                                        # tests pass
□  cargo clippy --workspace --all-targets \
     -- -D warnings                                  # no clippy warnings
□  ./scripts/check-wasm-size.sh                      # under 256 KiB budget
□  No std:: calls introduced                         # grep -r "std::" src/
□  New entry-points have pause guard                 # require_not_paused
□  Token-transfer paths use reentrancy guard         # with_payment_guard
□  String inputs validated with check_string_length  # protocol_limits
□  Error codes not renumbered                        # errors.rs unchanged
□  Event topics not renamed                          # events.rs TOPIC_*
□  Secondary index updated on status change          # remove + add pattern
□  New doc linked from docs/contracts/ index         # or README
□  PR description contains: Closes #<issue-number>
```

---

## Related documents

- [`docs/contracts/invoice-lifecycle.md`](invoice-lifecycle.md) — full state machine with transitions
- [`docs/contracts/errors.md`](errors.md) — error code reference
- [`docs/contracts/events.md`](events.md) — event schema reference
- [`docs/contracts/security.md`](security.md) — reentrancy, pause, and access control
- [`docs/contracts/protocol-limits.md`](protocol-limits.md) — all configurable limits
- [`quicklendx-contracts/README.md`](../../quicklendx-contracts/README.md) — build, deploy, and test overview
- [`AGENTS.md`](../../AGENTS.md) — repo-wide coding conventions
