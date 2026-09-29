# Admin Dry-Run Mode for Configuration Updates

## Overview

The QuickLendX Protocol provides "dry-run" capabilities for critical administrative configuration changes. This feature allows administrators to preview the exact impact of proposed `set_protocol_config` and `set_fee_config` operations without actually committing any changes to the blockchain state. This is crucial for ensuring parameter correctness, validating expected outcomes, and mitigating risks associated with misconfigurations in a production environment.

## Key Features

-   **Read-Only Simulation**: Dry-run functions execute all validation logic and calculate the projected state changes in memory, without performing any storage writes.
-   **Admin-Gated Access**: Only authorized administrators can invoke dry-run functions, ensuring that sensitive configuration previews are restricted.
-   **Detailed Diff Output**: The functions return a `Diff` struct that clearly outlines the `current` configuration and the `proposed` configuration, enabling operators to compare and verify changes.
-   **Error Pre-check**: If proposed parameters are invalid, the dry-run will return the same `QuickLendXError` that the actual `set_` operation would, allowing for early detection of issues.

## Available Dry-Run Operations

### `preview_protocol_config`

This function simulates an update to the core protocol configuration.

```rust
pub fn preview_protocol_config(
    env: Env,
    admin: Address,
    min_bid_bps: u32,
    min_bid_amount: i128,
    bid_ttl_seconds: u64,
    max_active_bids_per_investor: u32,
) -> Result<ProtocolConfigDiff, QuickLendXError>
```

**Parameters:**

-   `env`: The Soroban environment.
-   `admin`: The address of the administrator.
-   `min_bid_bps`: The proposed minimum bid in basis points.
-   `min_bid_amount`: The proposed absolute minimum bid amount.
-   `bid_ttl_seconds`: The proposed time-to-live for bids in seconds.
-   `max_active_bids_per_investor`: The proposed maximum active bids an investor can have.

**Returns:**

-   `Ok(ProtocolConfigDiff)`: A struct containing the `current` and `proposed` `ProtocolConfig` states.
-   `Err(QuickLendXError)`: An error if any of the proposed parameters are invalid (e.g., `InvalidBidBasisPoints`, `InvalidAmount`, `InvalidBidTTL`, `InvalidLimit`).

**Example Usage:**

```rust
use soroban_sdk::{Address, Env};
use quicklendx_contracts::{QuickLendXContractClient, admin::ProtocolConfigDiff};

let env = Env::default();
env.mock_all_auths();

let contract_id = env.register_contract(None, QuickLendXContract);
let client = QuickLendXContractClient::new(&env, &contract_id);

let admin = Address::random(&env);
client.set_admin(&admin); // Assume admin is set and initialized

// Preview a change to min_bid_bps
let diff = client.preview_protocol_config(
    &admin,
    150, // new min_bid_bps
    1000,
    60 * 60 * 24 * 7, // 7 days
    10,
).unwrap();

println!("Current min_bid_bps: {}", diff.current.min_bid_bps);
println!("Proposed min_bid_bps: {}", diff.proposed.min_bid_bps);
```

### `preview_fee_config`

This function simulates an update to a specific fee structure.

```rust
pub fn preview_fee_config(
    env: Env,
    admin: Address,
    fee_type: FeeType,
    base_fee_bps: u32,
    min_fee: i128,
    max_fee: i128,
    is_active: bool,
) -> Result<FeeConfigDiff, QuickLendXError>
```

**Parameters:**

-   `env`: The Soroban environment.
-   `admin`: The address of the administrator.
-   `fee_type`: The type of fee to preview (e.g., `FeeType::Platform`).
-   `base_fee_bps`: The proposed base fee in basis points.
-   `min_fee`: The proposed minimum fee amount.
-   `max_fee`: The proposed maximum fee amount.
-   `is_active`: The proposed active status of the fee.

**Returns:**

-   `Ok(FeeConfigDiff)`: A struct containing the `current` (if exists) and `proposed` `FeeStructure` states.
-   `Err(QuickLendXError)`: An error if any of the proposed parameters are invalid (e.g., `InvalidFeeBasisPoints`, `InvalidAmount`, `InvalidFeeConfiguration`).

**Example Usage:**

```rust
use soroban_sdk::{Address, Env};
use quicklendx_contracts::{QuickLendXContractClient, admin::FeeConfigDiff, fees::FeeType};

let env = Env::default();
env.mock_all_auths();

let contract_id = env.register_contract(None, QuickLendXContract);
let client = QuickLendXContractClient::new(&env, &contract_id);

let admin = Address::random(&env);
client.set_admin(&admin); // Assume admin is set and initialized
client.initialize_fee_system(&admin); // Assume fee system is initialized

// Preview a change to the Platform fee
let diff = client.preview_fee_config(
    &admin,
    &FeeType::Platform,
    250, // new base_fee_bps (2.5%)
    100,
    1_000_000,
    true,
).unwrap();

println!("Current Platform fee BPS: {}", diff.current.unwrap().base_fee_bps);
println!("Proposed Platform fee BPS: {}", diff.proposed.base_fee_bps);
```

## Security Considerations

-   **Admin-Gated**: The dry-run functions are protected by `require_auth()` to ensure only authorized administrators can access them.
-   **Read-Only**: These functions explicitly do not modify any on-chain state, ensuring they are safe to call without risk of accidental changes.
-   **Operator UX**: By providing a clear preview, operators can verify complex configuration changes in a simulated environment before executing them on mainnet, significantly reducing the risk of human error and costly mistakes. This is particularly important for parameters like fee rates and bid limits that directly impact the protocol's economics and user experience.

## Testing Strategy

The dry-run functions are thoroughly tested to ensure:

-   The output `Diff` accurately reflects the `current` and `proposed` states.
-   All underlying validation logic is executed, and appropriate errors are returned for invalid inputs.
-   No storage mutations occur during a dry-run.
-   Access control (`admin.require_auth()`) is correctly enforced.
-   Edge cases, such as non-existent fee structures or boundary values for parameters, are handled gracefully.
