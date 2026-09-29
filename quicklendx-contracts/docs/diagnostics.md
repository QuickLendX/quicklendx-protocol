# Structured Diagnostics

This document describes the structured diagnostics framework used in the QuickLendX smart contracts, the Cargo feature-gating mechanism, the meaning of domain-tagged signals, and guidelines for when contributors should emit diagnostics versus standard contract events.

## Feature Gate & Compilation Behavior

Diagnostic logs in QuickLendX are managed by the `qlx_log!` macro (defined in [diagnostics.rs](file:///Users/backenddevopsdeveloper/Downloads/DRIPS/geni3-quicklendx-protocol/quicklendx-contracts/src/diagnostics.rs)). This macro is feature-gated using Cargo features:

1. **Enabled Configurations**:
   - In unit/integration tests (i.e. under `#[cfg(test)]` context like running `cargo test`).
   - When compiled with the `--features diagnostics` flag.
   - In these configurations, `qlx_log!` formats and prints logs to the Soroban host log system via `soroban_sdk::log!`.

2. **Disabled Configurations (Production/Release)**:
   - In standard production release builds without the `diagnostics` feature.
   - In this mode, the macro expands to an empty statement `{}`. The compiler/LLVM fully eliminates format strings, arguments, and domain tag literals before WASM codegen, resulting in **zero runtime overhead** and no WASM size impact.

### Profiling with Logs

To build or test the contract in a release-like environment but with diagnostics/debug logs enabled, you can use the custom `release-with-logs` profile configured in the [Cargo.toml](file:///Users/backenddevopsdeveloper/Downloads/DRIPS/geni3-quicklendx-protocol/quicklendx-contracts/Cargo.toml):

```toml
[profile.release-with-logs]
inherits = "release"
debug-assertions = true
```

Building with `--profile release-with-logs --features diagnostics` allows testing production-optimized code while retaining debug/log visibility.

---

## Domain Tagged Signals

All diagnostic logs emitted via `qlx_log!` must carry an explicit string domain tag. This tag groups related diagnostic messages for easy filtering in developer consoles.

The following domain tags are defined and must be used consistently by contributors:

| Domain | Scope |
| :--- | :--- |
| `"escrow"` | Escrow creation, acceptance, refund, and release lifecycle transitions. |
| `"bid"` | Bid placement, withdrawal, cancellation, and expiry events. |
| `"settlement"` | Partial payments processing, full settlement, and contract finalization. |
| `"payment"` | Low-level token transfers and escrow fund movements. |

### Usage Example

```rust
// Standard log message
qlx_log!(env, "escrow", "Invoice funded and bid accepted");

// Log message with formatting arguments
qlx_log!(env, "payment", "Creating escrow: amount={}", amount);
```

---

## Diagnostics vs. Events

When adding status tracking, developers must choose between logging a diagnostic signal or publishing a contract event. Use the guidelines below to decide:

| Criterion | Diagnostic (`qlx_log!`) | Event (`env.events().publish`) |
| :--- | :--- | :--- |
| **Primary Audience** | Smart contract developers, integration testers. | Off-chain indexers, UI frontends, end-users. |
| **Production Presence** | Compiled out (absent in production builds). | Retained in production builds. |
| **Execution Cost** | Zero gas / storage cost in production. | Incurs gas and transaction storage fees. |
| **Use Case** | Internal state assertions, tracing control flow, debugging error paths. | Emitting state changes (e.g. bid accepted, invoice created) that indexers must track. |

---

## `get_protocol_diagnostics` Entry-Point

A feature-gated contract entry-point that returns a `ProtocolDiagnostics` struct with a rich internal snapshot useful for operators and support tooling.

**Only compiled when `--features diagnostics` is set — absent from production WASM builds.**

| Field | Type | Description |
|-------|------|-------------|
| `total_invoices` | `u64` | All invoices ever stored |
| `pending_invoices` | `u32` | Count in `Pending` status |
| `verified_invoices` | `u32` | Count in `Verified` status |
| `funded_invoices` | `u32` | Count in `Funded` status |
| `paid_invoices` | `u32` | Count in `Paid` status |
| `defaulted_invoices` | `u32` | Count in `Defaulted` status |
| `total_bids_ever` | `u64` | Monotonic bid counter |
| `is_paused` | `bool` | Pause flag |
| `is_maintenance` | `bool` | Maintenance mode flag |
| `backpressure_active` | `bool` | Load-shedding flag |
| `fee_bps` | `u32` | Fee in basis points |
| `currency_count` | `u32` | Whitelisted currency count |
| `ledger_sequence` | `u32` | Sequence at snapshot time |
| `ledger_timestamp` | `u64` | Timestamp at snapshot time |

No authentication required. Read-only; never mutates state. No PII is exposed.

### Summary Rule

If the signal is required for the frontend or indexing service to reconstruct contract history, use a contract **Event**. If the signal is only useful for troubleshooting, debugging, or verifying internal execution flow during development/testing, use a **Diagnostic**.
