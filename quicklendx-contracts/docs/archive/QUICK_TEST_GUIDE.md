# Quick Test Guide - Invoice Count Tests

## Quick Start

```bash
cd quicklendx-contracts
cargo test invoice_count --lib
```

## Individual Tests

```bash
# Test all statuses
cargo test test_get_invoice_count_by_status_all_statuses --lib

# Test total equals sum
cargo test test_get_total_invoice_count_equals_sum_of_status_counts --lib

# Test status transitions
cargo test test_invoice_counts_after_status_transitions --lib

# Test cancellations
cargo test test_invoice_counts_after_cancellation --lib

# Test multiple updates
cargo test test_invoice_counts_with_multiple_status_updates --lib

# Test consistency
cargo test test_invoice_count_consistency --lib
```

## With Output

```bash
cargo test invoice_count --lib -- --nocapture
```

## Coverage Check

```bash
cargo tarpaulin --lib --exclude-files "test*.rs" --out Html
```

## What's Tested

- ✅ All 7 invoice statuses (Pending, Verified, Funded, Paid, Defaulted, Cancelled, Refunded)
- ✅ Total count = sum of status counts (invariant)
- ✅ Counts after create/cancel/status updates
- ✅ Complex multi-invoice scenarios
- ✅ Consistency across all operations

## Expected Results

All 6 tests should pass:

- test_get_invoice_count_by_status_all_statuses ... ok
- test_get_total_invoice_count_equals_sum_of_status_counts ... ok
- test_invoice_counts_after_status_transitions ... ok
- test_invoice_counts_after_cancellation ... ok
- test_invoice_counts_with_multiple_status_updates ... ok
- test_invoice_count_consistency ... ok

## Coverage Target

**>95%** for invoice count functions

## Documentation

See `INVOICE_COUNT_TESTS.md` for detailed documentation.
