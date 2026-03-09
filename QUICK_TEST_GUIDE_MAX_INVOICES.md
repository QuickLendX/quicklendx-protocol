# Quick Test Guide - Max Invoices Per Business

## Run Tests

```bash
cd quicklendx-contracts
cargo test test_max_invoices --lib
```

## Test List

1. ✅ `test_create_invoices_up_to_limit_succeeds` - Create up to limit
2. ✅ `test_next_invoice_after_limit_fails_with_clear_error` - Error when exceeded
3. ✅ `test_cancelled_invoices_free_slot` - Cancelled frees slot
4. ✅ `test_paid_invoices_free_slot` - Paid frees slot
5. ✅ `test_config_update_changes_limit` - Dynamic config
6. ✅ `test_limit_zero_means_unlimited` - Unlimited mode
7. ✅ `test_multiple_businesses_independent_limits` - Per-business
8. ✅ `test_only_active_invoices_count_toward_limit` - Active counting
9. ✅ `test_various_statuses_count_as_active` - Status coverage
10. ✅ `test_limit_of_one` - Edge case

## Expected Coverage

>95% for max invoices per business feature

## Files Modified

- `src/protocol_limits.rs` - Added field
- `src/errors.rs` - Added error
- `src/invoice.rs` - Added counting function
- `src/lib.rs` - Added enforcement
- `src/test_max_invoices_per_business.rs` - Tests (NEW)

## Commit

```
test: max invoices per business enforcement
```

Branch: `test/max-invoices-per-business`
