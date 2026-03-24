# Analytics usage contract

## Overview

Analytics functions in QuickLendX are designed to be panic-free, deterministic,
and safe on empty datasets. This document defines the guaranteed behavior that
all consumers of the analytics module can rely on.

---

## Fallback behavior (empty state)

When no data exists in storage, all analytics functions return safe zero defaults:

| Field type        | Default value |
|-------------------|---------------|
| Counts            | 0             |
| Volume            | 0             |
| Fees              | 0             |
| Averages          | 0             |
| Rates             | 0             |
| Optional fields   | None          |
| Lists / vectors   | empty         |

---

## Guarantees

### 1. No panic

All analytics functions are panic-free under any storage state:

- No `unwrap()` on storage reads — missing keys return `None` and are handled
- Empty invoice / investment lists produce zero metrics, not errors
- All fallback values are explicitly constructed, not assumed

### 2. Deterministic output

Given the same ledger state, repeated calls to any analytics function return
identical results. There is no randomness or hidden mutable state.

### 3. Safe division

Every division operation is guarded against divide-by-zero:
```rust
// Pattern used throughout analytics.rs
if denominator > 0 {
    numerator.saturating_div(denominator)
} else {
    0
}
```

### 4. Overflow protection

All accumulation uses saturating arithmetic:
```rust
total_volume = total_volume.saturating_add(invoice.amount);
total_fees   = total_fees.saturating_add(platform_fee);
```

This means values cap at `i128::MAX` / `u32::MAX` rather than wrapping or panicking.

### 5. Underflow protection

Period date calculations use `saturating_sub` so small ledger timestamps
(e.g. in tests) never produce integer underflow:
```rust
let day_start = current_timestamp.saturating_sub(24 * 60 * 60);
```

---

## Functions covered by this contract

| Function                                        | Safe on empty? | Deterministic? |
|-------------------------------------------------|----------------|----------------|
| `get_analytics_summary()`                       | yes            | yes            |
| `get_platform_metrics()`                        | yes            | yes            |
| `get_performance_metrics()`                     | yes            | yes            |
| `get_financial_metrics(period)`                 | yes            | yes            |
| `get_user_behavior_metrics(user)`               | yes            | yes            |
| `generate_business_report(business, period)`    | yes            | yes            |
| `generate_investor_report(investor, period)`    | yes            | yes            |

---

## Example — verifying fallback behavior in tests
```rust
#[test]
fn test_analytics_summary_empty_all_fields_zero() {
    let env = Env::default();
    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    env.mock_all_auths();
    client.set_admin(&admin);

    let (platform, performance) = client.get_analytics_summary();

    assert_eq!(platform.total_invoices, 0);
    assert_eq!(platform.total_volume, 0);
    assert_eq!(performance.error_rate, 0);
    assert_eq!(performance.transaction_success_rate, 0);
}
```

---

## Security assumptions

- Analytics functions are read-only — they do not mutate contract state
- No authorization is required to call analytics query functions
- Results reflect ledger state at the time of the call; there is no caching
- Overflow is impossible due to saturating arithmetic on all accumulations