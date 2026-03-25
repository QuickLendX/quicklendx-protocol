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

## Migration and Upgrades

The system is designed for backward compatibility, with new event structures providing more detail than legacy versions without breaking core settlement logic.
