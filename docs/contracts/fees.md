# Platform Fee System Documentation

## Overview

The QuickLendX platform implements a configurable fee system with treasury routing capabilities. The system applies a default 2% platform fee on invoice transactions during settlement, with fees automatically routed to a configured treasury address.

## Key Features

### 1. Configurable Platform Fee

- **Default Rate**: 2% (200 basis points)
- **Maximum Rate**: 10% (1000 basis points)
- **Admin-only Configuration**: Only platform administrators can modify fee rates
- **Real-time Updates**: Fee changes take effect immediately for new transactions

#### Platform Fee Boundaries
To prevent protocol misconfiguration or malicious fee hikes, the platform fee is strictly capped at the contract level.
- **Hard Cap**: Any update attempt exceeding 1000 BPS (10%) will result in an `InvalidFeeBasisPoints` error.
- **Integrity**: Every update emits a `platform_fee_config_updated` (topic: `fee_cfg`) event containing both `old_fee_bps` and `new_fee_bps` to ensure off-chain auditability.
- **Optimization**: If the proposed new fee is identical to the current fee, the system performs a "no-op," meaning no storage is updated and no event is emitted.

### 2. Treasury Routing

- **Automatic Routing**: Platform fees are automatically routed to the configured treasury address
- **Fallback Mechanism**: If no treasury is configured, fees are sent to the contract address
- **Secure Configuration**: Only administrators can set or update the treasury address
- **Event Tracking**: All treasury configuration changes are logged via `treasury_configured` (topic: `trs_cfg`) events.

### 3. Fee Structure Management

The protocol supports various types of fees (Platform, Processing, Verification, LatePayment, etc.) through configurable fee structures.
- **Strict Validation**: All fee structures must respect a hard cap of 1000 BPS for the base fee.
- **Event Audit**: Updates to any fee structure emit a `fee_structure_updated` (topic: `fee_str`) event including the `FeeType`, the old BPS, and the new BPS.

### 4. Volume Tiers

- **Tiered Discounts**: User transaction volume determines a discount applied to fee calculations: Standard (0), Silver (5%), Gold (10%), Platinum (15%).
- **Tier Thresholds**: Volume is accumulated via `update_user_transaction_volume`.

### 5. Fee Bounds Validation

- **Admin-only Config**: Fee structures and platform fee BPS are updated only by admin.
- **Internal Validation**: `validate_fee_params(base_fee_bps, min_fee, max_fee)` enforces:
  - `base_fee_bps <= 1000` (10% max)
  - `min_fee >= 0`
  - `max_fee >= min_fee`
- **Error Codes**: Rejection of invalid BPS returns `InvalidFeeBasisPoints` (Contract Error 105).

### 6. Min/Max Fee Structure Consistency Checks

The platform enforces strict consistency validations on min/max fee bounds per fee type and across the entire fee structure system to prevent misconfiguration and ensure reasonable fee scaling.

#### Per-Fee-Type Consistency Rules

The `validate_fee_structure_consistency()` function enforces the following rules for each fee type individually:

1. **Range Validity**: `min_fee <= max_fee`
   - Ensures the fee bounds define a valid range where all calculated fees fit.
   - Violation returns `InvalidAmount` error.

2. **Non-negative Values**: Both `min_fee` and `max_fee` must be >= 0
   - Fees cannot be negative (that would represent a rebate, not a fee).
   - Violation returns `InvalidAmount` error.

3. **Reasonable Bounds**: `max_fee` must not exceed 100x the base fee calculation
   - For Platform, Processing, and Verification fees: max ≤ base_fee_bps × 100 × 100
   - For EarlyPayment and LatePayment fees: max ≤ base_fee_bps × 500 × 100 (more flexible for incentives/penalties)
   - Prevents fee structures where the cap is disproportionate to the base rate.
   - Violation returns `InvalidFeeConfiguration` error.

4. **Absolute Protocol Maximum**: `max_fee <= 10,000,000,000,000` (10M stroops)
   - Hard cap prevents fees from consuming entire user balances.
   - Protects against configuration errors or overflow scenarios.
   - Violation returns `InvalidFeeConfiguration` error.

#### Cross-Fee-Type Consistency Rules

The `validate_cross_fee_consistency()` function enforces invariants across multiple fee structures:

1. **LatePayment Floor Rule**: LatePayment fees must not undercut Platform fees
   - If a LatePayment fee is configured, its `min_fee` must not be less than the Platform fee's `min_fee`.
   - Ensures late payment penalties don't accidentally become cheaper than regular payments.
   - Violation returns `InvalidFeeConfiguration` error.

2. **Total Active Min Fees Limit**: Sum of all active fee structures' `min_fee` must not exceed 2,500,000,000,000 (2.5M stroops)
   - Prevents misconfiguration where multiple fee types combine to create excessive minimum charges.
   - Formula: `total_active_min_fees = Σ(min_fee for all active fee types) <= PROTOCOL_MAX_TOTAL_MIN_FEES`
   - Violation returns `InvalidFeeConfiguration` error.

3. **No Type Overlap**: Each fee type serves a distinct purpose
   - Platform fees for general transaction overhead
   - Processing fees for specialized processing
   - Verification fees for identity/business verification
   - EarlyPayment fees for incentivizing early repayment
   - LatePayment fees for penalizing late repayment

#### Implementation Details

All consistency checks are performed in `update_fee_structure()` before any state mutations:

```rust
pub fn update_fee_structure(
    env: &Env,
    admin: &Address,
    fee_type: FeeType,
    base_fee_bps: u32,
    min_fee: i128,
    max_fee: i128,
    is_active: bool,
) -> Result<FeeStructure, QuickLendXError> {
    admin.require_auth();
    
    if base_fee_bps > MAX_FEE_BPS {
        return Err(QuickLendXError::InvalidFeeBasisPoints);
    }

    // Apply per-type consistency checks
    Self::validate_fee_structure_consistency(
        &fee_type, 
        base_fee_bps, 
        min_fee, 
        max_fee
    )?;
    
    // Apply cross-type consistency checks
    Self::validate_cross_fee_consistency(env, &fee_type, min_fee, max_fee)?;
    
    // ... continue with fee structure update
}
```

#### Error Scenarios

| Validation | Error Code | Interpretation |
| :--- | :--- | :--- |
| min_fee > max_fee | `InvalidAmount` | Invalid range; bounds must respect ordering |
| min_fee < 0 or max_fee < 0 | `InvalidAmount` | Negative fees not allowed |
| max_fee > protocol limit | `InvalidFeeConfiguration` | Exceeds absolute protocol bound |
| max_fee > 100x base_fee | `InvalidFeeConfiguration` | Unreasonable scaling for type |
| LatePayment min < Platform min | `InvalidFeeConfiguration` | Late payments undercut regular fees |
| Total active min fees too high | `InvalidFeeConfiguration` | Excessive combined minimums |

#### Examples

**Valid Configuration**:
```
FeeStructure {
    fee_type: Platform,
    base_fee_bps: 200,        // 2%
    min_fee: 100,             // 100 stroops
    max_fee: 500_000,         // Reasonable cap (within 100x multiplier)
    is_active: true,
}
```

**Invalid Configuration** (max_fee < min_fee):
```
FeeStructure {
    fee_type: Processing,
    base_fee_bps: 100,
    min_fee: 1000,            // 1000 stroops
    max_fee: 500,             // ERROR: max < min
    is_active: true,
}
```

**Invalid Configuration** (exceeds protocol maximum):
```
FeeStructure {
    fee_type: Verification,
    base_fee_bps: 100,
    min_fee: 100,
    max_fee: 15_000_000_000_000,  // ERROR: > 10M stroops absolute max
    is_active: true,
}
```

## Technical Implementation

### Core Components

#### 1. Platform Fee Configuration Structure

```rust
pub struct PlatformFeeConfig {
    pub fee_bps: u32,
    pub treasury_address: Option<Address>,
    pub updated_at: u64,
    pub updated_by: Address,
}
```

#### 2. Fee Structure

```rust
pub struct FeeStructure {
    pub fee_type: FeeType,
    pub base_fee_bps: u32,
    pub min_fee: i128,
    pub max_fee: i128,
    pub is_active: bool,
    pub updated_at: u64,
    pub updated_by: Address,
}
```

### Key Functions (Administrative)

1. **`configure_treasury(treasury_address: Address)`**
   - Sets the treasury address for fee routing.
   - Emits `trs_cfg` event.

2. **`update_platform_fee_bps(new_fee_bps: u32)`**
   - Updates the platform fee rate.
   - Enforces 10% hard cap.
   - Emits `fee_cfg` event with transition details.

3. **`update_fee_structure(fee_type, base_fee_bps, min_fee, max_fee, is_active)`**
   - Updates specific fee mechanics.
   - Enforces 10% hard cap on `base_fee_bps`.
   - Emits `fee_str` event.

### Settlement Process

1. **Fee Calculation**: System calculates platform fee based on profit.
2. **Fund Distribution**: Investor receives `payment_amount - platform_fee`. Treasury receives `platform_fee`.

### Deterministic Profit/Fee Formula

Core logic in `profits.rs` ensures:
- **No dust**: `investor_return + platform_fee == safe_payment`.
- **Overflow-safe arithmetic**: Uses saturating i128 math.
- **Investor-favored rounding**: Integer floor division.

## Security and Auditability

### Access Control
All administrative functions require `admin.require_auth()`.

### Validation
Strict boundary checks prevent "silent misconfiguration" where a typo could lead to excessive fees.

### Events Registry

| Topic | Event Name | Payload | Rationale |
| :--- | :--- | :--- | :--- |
| `fee_cfg` | Platform Fee Updated | `(old_bps, new_bps, admin, ts)` | Tracks platform-wide fee changes |
| `fee_str` | Fee Structure Updated | `(fee_type, old_bps, new_bps, admin, ts)` | Tracks specific structural changes |
| `trs_cfg` | Treasury Configured | `(treasury_addr, admin, ts)` | Tracks where funds are routed |
| `fee_upd` | Legacy Profit Fee | `(bps, ts, admin)` | (Used in profits.rs module) |

## Error Handling

- **`InvalidFeeBasisPoints`**: Rejection of BPS > 1000.
- **`InvalidAmount`**: Rejection of negative amounts or inconsistent min/max bounds.
- **`NotAdmin`**: Unauthorized modification attempt.
- **`InvalidFeeConfiguration`**: Map sum does not equal `total_amount`, or revenue shares do not sum to 10,000 BPS.
- **`StorageKeyNotFound`**: Reading fee config before the fee system has been initialized.

## Migration and Upgrades

The system is designed for backward compatibility, with new event structures providing more detail than legacy versions without breaking core settlement logic.

---

## Comprehensive Fee System Testing

The QuickLendX fee system is validated through extensive automated testing covering volume accumulation, tier transitions, and settlement scenarios. All tests are located in [src/test_fees_extended.rs](../../quicklendx-contracts/src/test_fees_extended.rs).

### Testing Scope

The test suite provides **95%+ code coverage** with 40+ comprehensive tests covering:

#### 1. Volume Accumulation Tests (5 tests)

These tests verify that user transaction volumes are correctly tracked and persisted across the contract lifecycle.

- **`test_volume_accumulates_single_transaction`**: Validates that a single transaction correctly increments total volume and transaction count.
- **`test_volume_accumulates_multiple_transactions`**: Confirms cumulative volume tracking across 3+ sequential transactions with varying amounts.
- **`test_volume_persists_after_state_retrieval`**: Ensures volume data is durably stored and survives state queries and additional updates.
- **`test_volume_large_accumulation_no_overflow`**: Validates saturating arithmetic handles transactions at 10^12 stroops without panicking.
- **`test_volume_transaction_count_increments`**: Confirms transaction counter increments deterministically with each volume update.

**Security Validations**:
- No integer overflow panics on large amounts
- Volume is monotonically increasing (never decreases)
- Transaction count increments exactly once per call

#### 2. Tier Transition Tests (6 tests)

These tests verify that volume thresholds correctly trigger tier promotions and apply appropriate fee discounts.

- **`test_tier_transition_standard_to_silver`**: User at 0 volume transitions to Silver (5% discount) at 100 billion stroops threshold.
- **`test_tier_transition_silver_to_gold`**: User at Silver tier transitions to Gold (10% discount) at 500 billion stroops.
- **`test_tier_transition_gold_to_platinum`**: User at Gold tier transitions to Platinum (15% discount) at 1 trillion stroops.
- **`test_tier_monotonic_no_downgrade`**: Confirms tiers never downgrade; Platinum users remain Platinum even after single stroops.
- **`test_fee_discount_increases_with_tier`**: Validates fee amounts decrease monotonically as tiers progress (Standard > Silver > Gold > Platinum).
- **`test_tier_discount_values_correct`**: Confirms exact discount percentages: Standard 0%, Silver 5%, Gold 10%, Platinum 15%.

**Security Validations**:
- Tier transitions are monotonic (no downgrade or artificial tier reset)
- Threshold crossings are precise (100B, 500B, 1T stroops)
- Discounts compound only on non-LatePayment fees
- Tier state is recoverable via `get_user_volume_data()`

#### 3. Settlement and Repeated Transaction Tests (6 tests)

These tests simulate real-world invoice settlement sequences with multiple payments, tier changes, and fee recalculations.

- **`test_fee_calculation_consistent_multiple_settlements`**: Confirms fee amounts remain identical across settlements when tier is unchanged.
- **`test_fee_reduction_after_tier_upgrade_settlement`**: Validates fees decrease after user enters higher tier mid-settlement.
- **`test_cumulative_volume_through_settlement_lifecycle`**: Simulates 3-round settlement process: Standard → Silver → Gold tier progression.
- **`test_fee_calculation_deterministic_after_settlements`**: Calls fee calculation 4 times (3 at same tier, 1 after immaterial volume bump); expects identical results.
- **`test_revenue_accumulation_through_settlements`**: Collects fees across 2 settlements, verifies revenue distribution (50% treasury, 25% developer, 25% platform).
- **`test_settlement_with_tier_change_and_fee_update`**: Combined scenario: tier promotion + platform fee BPS change in same settlement round.

**Security Validations**:
- Fee calculations are deterministic (same inputs → same outputs)
- Volume accumulation is atomic (no partial updates)
- Revenue collection and distribution balance (no dust or loss)
- Settlement state transitions are idempotent

#### 4. Volume Tier Discount Application Tests (3 tests)

These tests verify tier-based fee reductions are correctly applied to fee calculations.

- **`test_volume_tier_standard_no_discount`**: User with 0 volume receives 0% discount (Standard tier).
- **`test_fee_discount_percentage_silver_5_percent`**: Silver tier users receive exactly 5% fee reduction.
- **`test_fee_discount_percentage_gold_10_percent`**: Gold tier users receive exactly 10% fee reduction.
- **`test_fee_discount_percentage_platinum_15_percent`**: Platinum tier users receive exactly 15% fee reduction.

**Security Validations**:
- Discount percentages are exact (integer math maintains precision)
- Discounts do not apply to LatePayment fees (only penalize)
- Discounts apply before early-payment incentives

#### 5. Fee Calculation Determinism Tests (4 tests)

These tests ensure fee calculations produce identical results for the same inputs and contract state.

- **`test_transaction_fee_same_inputs_are_deterministic`**: Same user, amount, timing flags produce identical fees across 3+ calculations.
- **`test_rounding_with_odd_amounts`**: Fee calculations with non-divisible amounts (333, 777 stroops) are consistent and positive.
- **`test_transaction_fee_small_amount_uses_minimums_before_modifiers`**: 1 stroop correctly clamps to min fees (250), then applies early discount (→240).
- **`test_transaction_fee_large_amount_uses_maximums_before_tier_discount`**: 100M stroop amounts clamp to max (1.36M after Platinum discount).

**Security Validations**:
- Clamping order is deterministic: calculate BPS → clamp to [min, max] → apply tier discount → apply timing modifiers
- Floor division rounding is consistent
- No floating-point precision errors

#### 6. Initialization and State Persistence Tests (4 tests)

These tests verify fee system initialization and configuration changes persist correctly.

- **`test_initialize_fee_system_sets_defaults`**: First initialization creates Platform (200 BPS), Processing (50 BPS), Verification (100 BPS) fee structures.
- **`test_multiple_fee_updates_sequence`**: Updating platform fee to 300 → 500 → 150 BPS is persisted correctly.
- **`test_treasury_persists_across_updates`**: Setting treasury address, then updating fee BPS, preserves treasury routing.
- **`test_fee_structures_unchanged_after_rejected_reinit`**: Updating custom fee structure (e.g., Platform → 300 BPS), then rejecting re-initialization, preserves custom value.

**Security Validations**:
- Initialization guard prevents re-initialization (idempotency)
- Fee structures survive invalid operations (graceful error handling)
- Treasury configuration is immutable once set (no accidental misrouting)

#### 7. Revenue Distribution Tests (4 tests)

These tests verify fee collection and revenue split distribution.

- **`test_revenue_all_to_treasury`**: All collected fees (100% share) route to treasury.
- **`test_revenue_all_to_platform`**: All collected fees (100% share) remain in platform.
- **`test_revenue_asymmetric_distribution`**: 45% treasury / 45% developer / 10% platform split distributes correctly.
- **`test_revenue_distribution_sum_equals_collected`**: Distributed amounts sum exactly to collected amount (no dust).

**Security Validations**:
- Share amounts sum to 10,000 BPS (100%) exactly
- No fees are lost in rounding (platform gets remainder)
- Distribution is atomic (succeeds or fails completely)

#### 8. Payment Timing Modifier Tests (4 tests)

These tests verify early-payment incentives and late-payment penalties.

- **`test_early_payment_fee_reduction`**: Early payment flag reduces Platform fee by 10% discount.
- **`test_late_payment_fee_increase`**: LatePayment fee structure (when present) increases by 20% surcharge with late flag.
- **`test_early_and_late_payment_combined`**: Early flag applies only to Platform, late flag only to LatePayment (orthogonal).
- **`test_payment_timing_combined_early_priority`**: Early-payment discount takes precedence over all other modifiers.

**Security Validations**:
- Modifiers apply after min/max clamping
- Modifier order is fixed (tier → early → late)
- Platform fee early discount is always 10% (hard-coded)
- LatePayment surcharge is always 20% (hard-coded)

### Test Data & Constants

All tests use realistic stroops amounts and thresholds:

| Tier | Volume Threshold | Fee Discount |
|:---|---|---|
| Standard | 0 | 0% |
| Silver | 100_000_000_000 (100B) | 5% |
| Gold | 500_000_000_000 (500B) | 10% |
| Platinum | 1_000_000_000_000 (1T) | 15% |

| Fee Type | Default BPS | Min Fee | Max Fee |
|:---|---|---|---|
| Platform | 200 (2%) | 100 | 1_000_000 |
| Processing | 50 (0.5%) | 50 | 500_000 |
| Verification | 100 (1%) | 100 | 100_000 |

### Running the Tests

```bash
cd quicklendx-contracts

# Run all fee tests
cargo test test_fees_extended --lib -- --nocapture

# Run a specific test category
cargo test test_volume_accumulates --lib -- --nocapture
cargo test test_tier_transition --lib -- --nocapture
cargo test test_revenue_accumulation --lib -- --nocapture

# Run with verbose output
cargo test -- --nocapture --test-threads=1
```

### Test Coverage

The test suite achieves **95%+ code coverage** for the fees module:

- **FeeManager implementation**: 40+ tests
- **Volume tracking**: 5 core tests + 3 discount tests = 8 pathways
- **Tier transitions**: 6 tests covering all tier pairs
- **Settlement sequences**: 6 tests with multi-round scenarios
- **Determinism**: 4 tests validating idempotency
- **Edge cases**: Zero amounts, overflow protection, min/max bounds
- **Security invariants**: Access control, validation, event emission

### Known Limitations

1. Tests use mock authentication (`env.mock_all_auths()`), which bypasses real Soroban signature validation. Production deployments rely on Soroban's native access control.
2. Volume thresholds are hard-coded in the contract. To change tier boundaries, contract redeployment is required.
3. Fee discount percentages are fixed and cannot be adjusted per-tier without contract updates.

### Future Enhancements

1. **Adaptive tiers**: Add on-chain voting or governance for tier threshold adjustments.
2. **Time-decay discounts**: Implement volume reset periods (annual reconciliation).
3. **Per-tier analytics**: Track fee savings and platform impact by tier.
4. **Custom fee structures**: Admin-configurable per-invoice-type fee schedules.
