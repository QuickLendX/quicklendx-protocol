# Analytics Contract Notes

## Investor Report Consistency

The analytics module now enforces deterministic investor-report behavior for a fixed ledger snapshot.

### Guarantees

- `generate_investor_report` derives report contents from persisted investor investment indexes instead of placeholder in-memory data.
- Generated investor reports are validated before persistence.
- Persisted reports are immediately retrievable through `get_investor_report`.
- Repeated generation in the same ledger state produces identical report contents, with only `report_id` changing.
- Empty-history investors return a valid zeroed report instead of failing or producing partial data.

### Persistence Model

- Investor reports are stored under their generated `report_id`.
- Business reports are also persisted on generation to keep reporting behavior consistent across analytics endpoints.
- Report identifiers now use a monotonic counter plus ledger timestamp and sequence values to avoid accidental key collisions.

## Security Assumptions

- Investor report generation trusts the contract-maintained investment index in `InvestmentStorage`.
- Report validation rejects malformed snapshots where:
  - `end_date < start_date`
  - `total_invested` or `total_returns` is negative
  - `success_rate` or `default_rate` falls outside `0..=10_000`
  - `risk_tolerance > 100`
  - preferred-category totals do not match `investments_made`
- Category counters use a fixed ordering to keep retrieval deterministic and review-friendly.

## Test Coverage

The focused analytics test suite in [`src/test/test_analytics.rs`](/c:/Users/ADMIN/Desktop/midea-drips/quicklendx-protocol/quicklendx-contracts/src/test/test_analytics.rs) covers:

- investor report generation consistency for a fixed snapshot
- investor report persistence round-trips
- retrieval determinism
- no-investment-history scenarios
- period filtering
- business report persistence regression coverage

## Test Command

```bash
cargo test --lib test_analytics -- --nocapture
```

Observed result during this change:

```text
running 7 tests
7 passed; 0 failed
```
