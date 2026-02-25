# Investment Insurance

## Overview

The investment insurance module enables investors to attach insurance coverage to their investments in the QuickLendX protocol. Insurance provides protection against investment loss with configurable coverage percentages and automatically calculated premiums.

## Architecture

### Core Components

#### InsuranceCoverage Structure
```rust
pub struct InsuranceCoverage {
    pub provider: Address,           // Insurance provider address
    pub coverage_amount: i128,       // Amount covered in base currency
    pub premium_amount: i128,        // Premium charged in base currency
    pub coverage_percentage: u32,    // Coverage as percentage (0-100)
    pub active: bool,                // Whether coverage is currently active
}
```

#### Investment Structure (Extended)
```rust
pub struct Investment {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub amount: i128,
    pub funded_at: u64,
    pub status: InvestmentStatus,
    pub insurance: Vec<InsuranceCoverage>,  // Insurance records
}
```

### Premium Calculation

Insurance premiums are calculated using basis points (1/10,000):

```
DEFAULT_INSURANCE_PREMIUM_BPS = 200  // 2% per annum
coverage_amount = investment_amount * coverage_percentage / 100
premium = coverage_amount * DEFAULT_INSURANCE_PREMIUM_BPS / 10_000

// Minimum premium is 1 if coverage_amount > 0
```

**Example:**
- Investment amount: 10,000 USDC
- Coverage percentage: 80%
- Coverage amount: 8,000 USDC
- Premium: 160 USDC (2% of 8,000)

## Public API

### Add Insurance Coverage

**Function:** `add_investment_insurance`

```rust
pub fn add_investment_insurance(
    env: Env,
    investment_id: BytesN<32>,
    provider: Address,
    coverage_percentage: u32,
) -> Result<(), QuickLendXError>
```

**Parameters:**
- `investment_id`: Unique identifier of the investment
- `provider`: Address of the insurance provider
- `coverage_percentage`: Coverage as percentage (1-100)

**Preconditions:**
- Investment must exist and be in `Active` status
- Coverage percentage must be between 1 and 100
- Caller must be the investment owner (investor)
- Investment cannot already have active insurance

**Behavior:**
1. Validates coverage percentage
2. Calculates coverage amount: `investment_amount * coverage_percentage / 100`
3. Calculates premium using basis points formula
4. Creates InsuranceCoverage record with `active = true`
5. Stores insurance record in investment
6. Emits `InsuranceAdded` event
7. Emits `InsurancePremiumCollected` event

**Security Checks:**
- `investor.require_auth()` - Only the investor can add insurance
- Status validation - Only Active investments can be insured
- Parameter validation - Coverage percentage bounds checked

**Errors:**
- `StorageKeyNotFound` - Investment does not exist
- `InvalidStatus` - Investment is not in Active status
- `InvalidCoveragePercentage` - Coverage percentage < 1 or > 100
- `InvalidAmount` - Calculated premium is zero or invalid
- `OperationNotAllowed` - Investment already has active insurance

### Query Insurance Coverage

**Function:** `query_investment_insurance`

```rust
pub fn query_investment_insurance(
    env: Env,
    investment_id: BytesN<32>,
) -> Result<Vec<InsuranceCoverage>, QuickLendXError>
```

**Parameters:**
- `investment_id`: Unique identifier of the investment to query

**Returns:**
- `Ok(Vec<InsuranceCoverage>)` - All insurance records (active and inactive)
- `Err(StorageKeyNotFound)` - Investment does not exist

**Security Notes:**
- No authorization required - Query function is read-only
- Returns all insurance records regardless of state
- Can be called by any address

**Use Cases:**
1. Display insurance status in UI
2. Calculate total coverage for an investment
3. Retrieve provider information
4. Audit insurance premium history

## Lifecycle

```
Uninsured Investment (Active)
        ↓
    [Add Insurance]
        ↓
Insured Investment (Active + Insurance.active=true)
        ↓
  [On Default/Settlement]
        ↓
Insured Investment (Status Changed + Insurance.active=false)
```

### State Transitions

1. **Initial State**: Investment created, no insurance
   - `investment.insurance.len() = 0`
   - `insurance.active = N/A`

2. **Insurance Added**: Investor attaches insurance
   - `investment.insurance.len() = 1`
   - `insurance.active = true`
   - Premium is locked in coverage amount

3. **Insurance Claimed**: Triggered by default or settlement
   - `investment.insurance.len() = 1`
   - `insurance.active = false`
   - Provider address and coverage amount preserved

## Events

### InsuranceAdded
Emitted when insurance is successfully added to an investment.

```rust
pub fn emit_insurance_added(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    provider: &Address,
    coverage_percentage: u32,
    coverage_amount: i128,
    premium_amount: i128,
)
```

**Event Topics:** `("ins_add",)`

**Data:**
- investment_id: BytesN<32>
- invoice_id: BytesN<32>
- investor: Address
- provider: Address
- coverage_percentage: u32
- coverage_amount: i128
- premium_amount: i128

### InsurancePremiumCollected
Emitted when insurance premium is processed.

```rust
pub fn emit_insurance_premium_collected(
    env: &Env,
    investment_id: &BytesN<32>,
    provider: &Address,
    premium_amount: i128,
)
```

**Event Topics:** `("ins_prm",)`

**Data:**
- investment_id: BytesN<32>
- provider: Address
- premium_amount: i128

### InsuranceClaimed
Emitted when insurance coverage is claimed (on default).

```rust
pub fn emit_insurance_claimed(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    provider: &Address,
    coverage_amount: i128,
)
```

**Event Topics:** `("ins_clm",)`

**Data:**
- investment_id: BytesN<32>
- invoice_id: BytesN<32>
- provider: Address
- coverage_amount: i128

## Validation Rules

### Coverage Percentage Validation

```
✓ Valid:   1 ≤ coverage_percentage ≤ 100
✗ Invalid: coverage_percentage < 1
✗ Invalid: coverage_percentage > 100
```

### Premium Calculation Validation

```
✓ coverage_amount > 0 → premium ≥ 1
✓ coverage_amount = 0 → premium = 0
```

### Investment Status Validation

```
✓ Can add insurance:    InvestmentStatus::Active
✗ Cannot add:           InvestmentStatus::Withdrawn
✗ Cannot add:           InvestmentStatus::Completed
✗ Cannot add:           InvestmentStatus::Defaulted
```

### Single Active Insurance Per Investment

```
✓ Can add:     When no active insurance exists
✗ Cannot add:  When active insurance already exists

Reason: Prevents overlapping coverage and simplifies settlement logic
```

## Security Considerations

### Authorization

1. **Add Insurance**: Only investment owner (investor) can add
   - Enforced via `investor.require_auth()`
   - Prevents unauthorized coverage attachment

2. **Query Insurance**: No authorization required
   - Read-only operation
   - Anyone can query coverage details

### Data Integrity

1. **Immutable Coverage Terms**
   - Once insurance is added, coverage amount cannot be modified
   - Premium is calculated once at creation time

2. **Atomic Operations**
   - Insurance addition is atomic
   - Either fully succeeds or fails - no partial states

3. **Historical Records**
   - Inactive insurance records are preserved
   - Enables audit trail and historical analysis

### Potential Vulnerabilities & Mitigations

| Vulnerability | Mitigation |
|---|---|
| Unauthorized insurance addition | `investor.require_auth()` enforces caller identity |
| Invalid coverage percentages | Input validation (1-100 range) |
| Coverage on inactive investments | Status check before allowing addition |
| Multiple active insurances | `has_active_insurance()` check prevents duplicates |
| Integer overflow in premium calc | Uses `saturating_mul` and `checked_div` |
| Stale coverage data | Vec<> is updated atomically with investment |

## Storage Schema

### Investment Storage Key
```
Key: investment_id (BytesN<32>)
Value: Investment {
    ...
    insurance: Vec<InsuranceCoverage>
}
```

### Investor Index Key
```
Key: ("invst_inv", investor_address)
Value: Vec<investment_id>
```

### Invoice Index Key
```
Key: ("inv_map", invoice_id)
Value: investment_id
```

## Example Usage

### Adding Insurance

```rust
// Investor adds 80% insurance coverage
let coverage_percentage = 80u32;
client.add_investment_insurance(
    &investment_id,
    &insurance_provider_address,
    &coverage_percentage
)?;

// Investment of 10,000 USDC:
// - Coverage amount: 8,000 USDC
// - Premium: 160 USDC (2%)
```

### Querying Insurance

```rust
// Get all insurance records for an investment
let insurance_records = client.query_investment_insurance(&investment_id)?;

for coverage in insurance_records {
    println!("Provider: {}", coverage.provider);
    println!("Coverage: {}%", coverage.coverage_percentage);
    println!("Coverage Amount: {}", coverage.coverage_amount);
    println!("Premium: {}", coverage.premium_amount);
    println!("Active: {}", coverage.active);
}
```

### Checking Coverage Status

```rust
// Get full investment with insurance details
let investment = client.get_investment(&investment_id)?;

if investment.insurance.len() > 0 {
    let coverage = investment.insurance.get(0)?;
    println!("Total Coverage: {}", coverage.coverage_amount);
    println!("Provider: {}", coverage.provider);
}
```

## Testing

### Test Coverage

The insurance module is tested with:

1. **Lifecycle Tests** (`test_investment_insurance_lifecycle`)
   - Insurance addition to active investments
   - Duplicate insurance prevention
   - Default handling with active insurance

2. **Query Tests** (`test_query_investment_insurance_single_coverage`)
   - Empty insurance vector on new investment
   - Insurance retrieval after addition
   - Provider and coverage verification

3. **Edge Case Tests** (`test_query_investment_insurance_nonexistent_investment`)
   - Nonexistent investment handling
   - Proper error propagation

4. **Premium Calculation Tests** (`test_query_investment_insurance_premium_calculation`)
   - Various coverage percentages (50%, 80%, 100%)
   - Correct premium calculation (2% basis)

5. **State Transition Tests** (`test_query_investment_insurance_inactive_coverage`)
   - Active → Inactive transition on default
   - Coverage amount preservation
   - Query consistency

### Running Tests

```bash
# Run all insurance tests
cargo test test_investment_insurance --lib

# Run specific test
cargo test test_query_investment_insurance_single_coverage --lib

# Run with output
cargo test test_investment_insurance_lifecycle --lib -- --nocapture
```

## Future Enhancements

### Phase 2: Settlement Integration
- Automatic payout to insurance providers on default
- Multi-provider insurance support
- Insurance fund management

### Phase 3: Advanced Features
- Variable premium rates based on risk tier
- Multiple active insurances per investment
- Partial insurance claims
- Insurance provider reputation system

### Phase 4: Governance
- Dynamic insurance premium adjustment
- Approved provider registry
- Insurance claim dispute resolution
- Risk-based coverage limits

## Related Modules

- **investment.rs** - Core investment data structures
- **settlement.rs** - Handles invoice payment and default scenarios
- **events.rs** - Event emission and logging
- **errors.rs** - Error types and handling
- **defaults.rs** - Default handling triggers insurance claims

## References

- [Invoice Lifecycle](./invoice-lifecycle.md)
- [Settlement](./settlement.md)
- [Default Handling](./default-handling.md)
- [Protocol Limits](./protocol-limits.md)
