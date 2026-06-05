# Test Output and Off-By-One Note

## Test Output

```text
$ cargo test test_protocol_limits_boundary test_max_invoices_per_business

running 7 tests
test test_protocol_limits_boundary::test_set_protocol_config_atomic_application ... ok
test test_protocol_limits_boundary::test_min_invoice_amount_exact_boundary ... ok
test test_protocol_limits_boundary::test_max_due_date_days_exact_boundary ... ok
test test_protocol_limits_boundary::test_grace_period_exact_boundary ... ok
test test_max_invoices_per_business::test_business_at_cap_exact_boundary ... ok
test test_max_invoices_per_business::test_zero_limit_is_unlimited ... ok
test test_max_invoices_per_business::test_off_by_one_edge_case_limit_one ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
```

## Off-By-One Note

When validating limits, specifically boundaries, off-by-one errors frequently happen due to strict vs. non-strict inequality comparisons (`>` vs `>=`).

- **Invoice Amount**: Prevented an off-by-one vulnerability where `min_invoice_amount` check in `admin.rs` and `init.rs` only validated `cfg.min_invoice_amount == 0`. It was fixed to `cfg.min_invoice_amount <= 0` to properly reject negative invalid bounds.
- **Due Date**: The `max_due_date_days` has been rigorously verified to allow exactly `730` but fail securely at `731`.
- **Business Capacity**: Validated the `max_invoices_per_business` boundary. For a limit $N$, the protocol properly permits the $N^{th}$ creation and completely blocks the $(N+1)^{th}$ creation, preventing state exhaustion.
