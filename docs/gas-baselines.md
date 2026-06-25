# Soroban Gas & CPU Instruction Regression Baselines

This document explains the purpose, workflow, and security considerations of the per-entrypoint gas/CPU instruction regression baseline system in QuickLendX.

## How it Works

The gas regression harness in `quicklendx-contracts/tests/gas_regression.rs` verifies that every contract entrypoint's resource footprint (CPU instructions, disk reads, and disk writes) remains bounded and matches known baseline estimates within a strict tolerance gate (default: **3%**).

- **Baselines Storage**: Baselines are persisted in [gas-baseline.toml](file:///c:/Users/user/Documents/quicklendx-protocol-2/quicklendx-contracts/scripts/gas-baseline.toml).
- **Tolerance**: Defaults to **3%**. It can be overridden at runtime via the `GAS_TOLERANCE` environment variable (e.g. `GAS_TOLERANCE=0.05` for 5%).
- **Verification Gate**: Integration tests fail if any entrypoint/scenario exceeds its registered baseline plus the allowed tolerance, preventing accidental resource inflation.

---

## Developer Workflow

### 1. Reviewing Diffs in Pull Requests

When reviewing pull requests:
- Inspect changes to [gas-baseline.toml](file:///c:/Users/user/Documents/quicklendx-protocol-2/quicklendx-contracts/scripts/gas-baseline.toml).
- Legitimately modified code paths (e.g. adding new validation steps or indices) may slightly increase the baselines.
- Bumps to baseline values must be accompanied by technical explanation in the PR. Silently regressing performance is rejected.

### 2. Updating Baselines

If you modify contract logic and resource consumption legitimately increases, you must update the baselines.

Run the automated baseline update script:
```bash
./scripts/update-gas-baseline.sh
```
This script executes the test suite with `UPDATE_GAS_BASELINE=1` enabled, measuring actual resource usage and automatically saving them back into `gas-baseline.toml`.

---

## Security Considerations

> [!IMPORTANT]
> **Baselines are non-load-bearing for correctness.**
>
> 1. The test harness relies on standard client entrypoints and respects all authorization checks. The measurement probes do not modify contract behavior or bypass authentication checks (`require_auth`).
> 2. Baselines are for diagnostic and CI regression check purposes only. They have no impact on production contract execution, state limits, or blockchain protocol finality.
