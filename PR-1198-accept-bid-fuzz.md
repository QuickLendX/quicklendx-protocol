# PR: Add fuzz harness for `accept_bid_and_fund` covering token transfer failure modes

Closes #1198

## 📝 Description

Adds a property-based fuzz harness and deterministic edge-case tests for the `accept_bid_and_fund` entrypoint, verifying atomicity and state invariants under token transfer failures, KYC/pause/expiry gates, and concurrent double-accept attempts.

## 🎯 Type of Change

- [x] New feature
- [x] Documentation update
- [ ] Test improvement

## 🔧 Changes Made

### New Files Added
- `quicklendx-contracts/src/test_fuzz_accept_bid_and_fund.rs` — fuzz harness + 9 deterministic tests
- `docs/accept-bid-fuzz.md` — documentation

### Files Modified
- `quicklendx-contracts/src/lib.rs` — added `mod test_fuzz_accept_bid_and_fund` (line 170)

### Key Changes
1. **FailToken mock contract** (`#[contract]` in the same crate) with 5 injectable modes: `Normal`, `BalanceZero`, `AllowanceZero`, `TransferPanic`, `TransferFromPanic`
2. **Fuzz strategy** (`AcceptBidScenario`) generates random combinations of: pause state, business KYC status, bid expiry, token fail mode, zero balance/allowance, and invoice amounts (100–10,000)
3. **Invariant checks** after every call verify: invoice status, escrow existence, bid status, investment count, and contract token balance
4. **Proptest harnesses**: `test_fuzz_accept_bid` (50,000 cases) and `test_fuzz_accept_bid_smoke` (10 cases)
5. **9 deterministic tests** covering: happy path, paused, business pending, expired bid, insufficient balance, insufficient allowance, `transfer_from` panic cleanup, `TransferPanic` no-op (flow uses `transfer_from`), double-accept rejection

## 🧪 Testing

- [x] Unit tests pass (9/9 deterministic + fuzz smoke)
- [x] All existing tests pass
- [x] Edge cases tested
- [x] No breaking changes introduced

### Test Coverage
- Deterministic tests: `cargo test --features fuzz-tests -- test_accept_bid` — 9 tests, all pass
- Fuzz smoke: `cargo test --features fuzz-tests -- test_fuzz_accept_bid_smoke` — 10 random cases, passes
- Full fuzz: `PROPTEST_CASES=50000 cargo test --features fuzz-tests -- test_fuzz_accept_bid`

## 📋 Contract-Specific Checks

- [x] Soroban contract builds successfully
- [x] WASM compilation works
- [x] Security considerations reviewed
- [x] Events properly emitted
- [x] Contract functions tested
- [x] Error handling implemented
- [x] Access control verified

## 📚 Documentation

- [x] `docs/accept-bid-fuzz.md` written with full test inventory, running instructions, and atomicity property description

## 🔗 Related Issues

Closes #1198

## 🧪 How to Test

1. `cd quicklendx-contracts`
2. `cargo test --features fuzz-tests -- test_accept_bid` — deterministic edge cases
3. `cargo test --features fuzz-tests -- test_fuzz_accept_bid_smoke` — fuzz smoke
4. `PROPTEST_CASES=50000 cargo test --features fuzz-tests -- test_fuzz_accept_bid` — full CI fuzz
