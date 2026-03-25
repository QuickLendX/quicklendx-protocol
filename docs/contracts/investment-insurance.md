# Investment Insurance

## Overview

The investment insurance module enables investors to attach insurance coverage to their
investments in the QuickLendX protocol.  Insurance provides configurable protection against
investment loss with strictly validated parameters and automatically bounded premium/coverage
math.

---

## Architecture

### Core Components

#### Constants (`investment.rs`)

| Constant | Value | Purpose |
|---|---|---|
| `DEFAULT_INSURANCE_PREMIUM_BPS` | `200` | 2 % premium rate in basis points (1/10 000) |
| `MIN_COVERAGE_PERCENTAGE` | `1` | Lowest valid coverage percentage (inclusive) |
| `MAX_COVERAGE_PERCENTAGE` | `100` | Highest valid coverage percentage (inclusive) |
| `MIN_PREMIUM_AMOUNT` | `1` | Minimum acceptable premium in base currency units |

#### `InsuranceCoverage` structure

```rust
pub struct InsuranceCoverage {
    pub provider:            Address,  // Insurance provider address
    pub coverage_amount:     i128,     // Amount covered in base currency units
    pub premium_amount:      i128,     // Premium charged in base currency units
    pub coverage_percentage: u32,      // Coverage as integer percentage (1–100)
    pub active:              bool,     // Whether this coverage record is active
}
```

#### `Investment` structure (relevant excerpt)

```rust
pub struct Investment {
    pub investment_id: BytesN<32>,
    pub invoice_id:    BytesN<32>,
    pub investor:      Address,
    pub amount:        i128,
    pub funded_at:     u64,
    pub status:        InvestmentStatus,
    pub insurance:     Vec<InsuranceCoverage>,  // All insurance records (active + historical)
}
```

---

## Premium Calculation

Insurance premiums are calculated by `Investment::calculate_premium` using basis points:

```
coverage_amount = investment_amount × coverage_percentage / 100
premium         = coverage_amount   × DEFAULT_INSURANCE_PREMIUM_BPS / 10_000
```

Both multiplications use `saturating_mul`; division uses `checked_div` to prevent
overflow and division-by-zero panics.

**Minimum premium floor:** if the computed `premium` is less than `MIN_PREMIUM_AMOUNT`
but `coverage_amount > 0`, the function returns `MIN_PREMIUM_AMOUNT` (1) instead of 0.
This ensures zero-premium insurance is impossible.

**Example:**

| Investment | Coverage % | Coverage Amount | Premium |
|---|---|---|---|
| 10 000 USDC | 80 % | 8 000 USDC | 160 USDC |
| 10 000 USDC | 100 % | 10 000 USDC | 200 USDC |
| 10 000 USDC | 1 % | 100 USDC | 2 USDC |
| 100 USDC | 1 % | 1 USDC | 1 USDC *(floor)* |

---

## Public API

### `add_investment_insurance`

```rust
pub fn add_investment_insurance(
    env:                Env,
    investment_id:      BytesN<32>,
    provider:           Address,
    coverage_percentage: u32,
) -> Result<(), QuickLendXError>
```

**Parameters:**

| Name | Type | Constraints |
|---|---|---|
| `investment_id` | `BytesN<32>` | Must identify an existing, Active investment |
| `provider` | `Address` | Insurance provider; no whitelist enforced at contract level |
| `coverage_percentage` | `u32` | `MIN_COVERAGE_PERCENTAGE (1)` ≤ value ≤ `MAX_COVERAGE_PERCENTAGE (100)` |

**Validation order (fail-fast):**

1. Investment must exist → `StorageKeyNotFound`
2. Caller must be the investment owner → auth panic
3. Investment status must be `Active` → `InvalidStatus`
4. `coverage_percentage` must be in `[MIN_COVERAGE_PERCENTAGE, MAX_COVERAGE_PERCENTAGE]`
   → `InvalidCoveragePercentage`
5. Computed premium must be ≥ `MIN_PREMIUM_AMOUNT` (investment too small otherwise)
   → `InvalidAmount`
6. `add_insurance` re-validates all bounds independently (defense-in-depth)
7. No currently active coverage may exist → `OperationNotAllowed`

**On success:**

1. Computes `coverage_amount = amount × coverage_percentage / 100`
2. Computes `premium` via `calculate_premium`
3. Pushes an `InsuranceCoverage { active: true }` record onto the investment
4. Persists the updated investment
5. Emits `InsuranceAdded` event
6. Emits `InsurancePremiumCollected` event

**Error table:**

| Error | Condition |
|---|---|
| `StorageKeyNotFound` | Investment does not exist |
| `InvalidStatus` | Investment is not in `Active` state |
| `InvalidCoveragePercentage` | `coverage_percentage < 1` or `> 100` |
| `InvalidAmount` | Computed premium is zero (investment amount too small), investment principal ≤ 0, coverage amount exceeds principal, or premium > coverage amount |
| `OperationNotAllowed` | An active insurance record already exists on this investment |

---

### `query_investment_insurance`

```rust
pub fn query_investment_insurance(
    env:           Env,
    investment_id: BytesN<32>,
) -> Result<Vec<InsuranceCoverage>, QuickLendXError>
```

Returns **all** insurance records (active and inactive) ordered by insertion time.
No authorization required — read-only operation.

**Errors:** `StorageKeyNotFound` if the investment does not exist.

---

## Lifecycle

```
Uninsured Investment (Active)
          │
          ▼ add_investment_insurance(…)
Insured Investment (Active + insurance[n].active = true)
          │
          ▼ process_insurance_claim() on default/settlement
Claimed   (insurance[n].active = false, provider/amount preserved)
          │
          ▼ add_investment_insurance(…)  [optional — new policy]
Re-insured (Active + insurance[n+1].active = true)
```

### State Transitions

| # | State | `insurance.len()` | `active` flag |
|---|---|---|---|
| 1 | Investment created | 0 | N/A |
| 2 | Insurance added | 1 | `true` |
| 3 | Insurance claimed | 1 | `false` |
| 4 | Second policy added (after claim) | 2 | `true` (index 1) |

---

## Validation Rules

### Coverage percentage

```
✓ Valid:   MIN_COVERAGE_PERCENTAGE (1) ≤ coverage_percentage ≤ MAX_COVERAGE_PERCENTAGE (100)
✗ Invalid: coverage_percentage = 0        → InvalidCoveragePercentage
✗ Invalid: coverage_percentage > 100      → InvalidCoveragePercentage
```

> **Why the explicit upper-bound check?**  Without it, a caller supplying
> `coverage_percentage = 200` would compute `coverage_amount = 2 × principal`,
> giving the claimant twice the invested amount — a direct over-coverage exploit.
> The bound is enforced **before** any arithmetic in both `lib.rs` and `add_insurance`
> (defense-in-depth).

### Investment principal

```
✓ Valid:   investment.amount > 0
✗ Invalid: investment.amount ≤ 0 → InvalidAmount
```

### Premium

```
✓ Valid:   premium ≥ MIN_PREMIUM_AMOUNT (1)
✗ Invalid: premium < 1            → InvalidAmount   (zero-premium exploit)
✗ Invalid: premium > coverage_amount → InvalidAmount (economic inversion)
```

### Coverage-amount invariant

```
✓ Invariant: coverage_amount ≤ investment.amount
```

Guaranteed analytically when `coverage_percentage ≤ 100`, but explicitly
re-checked after arithmetic as a defense-in-depth safeguard.

### Single active insurance

```
✓ Can add:    When no active insurance record exists
✗ Cannot add: When any insurance record has active = true → OperationNotAllowed
```

---

## Security Considerations

### Authorization

| Operation | Authorization |
|---|---|
| `add_investment_insurance` | `investor.require_auth()` — only the investment owner |
| `query_investment_insurance` | None required — read-only |

### Overflow and arithmetic safety

| Risk | Mitigation |
|---|---|
| Overflow in `coverage_amount` | `saturating_mul` — result saturates at `i128::MAX` |
| Division by zero | `checked_div(100)` and `checked_div(10_000)` — `unwrap_or(0)` |
| Over-coverage via large `coverage_percentage` | Explicit range check before arithmetic |
| Negative investment amount | `self.amount > 0` guard in `add_insurance` |
| Premium exceeding coverage | `premium ≤ coverage_amount` guard in `add_insurance` |

### Vulnerability matrix

| Vulnerability | Mitigation |
|---|---|
| Unauthorized insurance addition | `investor.require_auth()` |
| Over-coverage exploit (`coverage_percentage > 100`) | Explicit range check in `lib.rs` and `add_insurance` before any multiplication |
| Zero-premium free insurance | `MIN_PREMIUM_AMOUNT` floor in `calculate_premium`; `premium < MIN_PREMIUM_AMOUNT` check in `add_insurance` |
| Double-coverage / concurrent active policies | `has_active_insurance()` check; `OperationNotAllowed` on duplicate |
| Insurance on settled/defaulted investment | Status check → `InvalidStatus` |
| Negative or zero investment principal | Explicit `self.amount > 0` guard |
| Economic inversion (premium > coverage) | `premium > coverage_amount` guard |
| Integer overflow | `saturating_mul` + `checked_div` throughout |

---

## Storage Schema

### Investment record (insurance embedded)

```
Key:   investment_id  (BytesN<32>)
Value: Investment {
           …
           insurance: Vec<InsuranceCoverage>
       }
```

### Investor index

```
Key:   ("invst_inv", investor_address)
Value: Vec<investment_id>
```

### Invoice index

```
Key:   ("inv_map", invoice_id)
Value: investment_id
```

---

## Example Usage

### Adding 80 % insurance coverage

```rust
client.add_investment_insurance(
    &investment_id,
    &insurance_provider_address,
    &80u32,
)?;
// For a 10 000 USDC investment:
//   coverage_amount = 8 000 USDC
//   premium         =   160 USDC  (2 %)
```

### Querying all insurance records

```rust
let records = client.query_investment_insurance(&investment_id)?;
for cov in records.iter() {
    // cov.coverage_percentage, cov.coverage_amount, cov.premium_amount, cov.active
}
```

---

## Testing

### Test file: `src/test_insurance.rs`

The test suite is organised into 13 sections with ≥ 95 % branch coverage:

| # | Section | Key assertions |
|---|---|---|
| 1 | Bounds constants | Numeric values match documentation |
| 2 | `calculate_premium` pure math | Typical cases, zero/negative/out-of-range inputs, min floor, overflow safety, invariants |
| 3 | Authorization | Auth violation panics without `mock_all_auths` |
| 4 | Status validation | All non-Active statuses → `InvalidStatus` |
| 5 | Coverage-percentage validation | 0 and >100 → `InvalidCoveragePercentage`; 1 and 100 accepted |
| 6 | Investment-amount validation | 0, negative, tiny-amount-rounds-to-0 → `InvalidAmount` |
| 7 | Active-insurance guard | Duplicate active rejected; new policy accepted after deactivation |
| 8 | Premium correctness | Stored `premium_amount` matches calculation; minimum floor |
| 9 | Over-coverage prevention | `coverage_amount ≤ principal`; 101–u32::MAX rejected with specific error |
| 10 | Query correctness | Empty for new investment; all historical entries returned; non-existent → error |
| 11 | Claim logic | Deactivates coverage; returns provider + amount; second claim → None |
| 12 | Cross-investment isolation | Adding insurance to A does not affect B |
| 13 | Direct `add_insurance` unit tests | All validation paths without contract dispatch |

### Running the tests

```bash
# All insurance tests
cargo test test_insurance --lib

# Specific test
cargo test test_over_100_percent_rejected_with_specific_error --lib

# With log output
cargo test test_insurance --lib -- --nocapture
```

---

## Related Modules

- [investment.rs](../../quicklendx-contracts/src/investment.rs) — Data structures and business logic
- [settlement.rs](./settlement.md) — Triggers `process_insurance_claim` on default
- [events.rs](../../quicklendx-contracts/src/events.rs) — `emit_insurance_added`, `emit_insurance_premium_collected`, `emit_insurance_claimed`
- [errors.rs](../../quicklendx-contracts/src/errors.rs) — Error codes
- [defaults.md](./default-handling.md) — Default-handling flow that invokes insurance claims
