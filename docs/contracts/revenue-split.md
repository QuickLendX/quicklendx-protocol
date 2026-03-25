# Revenue Split Configuration

This document describes the revenue split mechanism in the QuickLendX protocol, which allows administrators to configure how platform fees are distributed among different parties.

## Overview

The revenue split system enables flexible distribution of collected platform fees between:
- **Treasury**: The protocol's operational treasury
- **Developers**: Developer funding pool for ongoing development
- **Platform**: Platform reserves for growth and maintenance

Revenue distribution is configured using **basis points (bps)**, where 10,000 bps = 100%. The sum of all shares must equal exactly 10,000 bps.

## Configuration

### `configure_revenue_distribution`

Admin-only function to set up the revenue split configuration.

```rust
pub fn configure_revenue_distribution(
    env: Env,
    admin: Address,
    treasury_address: Address,
    treasury_share_bps: u32,     // e.g., 6000 = 60%
    developer_share_bps: u32,    // e.g., 2000 = 20%
    platform_share_bps: u32,     // e.g., 2000 = 20%
    auto_distribution: bool,
    min_distribution_amount: i128,
) -> Result<(), QuickLendXError>
```

**Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `admin` | `Address` | Must match the stored admin address |
| `treasury_address` | `Address` | Address to receive treasury share |
| `treasury_share_bps` | `u32` | Treasury share in basis points |
| `developer_share_bps` | `u32` | Developer share in basis points |
| `platform_share_bps` | `u32` | Platform share in basis points |
| `auto_distribution` | `bool` | Enable automatic distribution on threshold |
| `min_distribution_amount` | `i128` | Minimum amount required for distribution |

**Validation:**
- Requires admin authorization
- `treasury_share_bps + developer_share_bps + platform_share_bps` must equal `10,000`

**Errors:**
- `NotAdmin`: Caller is not the admin
- `InvalidAmount`: Shares don't sum to 10,000 bps

### `get_revenue_split_config`

Query the current revenue split configuration.

```rust
pub fn get_revenue_split_config(env: Env) -> Result<RevenueConfig, QuickLendXError>
```

**Returns:** `RevenueConfig` struct containing all configuration parameters.

**Errors:**
- `StorageKeyNotFound`: Configuration not yet set

## Distribution

### `distribute_revenue`

Execute revenue distribution for a specific period.

```rust
pub fn distribute_revenue(
    env: Env,
    admin: Address,
    period: u64,
) -> Result<(i128, i128, i128), QuickLendXError>
```

**Parameters:**
| Parameter | Type | Description |
|-----------|------|-------------|
| `admin` | `Address` | Admin address (requires authorization) |
| `period` | `u64` | Period identifier (calculated as `timestamp / 2,592,000`) |

**Returns:** Tuple of `(treasury_amount, developer_amount, platform_amount)`

**Distribution Logic:**
1. Treasury amount = `pending * treasury_bps / 10,000`
2. Developer amount = `pending * developer_bps / 10,000`
3. Platform amount = `pending - treasury - developer` (receives any rounding remainder)

## Fee Collection

### `collect_transaction_fees`

Record collected fees for later distribution.

```rust
pub fn collect_transaction_fees(
    env: Env,
    user: Address,
    fees_by_type: Map<FeeType, i128>,
    total_amount: i128,
) -> Result<(), QuickLendXError>
```

## Data Structures

### RevenueConfig

```rust
pub struct RevenueConfig {
    pub treasury_address: Address,
    pub treasury_share_bps: u32,
    pub developer_share_bps: u32,
    pub platform_share_bps: u32,
    pub auto_distribution: bool,
    pub min_distribution_amount: i128,
}
```

### RevenueData

```rust
pub struct RevenueData {
    pub period: u64,
    pub total_collected: i128,
    pub fees_by_type: Map<FeeType, i128>,
    pub total_distributed: i128,
    pub pending_distribution: i128,
    pub transaction_count: u32,
}
```

## Example Usage

### Setting up a 60/20/20 Split

```rust
// Configure revenue split: 60% Treasury, 20% Developer, 20% Platform
client.configure_revenue_distribution(
    &admin,
    &treasury_address,
    &6000,  // 60% to treasury
    &2000,  // 20% to developers
    &2000,  // 20% to platform
    &false, // manual distribution
    &1000,  // minimum 1000 units to distribute
);
```

### Distributing Revenue

```rust
// Get current period
let current_period = env.ledger().timestamp() / 2_592_000;

// Distribute revenue and get amounts
let (treasury, developer, platform) = client.distribute_revenue(
    &admin,
    &current_period,
);
```

### Querying Configuration

```rust
// Get current configuration
let config = client.get_revenue_split_config();
println!("Treasury share: {}%", config.treasury_share_bps / 100);
```

## Security Considerations

1. **Admin-Only Configuration**: Only the verified admin can modify revenue split settings
2. **Validation**: Share percentages must sum to exactly 100% (10,000 bps)
3. **Minimum Threshold**: Prevents dust distributions that waste gas
4. **Remainder Handling**: Platform receives rounding remainder to prevent fund loss
5. **Period-Based Tracking**: Revenue is tracked per period to enable auditing

## Treasury Address Rotation

Routing fees to an incorrect address is an irreversible on-chain action. The protocol
therefore requires a **two-step confirmation** before any treasury or fee-recipient
address change takes effect.

### Flow

```
Admin                         New Treasury Address
  |                                   |
  |-- initiate_treasury_rotation -->  |  (pending rotation stored, 7-day window)
  |                                   |
  |                 <-- confirm_treasury_rotation --  (new address proves ownership)
  |                                   |
  |              (rotation committed, old address replaced)
```

If the admin changes their mind, `cancel_treasury_rotation` can be called at any time
before the new address confirms.

### `initiate_treasury_rotation`

```rust
pub fn initiate_treasury_rotation(
    env: Env,
    new_address: Address,
) -> Result<RecipientRotationRequest, QuickLendXError>
```

- Requires admin authorization.
- Rejects if a rotation is already pending (`RotationAlreadyPending`).
- Rejects if `new_address` equals the current treasury (`InvalidAddress`).
- Stores a `RecipientRotationRequest` with a 7-day (`604,800 s`) confirmation deadline.
- Emits `rot_init` event.

### `confirm_treasury_rotation`

```rust
pub fn confirm_treasury_rotation(
    env: Env,
    new_address: Address,
) -> Result<Address, QuickLendXError>
```

- Must be called **by the `new_address`** from the pending request.
- Rejects if no rotation is pending (`RotationNotFound`).
- Rejects if called by a different address (`Unauthorized`).
- Rejects after the 7-day deadline (`RotationExpired`), and clears the pending state.
- On success: writes `new_address` as the treasury, clears pending request, emits `rot_conf`.

### `cancel_treasury_rotation`

```rust
pub fn cancel_treasury_rotation(env: Env) -> Result<(), QuickLendXError>
```

- Admin-only.
- Rejects if nothing is pending (`RotationNotFound`).
- Clears pending request without modifying the current treasury. Emits `rot_canc`.

### `get_pending_treasury_rotation`

```rust
pub fn get_pending_treasury_rotation(env: Env) -> Option<RecipientRotationRequest>
```

Read-only query returning the pending rotation, if any.

### `RecipientRotationRequest` struct

```rust
pub struct RecipientRotationRequest {
    pub new_address: Address,
    pub initiated_by: Address,
    pub initiated_at: u64,
    pub confirmation_deadline: u64,
}
```

### Error codes

| Error | Code | Meaning |
|---|---|---|
| `RotationAlreadyPending` | 1853 | A rotation is already waiting for confirmation |
| `RotationNotFound` | 1854 | No pending rotation to confirm or cancel |
| `RotationExpired` | 1855 | Confirmation deadline has passed |

### Events

| Topic | Fields | Trigger |
|---|---|---|
| `rot_init` | `(new_address, initiated_by, deadline, timestamp)` | Rotation initiated |
| `rot_conf` | `(old_address, new_address, confirmed_at)` | Rotation confirmed |
| `rot_canc` | `(cancelled_address, cancelled_by, timestamp)` | Rotation cancelled |

### Security assumptions

- **Ownership proof**: The new address must sign a transaction to confirm, preventing
  misrouting to an address nobody controls.
- **Time-bounded**: Unconfirmed requests expire after 7 days, preventing stale rotations.
- **Single-pending**: Only one rotation at a time prevents confusion about the intended destination.
- **Idempotent cancel**: Cancellation is always safe; it never silently overwrites state.

## Analytics

### `get_fee_analytics`

```rust
pub fn get_fee_analytics(env: Env, period: u64) -> Result<FeeAnalytics, QuickLendXError>
```

Returns analytics including:
- `total_fees`: Total fees collected in the period
- `average_fee_rate`: Average fee per transaction
- `total_transactions`: Number of fee-generating transactions
- `fee_efficiency_score`: Distribution efficiency (0-100)

## Related Documentation

- [Fees Documentation](./fees.md)
- [Escrow Documentation](./escrow.md)
- [Security Documentation](./security.md)
