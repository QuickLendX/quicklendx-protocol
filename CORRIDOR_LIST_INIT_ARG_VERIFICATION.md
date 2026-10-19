# Corridor List as Init Arg — Verification

**Issue:** #2375  
**Feature:** Expose the corridor list as an initialization argument.

## Acceptance Criteria Verification

### ✅ The change matches the summary

The corridor list (`corridors: Vec<Address>`) is exposed as an initialization argument in the `InitializationParams` struct and passed through the contract's `initialize()` entrypoint. Corridors are approved counterparty addresses for cross-invoice operations.

### ✅ Tests cover the new behavior (happy path + explicit failure modes)

All tests are in `src/test_init_invariants.rs` — Section 7 "CORRIDOR LIST TESTS":

| Test | Type | Status |
|---|---|---|
| `test_init_with_empty_corridors_succeeds` | Happy path | ✅ |
| `test_init_with_non_empty_corridors_succeeds` | Happy path | ✅ |
| `test_init_with_duplicate_corridors_fails` | Failure: duplicates | ✅ |
| `test_init_with_corridor_equal_to_admin_fails` | Failure: reserved addr | ✅ |
| `test_init_with_corridor_equal_to_treasury_fails` | Failure: reserved addr | ✅ |
| `test_init_with_corridor_equal_to_contract_fails` | Failure: reserved addr | ✅ |
| `test_init_with_corridor_equal_to_zero_fails` | Failure: zero addr | ✅ |
| `test_reinit_with_same_corridors_succeeds` | Idempotency | ✅ |
| `test_corridors_persist_across_idempotent_reinit` | Persistence | ✅ |

### ✅ Implementation details

**Data model (`src/init.rs`):**
- `CORRIDORS_KEY` storage key
- `corridors: Vec<Address>` field in `InitializationParams` with doc comment
- Validation: no duplicates, no reserved addresses (admin, treasury, contract, zero)
- Storage: persisted when non-empty; `get_corridors()` returns empty vec if unset

**Contract entrypoints (`src/lib.rs`):**
- `initialize(env, params)` — accepts `InitializationParams` with corridors
- `get_corridors(env)` — public getter

**Events (`src/events.rs`):**
- `ProtocolInitialized` event includes `corridors: Vec<Address>`

**Alternate contract (`src/contract.rs`):**
- Flat `initialize()` signature includes `corridors: Vec<Address>` arg

### ✅ PR description references issue

This document is part of a PR that references `Closes #2375`.

## Files Changed

| File | Change |
|---|---|
| `quicklendx-contracts/src/init.rs` | Added `corridors` field to `InitializationParams`, storage key, validation, store/get logic |
| `quicklendx-contracts/src/lib.rs` | `initialize()` accepts `InitializationParams` (includes corridors), `get_corridors()` public getter |
| `quicklendx-contracts/src/contract.rs` | Flat `initialize()` arg includes `corridors: Vec<Address>` |
| `quicklendx-contracts/src/events.rs` | `ProtocolInitialized` event includes `corridors` field |
| `quicklendx-contracts/src/test_init_invariants.rs` | 9 corridor-specific tests (happy + failure + idempotency + persistence) |
