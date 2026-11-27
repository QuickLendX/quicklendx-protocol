# Soroban Gas & CPU Instruction Regression Baselines

This document explains the purpose, workflow, and security considerations of the per-entrypoint gas/CPU instruction regression baseline system in QuickLendX.

## How it Works

The gas regression harness in `quicklendx-contracts/tests/gas_regression.rs` verifies that every contract entrypoint's resource footprint (CPU instructions, disk reads, and disk writes) remains bounded and matches known baseline estimates within a strict tolerance gate (default: **3%**).

- **Baselines Storage**: Baselines are persisted in [gas-baseline.toml](../quicklendx-contracts/scripts/gas-baseline.toml).
- **Tolerance**: Defaults to **3%**. It can be overridden at runtime via the `GAS_TOLERANCE` environment variable (e.g. `GAS_TOLERANCE=0.05` for 5%).
- **Verification Gate**: Integration tests fail if any entrypoint/scenario exceeds its registered baseline plus the allowed tolerance, preventing accidental resource inflation.

---

## Developer Workflow

### 1. Reviewing Diffs in Pull Requests

When reviewing pull requests:
- Inspect changes to [gas-baseline.toml](../quicklendx-contracts/scripts/gas-baseline.toml).
- Legitimately modified code paths (e.g. adding new validation steps or indices) may slightly increase the baselines.
- Bumps to baseline values must be accompanied by technical explanation in the PR. Silently regressing performance is rejected.

### 2. Updating Baselines

If you modify contract logic and resource consumption legitimately increases, you must update the baselines.

Run the automated baseline update script from the repository root (or `quicklendx-contracts/`):
```bash
./quicklendx-contracts/scripts/update-gas-baseline.sh
```
This script executes `cargo test --test gas_regression` with `UPDATE_GAS_BASELINE=1` enabled, measuring actual resource usage and writing deterministic UTF-8 (no BOM) baselines back into `quicklendx-contracts/scripts/gas-baseline.toml`.

Verify determinism and regression enforcement by running:
```bash
cd quicklendx-contracts && cargo test --test gas_regression
```

---

## Soroban Transaction Footprint Limits

In addition to per-entrypoint gas regression baselines, QuickLendX enforces hard Soroban transaction footprint limits (ledger read entries, write entries, read bytes, write bytes, CPU instructions, and memory bytes) at maximum configured protocol bounds.

- **Single-Source Constants**: Defined in `FOOTPRINT_LIMITS` in [operational_limits.rs](../quicklendx-contracts/src/operational_limits.rs).
- **Enforced Test Suite**: [test_footprint_limits.rs](../quicklendx-contracts/src/test_footprint_limits.rs) runs `accept_bid_and_fund`, `settle_invoice`, `cleanup_expired_bids`, `process_partial_payment`, and paged queries (`get_payment_records`, `get_bids_for_invoice`, `get_dispute_timeline`) at their maximum configured bounds and asserts `env.cost_estimate().resources()` / `env.budget()` stay under the documented ceilings.

### Enforced Footprint Constants

| Constant | Entrypoint | Max Read Entries | Max Write Entries | Max Read Bytes | Max Write Bytes | Max Instructions | Max Mem Bytes |
|---|---|---|---|---|---|---|---|
| `FOOTPRINT_ACCEPT_BID_AND_FUND` | `accept_bid_and_fund` (50 bids) | 100 | 50 | 200 000 | 66 560 | 100 000 000 | 41 943 040 |
| `FOOTPRINT_SETTLE_INVOICE` | `settle_invoice` | 100 | 50 | 200 000 | 66 560 | 100 000 000 | 41 943 040 |
| `FOOTPRINT_CLEANUP_EXPIRED_BIDS` | `cleanup_expired_bids` (50 expired bids) | 100 | 50 | 200 000 | 66 560 | 100 000 000 | 41 943 040 |
| `FOOTPRINT_PROCESS_PARTIAL_PAYMENT` | `process_partial_payment` (after 50 payments) | 100 | 50 | 200 000 | 66 560 | 100 000 000 | 41 943 040 |
| `FOOTPRINT_PAGED_QUERIES` | Paged queries (limit 50) | 100 | 50 | 200 000 | 66 560 | 100 000 000 | 41 943 040 |

### Running & Updating Footprint Limits

```bash
cd quicklendx-contracts && cargo test --lib test_footprint_limits
```

If a legitimate protocol change alters an entrypoint's resource footprint:
1. Edit the corresponding `FOOTPRINT_*` constant in `quicklendx-contracts/src/operational_limits.rs` (must stay within the `SOROBAN_MAX_*` network caps).
2. Re-run `cd quicklendx-contracts && cargo test --lib test_footprint_limits` and `cargo test --lib operational_limits`.
3. Update the tables in `docs/CAPS.md` and `docs/gas-baselines.md` and explain the change in the PR description.

---

## Security Considerations

> [!IMPORTANT]
> **Baselines are non-load-bearing for correctness.**
>
> 1. The test harness relies on standard client entrypoints and respects all authorization checks. The measurement probes do not modify contract behavior or bypass authentication checks (`require_auth`).
> 2. Baselines are for diagnostic and CI regression check purposes only. They have no impact on production contract execution, state limits, or blockchain protocol finality.
