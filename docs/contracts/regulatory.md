# Regulatory Compliance Hook

This document describes the `require_regulatory_ok` seam in the QuickLendX contract.

## Purpose

`require_regulatory_ok` is a **deliberately empty** gate that the protocol calls at
every state-changing entry point before committing work. Today it is a pure no-op that
always returns `Ok(())`, which means it has zero runtime cost and imposes no new
restrictions on existing callers.

The seam exists so that a future compliance layer (e.g. on-chain allowlist, off-chain
oracle attestation, jurisdiction-based block list) can be dropped in **without touching
any other contract module**: only `regulatory.rs` changes when the policy is upgraded.

## Current behaviour

- The hook is invoked from the following public entry points:
  - `store_invoice` — receives the `business` address
  - `place_bid` — receives the `investor` address
- It always returns `Ok(())`.
- No new error variants are introduced yet.

## Future behaviour

A future implementation may return a compliance-specific error (for example
`RegulatoryCheckFailed`) when the actor is not cleared for participation. The function
signature **must** remain identical so that all call sites continue to compile without
modification.

## Operator guidance

If you are building a compliance layer around QuickLendX:

1. Fork or extend `src/regulatory.rs`.
2. Replace the body of `require_regulatory_ok` with your logic.
3. If a new error variant is required, add it to `QuickLendXError` in `src/errors.rs`
   _before_ deploying to a network with existing callers.

## Tests

The no-op contract is locked in by `src/test_regulatory.rs`:

- `test_require_regulatory_ok_is_noop` calls the hook directly for a range of addresses.
- `test_store_invoice_regulatory_gate_is_noop` verifies that `store_invoice` succeeds.
- `test_place_bid_regulatory_gate_is_noop` verifies that `place_bid` succeeds.
