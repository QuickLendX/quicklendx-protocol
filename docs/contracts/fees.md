# Platform Fee System Documentation

## Overview

The QuickLendX platform implements a configurable fee system with treasury routing capabilities. The system applies a default 2% platform fee on invoice transactions during settlement, with fees automatically routed to a configured treasury address.

## Key Features

### 1. Configurable Platform Fee

- **Default Rate**: 2% (200 basis points)
- **Maximum Rate**: 10% (1000 basis points)
- **Admin-only Configuration**: Only platform administrators can modify fee rates
- **Real-time Updates**: Fee changes take effect immediately for new transactions

### 2. Treasury Routing

- **Automatic Routing**: Platform fees are automatically routed to the configured treasury address
- **Fallback Mechanism**: If no treasury is configured, fees are sent to the contract address
- **Secure Configuration**: Only administrators can set or update the treasury address
- **Event Tracking**: All fee routing activities are logged via blockchain events

### 3. Settlement Integration

- **Applied at Settlement**: Fees are calculated and collected during invoice settlement
- **Profit-based Calculation**: Fees are only applied to the profit portion (payment amount - investment amount)
- **Transparent Calculation**: Clear separation between investor returns and platform fees

### 4. Volume Tiers

- **Tiered Discounts**: User transaction volume determines a discount (basis points) applied to fee calculation: Standard (0), Silver (5%), Gold (10%), Platinum (15%).
- **Tier Thresholds**: Volume is accumulated via `update_user_transaction_volume`; tiers are derived from `total_volume` (e.g. Platinum at 1e12+).
- **Usage**: `calculate_total_fees` and `calculate_transaction_fees` use `get_tier_discount` for volume-based fee reduction.

### 5. Fee Bounds Validation

- **Admin-only Config**: Fee structures (base_fee_bps, min_fee, max_fee) and platform fee BPS are updated only by admin; all such functions require admin auth and validate bounds.
- **Validate Fee Parameters**: `validate_fee_parameters(base_fee_bps, min_fee, max_fee)` enforces: `base_fee_bps <= 1000` (10% max), `min_fee >= 0`, `max_fee >= min_fee`. Used before updating fee structures.
- **Zero Fee**: Supported; when fee_bps is 0 or profit is zero, platform fee is 0 and investor receives full payment. Overflow-safe math uses `saturating_mul` / `saturating_sub` in fee and revenue calculations.

## Technical Implementation

### Core Components

#### 1. Fee Configuration Structure

```rust
pub struct PlatformFeeConfig {
    pub fee_bps: u32,                          // Fee in basis points (e.g., 200 = 2%)
    pub treasury_config: Option<TreasuryConfig>, // Optional treasury configuration
    pub updated_at: u64,                        // Last update timestamp
    pub updated_by: Address,                    // Admin who made the update
}
```

#### 2. Treasury Configuration

```rust
pub struct TreasuryConfig {
    pub treasury_address: Address,  // Address to receive platform fees
    pub is_active: bool,           // Whether treasury routing is active
    pub updated_at: u64,           // Configuration timestamp
    pub updated_by: Address,       // Admin who configured it
}
```

### Key Functions

#### Administrative Functions

1. **`configure_treasury(treasury_address: Address)`**
   - Sets the treasury address for fee routing
   - Requires admin authorization
   - Emits `treasury_configured` event

2. **`update_platform_fee_bps(new_fee_bps: u32)`**
   - Updates the platform fee rate
   - Validates fee is within acceptable range (0-10%)
   - Requires admin authorization
   - Emits `platform_fee_config_updated` event

#### Query Functions

1. **`get_platform_fee_config()`**
   - Returns current platform fee configuration
   - Includes treasury settings if configured

2. **`get_treasury_address()`**
   - Returns the configured treasury address
   - Returns `None` if no treasury is configured

### Settlement Process

The fee system integrates seamlessly with the invoice settlement process:

1. **Invoice Settlement Initiated**: Business or automated process calls `settle_invoice`
2. **Fee Calculation**: System calculates platform fee based on profit (payment - investment)
3. **Fund Distribution**:
   - Investor receives: `payment_amount - platform_fee`
   - Treasury receives: `platform_fee` (if configured)
   - Contract receives: `platform_fee` (if no treasury configured)
4. **Event Emission**: `platform_fee_routed` event is emitted with routing details

### Revenue Distribution (Treasury / Developer / Platform)

- **Configuration**: `configure_revenue_distribution` (admin only) sets `treasury_share_bps`, `developer_share_bps`, `platform_share_bps` (must sum to 10_000), plus `min_distribution_amount` and `auto_distribution`.
- **Distribution**: `distribute_revenue(admin, period)` splits collected fees for the period according to the configured BPS; returns `(treasury_amount, developer_amount, platform_amount)`. Tests cover rounding and zero-fee cases (`test_fees.rs`, `test_revenue_split.rs`).

## Security Considerations

### Access Control

- **Admin-only Configuration**: All fee and treasury configuration functions require admin authorization
- **Authorization Validation**: Each administrative function validates caller permissions
- **Immutable During Settlement**: Fee rates cannot be changed mid-settlement

### Validation

- **Fee Range Validation**: Platform fees are capped at 10% maximum
- **Address Validation**: Treasury addresses are validated before configuration
- **Amount Validation**: Fee calculations include overflow protection

### Audit Trail

- **Complete Event Logging**: All fee-related activities are logged via blockchain events
- **Configuration History**: Updates include timestamps and admin addresses
- **Settlement Tracking**: Each fee routing is recorded with invoice and recipient details

## Events

The system emits the following events for transparency and monitoring:

### 1. `platform_fee_routed`

```rust
(invoice_id, recipient_address, fee_amount, timestamp)
```

Emitted when platform fees are routed during settlement.

### 2. `treasury_configured`

```rust
(treasury_address, configured_by, timestamp)
```

Emitted when treasury address is set or updated.

### 3. `platform_fee_config_updated`

```rust
(old_fee_bps, new_fee_bps, updated_by, timestamp)
```

Emitted when platform fee rate is modified.

## Usage Examples

### Initial Setup

```rust
// Initialize the fee system (admin only)
contract.initialize_fee_system(admin_address)?;

// Configure treasury address
contract.configure_treasury(treasury_address)?;
```

### Fee Management

```rust
// Update platform fee to 2.5%
contract.update_platform_fee_bps(250)?;

// Query current configuration
let config = contract.get_platform_fee_config()?;
println!("Current fee: {}%", config.fee_bps as f64 / 100.0);
```

### Settlement with Fees

```rust
// Settle invoice (fees automatically calculated and routed)
contract.settle_invoice(invoice_id, payment_amount)?;

// Check where fees were routed
if let Some(treasury) = contract.get_treasury_address() {
    println!("Fees routed to treasury: {}", treasury);
} else {
    println!("Fees routed to contract");
}
```

## Error Handling

The system includes comprehensive error handling:

- `InvalidFeeConfiguration`: Invalid fee configuration parameters
- `TreasuryNotConfigured`: Treasury-related operation when not configured
- `InvalidFeeBasisPoints`: Fee rate outside acceptable range (0-1000 bps)
- `NotAdmin`: Unauthorized access to administrative functions
- `InvalidAmount`: Invalid fee amounts or calculations

## Best Practices

### For Platform Administrators

1. **Regular Monitoring**: Monitor fee collection and routing through events
2. **Treasury Security**: Ensure treasury address is secure and properly managed
3. **Fee Optimization**: Regularly review fee rates for competitiveness
4. **Backup Configuration**: Maintain backup treasury addresses if needed

### For Integration

1. **Event Monitoring**: Subscribe to fee-related events for real-time tracking
2. **Error Handling**: Implement proper error handling for fee-related operations
3. **Testing**: Thoroughly test fee calculations in various scenarios
4. **Documentation**: Keep integration documentation updated with fee changes

## Migration and Upgrades

The fee system is designed for seamless upgrades:

- **Backward Compatibility**: New features maintain compatibility with existing functionality
- **Gradual Migration**: Treasury configuration is optional, allowing gradual adoption
- **Event Continuity**: Event schemas are versioned to maintain monitoring compatibility

## Conclusion

The QuickLendX platform fee system provides a robust, secure, and transparent mechanism for collecting platform fees while maintaining flexibility for future enhancements. The integration with treasury routing ensures efficient fee management and supports the platform's economic model.
