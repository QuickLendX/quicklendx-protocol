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

## FeeType Configuration Matrix

### Overview

Every `FeeType` variant is exercised independently through `update_fee_structure`.
The matrix below captures all boundary and validity rules for each type.

### FeeType Variants

| Variant | Default in storage | Max-fee multiplier | Notes |
|---|---|---|---|
| `Platform` | Yes (bps=200, min=100, max=1_000_000) | 100× | General transaction overhead |
| `Processing` | Yes (bps=50, min=50, max=500_000) | 100× | Specialized processing charge |
| `Verification` | Yes (bps=100, min=100, max=100_000) | 100× | Identity/business verification |
| `EarlyPayment` | **No** — inserted on first call | 500× | Incentive; wider ceiling |
| `LatePayment` | **No** — inserted on first call | 500× | Penalty; subject to cross-fee floor rule |

### BPS Boundaries (applies to all variants)

| BPS value | Result | Error |
|---|---|---|
| 0 | Accepted (free / no-op rate) | — |
| 1 – 999 | Accepted | — |
| 1000 | Accepted (hard cap) | — |
| ≥ 1001 | Rejected | `InvalidFeeBasisPoints` |

### Min/Max Ordering Rules (applies to all variants)

| Condition | Result | Error |
|---|---|---|
| `min_fee == max_fee` | Accepted (flat fee) | — |
| `min_fee < max_fee` | Accepted | — |
| `min_fee > max_fee` | Rejected | `InvalidAmount` |
| `min_fee < 0` | Rejected | `InvalidAmount` |
| `max_fee < 0` | Rejected | `InvalidAmount` |

### Absolute Max-Fee Limit

`max_fee > 10_000_000_000_000` (10 M stroops) is always rejected with `InvalidFeeConfiguration`.

### Per-Type Max-Fee Threshold

For a given `base_fee_bps`, the per-type ceiling is:

| Type group | Formula | Example (bps=200) |
|---|---|---|
| Platform / Processing / Verification | `bps × 100 × 100` | 2_000_000 |
| EarlyPayment / LatePayment | `bps × 500 × 100` | 10_000_000 |

When `base_fee_bps = 0` the formula yields 0 and the per-type threshold guard is
**skipped** (condition `calculated_max_threshold > 0` is false), so any
`max_fee` up to the absolute protocol cap is accepted with a zero-bps structure.

### Cross-Fee Floor Rule (LatePayment only)

`LatePayment.min_fee` must be **≥** the currently stored `Platform.min_fee`.

| Scenario | Result | Error |
|---|---|---|
| `LatePayment.min_fee >= Platform.min_fee` | Accepted | — |
| `LatePayment.min_fee == Platform.min_fee` | Accepted (exact boundary) | — |
| `LatePayment.min_fee < Platform.min_fee` | Rejected | `InvalidFeeConfiguration` |

### Insert vs. Update Behaviour

`update_fee_structure` performs an **upsert**:

- If the `fee_type` already exists in storage the existing record is overwritten.
- If the `fee_type` is absent a new record is appended.

`Platform`, `Processing`, and `Verification` are inserted during
`initialize_fee_system`, so their first user-driven call takes the **update**
path. `EarlyPayment` and `LatePayment` are absent until explicitly created,
so their first call takes the **insert** path; `get_fee_structure` returns
`StorageKeyNotFound` before that point.

### Isolation Guarantee

Updating one `FeeType` must not alter any other type's `base_fee_bps`,
`min_fee`, or `max_fee`. This is verified in
`test_matrix_update_platform_preserves_others` and
`test_matrix_insert_early_payment_preserves_existing_types`.

### Test Coverage Map

| Test name | Coverage |
|---|---|
| `test_matrix_platform_bps_zero_accepted` | Platform, bps=0 |
| `test_matrix_platform_bps_at_cap_accepted` | Platform, bps=1000 |
| `test_matrix_platform_bps_over_cap_rejected` | Platform, bps=1001 |
| `test_matrix_processing_bps_zero_accepted` | Processing, bps=0 |
| `test_matrix_processing_bps_at_cap_accepted` | Processing, bps=1000 |
| `test_matrix_processing_bps_over_cap_rejected` | Processing, bps=1001 |
| `test_matrix_verification_bps_zero_accepted` | Verification, bps=0 |
| `test_matrix_verification_bps_at_cap_accepted` | Verification, bps=1000 |
| `test_matrix_verification_bps_over_cap_rejected` | Verification, bps=1001 |
| `test_matrix_early_payment_bps_zero_accepted` | EarlyPayment, bps=0, insert |
| `test_matrix_early_payment_bps_at_cap_accepted` | EarlyPayment, bps=1000 |
| `test_matrix_early_payment_bps_over_cap_rejected` | EarlyPayment, bps=1001 |
| `test_matrix_late_payment_bps_zero_accepted` | LatePayment, bps=0, cross-floor |
| `test_matrix_late_payment_bps_at_cap_accepted` | LatePayment, bps=1000 |
| `test_matrix_late_payment_bps_over_cap_rejected` | LatePayment, bps=1001 |
| `test_matrix_platform_returned_fields_complete` | Returned struct correctness |
| `test_matrix_processing_returned_fields_complete` | Returned struct correctness |
| `test_matrix_verification_returned_fields_complete` | Returned struct correctness |
| `test_matrix_early_payment_returned_fields_complete` | Returned struct correctness |
| `test_matrix_late_payment_returned_fields_complete` | Returned struct correctness |
| `test_matrix_active_toggle_all_types` | is_active toggle, all 5 types |
| `test_matrix_early_payment_insert_then_update` | Insert → update lifecycle |
| `test_matrix_late_payment_insert_then_update` | Insert → update lifecycle |
| `test_matrix_update_platform_preserves_others` | Isolation guarantee |
| `test_matrix_insert_early_payment_preserves_existing_types` | Isolation guarantee |
| `test_matrix_late_payment_cross_check_floor_enforced_on_insert` | Cross-fee floor rejection |
| `test_matrix_late_payment_cross_check_floor_exact_equal_accepted` | Cross-fee floor boundary |
| `test_matrix_flat_fee_all_types_accepted` | min == max for all 5 types |
| `test_matrix_intermediate_bps_accepted_for_all_types` | bps ∈ {1,500,999} × all types |

### Security Notes

- **No partial writes**: all validation runs before any storage mutation; a
  rejected call leaves the fee store unchanged.
- **Checked arithmetic**: threshold and min-fee computations use
  `saturating_mul` / `saturating_add`, preventing integer overflow panics.
- **Auth gate**: every call requires `admin.require_auth()` — an attacker who
  does not hold the admin key cannot mutate fee parameters.
- **Event auditability**: every accepted call emits `fee_structure_updated`
  (`fee_str` topic) containing the old and new BPS values for off-chain
  monitoring.

## Migration and Upgrades

The system is designed for backward compatibility, with new event structures providing more detail than legacy versions without breaking core settlement logic.
