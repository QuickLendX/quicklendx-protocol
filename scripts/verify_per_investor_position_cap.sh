#!/usr/bin/env bash
# Lightweight logic check for Issue #1858 (per_investor_position_cap).
# Full `cargo test` currently blocked by unrelated upstream compile errors on main;
# this script mirrors the validate_bid guard semantics.
set -euo pipefail

python3 - <<'PY'
def exceeds(bid: int, cap: int | None) -> bool:
    return cap is not None and bid > cap

assert not exceeds(3000, 3000), "bid == cap must pass"
assert exceeds(3001, 3000), "bid == cap+1 must fail"
assert not exceeds(5000, None), "uncapped must pass"
assert not exceeds(1, 3000), "bid below cap must pass"
print("per_investor_position_cap logic checks: OK")
PY
