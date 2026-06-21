# Structured Diagnostics

QuickLendX contract diagnostics use the `qlx_log!` macro from
`src/diagnostics.rs`. The macro prefixes every message with a stable domain tag
so contributors can filter Soroban logs while debugging contract flows.

## Feature Gate

Diagnostics are emitted in two configurations:

| Configuration | Emits diagnostics? | Notes |
| --- | --- | --- |
| `cargo test` | Yes | Tests compile the emitting branch through `cfg(test)`. |
| `cargo test --features diagnostics` | Yes | Exercises the same emitting branch with the feature enabled. |
| `cargo build --features diagnostics` | Yes | Intended for local/debug builds that need structured logs. |
| `cargo build` | No | Non-test builds without the feature compile `qlx_log!` to a no-op. |
| `cargo build --profile release-with-logs` | No, unless `--features diagnostics` is also set | The profile enables debug assertions, but the diagnostics feature still controls `qlx_log!` emission. |

`release-with-logs` is useful for release-like debugging because it inherits the
release profile and enables debug assertions. Combine it with
`--features diagnostics` only when the build should retain structured logs.

## Domain Tags

Keep these tags stable because tests and log consumers use them as the contract
for structured diagnostics.

| Tag | Meaning | Example trigger |
| --- | --- | --- |
| `escrow` | Escrow creation, acceptance, refund, and release transitions | Accepting a bid, funding an invoice, refunding escrow. |
| `bid` | Bid placement, withdrawal, cancellation, and expiry | Placing or withdrawing a bid. |
| `settlement` | Partial payments, full settlement, and finalization | Recording partial payments or settling an invoice. |
| `payment` | Low-level token transfers and escrow fund movements | Creating, releasing, or refunding escrow funds. |

## Diagnostics vs Events

Add a diagnostic when the message is only needed by contributors during local
debugging, tests, or operational investigation. Diagnostics are not a stable
on-chain interface and may be compiled out.

Add an event when external consumers, indexers, users, or integrations need a
durable protocol signal. Events are part of the observable contract behavior and
must remain available outside debug builds.

## Validation

Run both configurations when changing `src/diagnostics.rs` or adding a new tag:

```bash
cargo test -p quicklendx-contracts --features diagnostics test_diagnostics -- --nocapture
cargo test -p quicklendx-contracts test_diagnostics -- --nocapture
```

The diagnostics tests assert the canonical tag list, verify domain prefixes in
captured logs, and document the feature-disabled test configuration.
