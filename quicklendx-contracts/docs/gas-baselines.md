# Gas baselines

This document explains how to refresh and review per-entrypoint gas/CPU instruction baselines.

Run measurements:

  1. Implement or enable the `measure` harness in `src/bench.rs`.
  2. Run the bench/test harness:
     ```
     cargo test --test gas_regression -- --nocapture
     ```
  3. Update `scripts/gas-baseline.toml` with new values and commit.

Security note: baseline measurements are non-load-bearing and must not bypass `require_auth` or paused checks.
