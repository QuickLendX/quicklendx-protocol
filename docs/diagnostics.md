# QuickLendX Structured Diagnostics

This document describes the `qlx_log!` macro — the uniform structured diagnostics system for QuickLendX Soroban smart contracts.

## Overview

`qlx_log!` is a feature-gated macro that emits domain-tagged log messages via the Soroban host's native diagnostic interface (`soroban_sdk::log!`). It provides consistent, filterable diagnostics across all contract modules **with a strict zero-overhead guarantee in production builds**.

---

## Feature Flags

| Flag / Context                     | Behavior                                     |
|------------------------------------|----------------------------------------------|
| `cargo test` (no extra flags)      | ✅ Logs emitted, captured by `env.logs()`     |
| `cargo test --features diagnostics`| ✅ Logs emitted, captured by `env.logs()`     |
| `cargo build` (release)            | 🚫 Macro expands to `{}` — zero cost          |
| `--features diagnostics` + release | ✅ Logs emitted (use `release-with-logs` profile) |

> **Never deploy a production WASM built with `--features diagnostics`** unless you also use the `release-with-logs` profile and intentionally want debug output. Standard `release` builds always have `debug-assertions = false`, which mutes `soroban_sdk::log!` at the host level regardless.

### Build Profiles

The project's `Cargo.toml` already defines:

```toml
[profile.release-with-logs]
inherits = "release"
debug-assertions = true
```

To build with diagnostics enabled for local CLI debugging:

```bash
stellar contract build \
  --profile release-with-logs \
  --features diagnostics
```

---

## Macro Usage

```rust
use crate::qlx_log;

// Plain message (no format args)
qlx_log!(env, "domain", "Human-readable message");

// With formatting arguments (forwarded to soroban_sdk::log!)
qlx_log!(env, "domain", "key={}", value);
qlx_log!(env, "domain", "a={} b={}", a, b);
```

### Parameters

| Parameter  | Type      | Description                                           |
|------------|-----------|-------------------------------------------------------|
| `env`      | `&Env`    | The Soroban environment reference                     |
| `domain`   | `&str` literal | A short string identifying the subsystem         |
| `msg`      | `&str` literal | A human-readable message, optionally with `{}`   |
| `args`     | any       | Format arguments matching `{}` placeholders in `msg` |

---

## Domains

Use the following domain strings consistently to enable log filtering in the Stellar CLI:

| Domain       | Module(s)                         | Lifecycle events covered                                      |
|--------------|-----------------------------------|---------------------------------------------------------------|
| `"escrow"`   | `escrow.rs`                       | Bid acceptance, escrow funding, escrow refund                 |
| `"bid"`      | `lib.rs` (entry points)           | Bid placement, bid withdrawal                                 |
| `"settlement"` | `settlement.rs`                 | Partial payment recording, full settlement, finalization      |
| `"payment"`  | `payments.rs`                     | Escrow creation, escrow release, escrow refund (fund moves)   |

---

## Example Output

When diagnostics are enabled, log output looks like:

```
[escrow] Accepting bid and funding invoice
[payment] Creating escrow: amount=50000
[payment] Escrow created successfully
[escrow] Invoice funded and bid accepted
[settlement] Recording partial payment: amount=10000
[settlement] Payment recorded: total_paid=10000 progress=20%
[settlement] Full settlement reached, finalizing
[payment] Releasing escrow funds to business
[settlement] Invoice settled: investor_return=49000 platform_fee=1000
[bid] Bid placed: amount=50000 expected_return=51000
[bid] Bid withdrawn
[payment] Refunding escrow to investor
[escrow] Escrow refunded successfully
```

---

## Viewing Logs

### In Unit Tests

```rust
#[test]
fn test_my_flow() {
    let env = Env::default();
    // ... set up contract ...
    contract.place_bid(env.clone(), investor, invoice_id, 5000, 5200);

    // Retrieve all diagnostic logs captured during this test
    let logs = env.logs().all();
    for log in logs.iter() {
        std::println!("{}", log);
    }

    // Assert a specific log was emitted
    assert!(logs.iter().any(|l| l.contains("[bid] Bid placed")));
}
```

### Via Stellar CLI

When running a contract against a local network with `--profile release-with-logs`:

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ACCOUNT> \
  --network testnet \
  -v \               # <-- verbose flag surfaces diagnostic events
  -- place_bid ...
```

---

## Debugging Workflow

1. **Reproduce locally:** Run `cargo test <test_name>` — logs are captured automatically and visible via `env.logs().all()` in the test body or with `--nocapture`.
2. **Targeted feature run:** Use `cargo test --features diagnostics <test_name> -- --nocapture` to force log emission even in non-test binary contexts.
3. **CLI debugging:** Build with `--profile release-with-logs --features diagnostics` and invoke with `-v`.
4. **Filter by domain:** Search for `[escrow]`, `[bid]`, `[settlement]`, or `[payment]` in output.

---

## Zero-Overhead Promise

When compiled without `cfg(test)` and without `--features diagnostics`, the macro expands to a no-op:

```rust
// This invocation:
qlx_log!(env, "escrow", "Escrow created: amount={}", escrow.amount);

// Compiles to exactly this in release mode:
{}
```

- **No string allocation** — format literals are not included in the binary
- **No argument evaluation** — `escrow.amount` is never read for logging
- **No host call** — `env.logs()` is never invoked
- **No WASM size increase** — the optimizer eliminates all call sites

The WASM size budget enforced by `scripts/check-wasm-size.sh` (256 KB) is not affected by `qlx_log!` calls in release builds.

---

---

## `get_protocol_diagnostics` Entry-Point

The contract exposes a `get_protocol_diagnostics` entry-point that returns a `ProtocolDiagnostics` struct with a rich internal snapshot — per-status invoice counts, bid counters, subsystem flags, fee config, and ledger info — in a single call.

**This entry-point is only compiled when `--features diagnostics` is set.** It is completely absent from standard production WASM builds: zero bytes, zero gas cost, and it does not appear in the ABI.

### Returned fields

| Field | Type | Description |
|-------|------|-------------|
| `total_invoices` | `u64` | All invoices ever created |
| `pending_invoices` | `u32` | Invoices in `Pending` status |
| `verified_invoices` | `u32` | Invoices in `Verified` status |
| `funded_invoices` | `u32` | Invoices in `Funded` status |
| `paid_invoices` | `u32` | Invoices in `Paid` status |
| `defaulted_invoices` | `u32` | Invoices in `Defaulted` status |
| `total_bids_ever` | `u64` | Monotonic bid counter |
| `is_paused` | `bool` | Protocol pause flag |
| `is_maintenance` | `bool` | Maintenance mode flag |
| `backpressure_active` | `bool` | Load-shedding flag |
| `fee_bps` | `u32` | Current fee in basis points |
| `currency_count` | `u32` | Whitelisted currency count |
| `ledger_sequence` | `u32` | Ledger sequence at snapshot time |
| `ledger_timestamp` | `u64` | Ledger timestamp at snapshot time |

### Invoking

```bash
# Build a diagnostics-enabled binary (never deploy this to production)
stellar contract build --profile release-with-logs --features diagnostics

# Invoke
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source <ACCOUNT> \
  --network testnet \
  -- get_protocol_diagnostics
```

### In tests

```rust
#[cfg(feature = "diagnostics")]
#[test]
fn my_test() {
    // ... setup ...
    let diag = client.get_protocol_diagnostics();
    assert_eq!(diag.verified_invoices, 1);
    assert!(!diag.is_paused);
}
```

---

## Adding New Log Points

To add structured diagnostics to a new function or module:

1. Import the macro: `use crate::qlx_log;` (or just `crate::qlx_log!(...)` directly).
2. Pick or define a domain string.
3. Add `qlx_log!(env, "domain", "message")` at the lifecycle transition.
4. (Optional) Add a unit test in `src/test/test_diagnostics.rs` verifying the log output.

```rust
// Example: adding a log to a new settlement path
pub fn my_new_function(env: &Env, invoice_id: &BytesN<32>) {
    crate::qlx_log!(env, "settlement", "my_new_function called");
    // ... implementation ...
}
```
