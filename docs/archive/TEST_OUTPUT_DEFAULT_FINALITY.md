# Test Output and Finality Security Note

## Test Output

```text
$ cargo test test_default_finality

running 2 tests
test test_default_finality::test_defaulted_invoice_operations_reject ... ok
test test_default_finality::test_single_insurance_claim_and_idempotent_default ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

## Finality Security Note

Terminal-state finality is a crucial fund-safety property for the QuickLendX protocol. Once an invoice enters the `Defaulted` state via `mark_invoice_defaulted` or `handle_default`, a `TransitionGuard` ensures the transition is atomic and idempotent.

Post-default operations such as `accept_bid_and_fund`, `settle_invoice`, and `process_partial_payment` strictly reject the invoice with `InvalidStatus`, guaranteeing that a defaulted invoice cannot be resurrected or double-settled. This guarantees that insurance claims, processed during the default transition, are executed exactly once per defaulted invoice.
