# Accept Bid & Fund — Fuzz Harness

Property-based fuzz coverage for the `accept_bid_and_fund` entrypoint
(`src/escrow.rs`), addressing issue **#1198**.

- **Harness:** `quicklendx-contracts/src/test_fuzz_accept_bid_and_fund.rs`
- **Feature gate:** `#[cfg(all(test, feature = "fuzz-tests"))]`
- **Engine:** [`proptest`](https://docs.rs/proptest)

## Failure-injection model

A lightweight mock token contract (`FailToken`) is deployed as the invoice
currency. The mock supports these failure modes, covering every error path
that `accept_bid_and_fund` can encounter after the KYC / pause / expiry gates:

| Failure mode         | What happens                                                |
|----------------------|-------------------------------------------------------------|
| `Normal`             | Happy path; token behaves correctly.                        |
| `BalanceZero`        | `balance()` returns 0  →  `InsufficientFunds` before xfer.   |
| `AllowanceZero`      | `allowance()` returns 0 →  `OperationNotAllowed` before xfer |
| `TransferPanic`      | `transfer()` panics — **not triggered** (flow uses `transfer_from`) |
| `TransferFromPanic`  | `transfer_from()` panics →  `TokenTransferFailed`.           |

Additional non-token failure conditions are driven by the fuzz strategy:

| Condition             | What happens                                         |
|-----------------------|------------------------------------------------------|
| `paused`              | `ContractPaused` before any state change.            |
| `business_pending`    | `BusinessNotVerified` before any state change.       |
| `bid_expired`         | `InvalidStatus` (bid cleaned up before processing).  |
| `investor_balance_zero` | `InsufficientFunds` at `balance()` check.          |
| `investor_allowance_zero` | `OperationNotAllowed` at `allowance()` check.     |

## Invariants asserted

After every call (success **or** failure) the harness checks:

1. **Invoice status:** `Funded` on success, `Verified` on failure.
2. **Escrow:** Exists iff the call succeeded — `Held` status, correct amount.
3. **Bid status:** `Accepted` on success; `Placed` or `Expired` on failure.
4. **Investments:** Exactly 1 on success, 0 on failure.
5. **Contract balance:** Holds `invoice_amount` tokens on success, 0 on failure.
6. **Invoice fields:** `funded_amount`, `investor` match expectations.

## Test inventory

| Test                                         | Kind                | Properties |
|----------------------------------------------|---------------------|------------|
| `test_fuzz_accept_bid`                       | proptest (50k)      | Full random coverage over all failure modes |
| `test_fuzz_accept_bid_smoke`                 | proptest (10)       | Quick smoke check |
| `test_accept_bid_happy_path`                 | unit edge           | Nominal flow succeeds |
| `test_accept_bid_paused_rejected`            | unit edge           | `ContractPaused` when paused |
| `test_accept_bid_business_pending_rejected`  | unit edge           | `BusinessNotVerified` when KYC pending |
| `test_accept_bid_expired_rejected`           | unit edge           | `InvalidStatus` for expired bid |
| `test_accept_bid_insufficient_balance_rejected` | unit edge        | `InsufficientFunds` when investor drained |
| `test_accept_bid_insufficient_allowance_rejected` | unit edge      | `OperationNotAllowed` when allowance revoked |
| `test_accept_bid_transfer_from_panic_cleanup`| unit edge           | `TokenTransferFailed` via `transfer_from` panic; state unchanged |
| `test_accept_bid_transfer_panic_cleanup`     | unit edge           | `TransferPanic` mode is a no-op (flow uses `transfer_from`) |
| `test_accept_bid_double_accept_rejected`     | unit edge           | Second accept of same bid+invoice rejected |

## Running

```shell
# Smoke (10 random cases)
cargo test --features fuzz-tests -- test_fuzz_accept_bid_smoke

# Full CI run (50 000 cases)
PROPTEST_CASES=50000 cargo test --features fuzz-tests -- test_fuzz_accept_bid

# Deterministic edge cases only
cargo test --features fuzz-tests -- test_accept_bid
```

## Atomicity property

The harness verifies that **no partial escrow state survives a failed transfer**.
If `transfer_from` panics, the invoice, bid, escrow, and investment records all
remain untouched. The contract balance is zero when any failure code path is
taken.
