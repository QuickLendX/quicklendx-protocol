# QuickLendX Protocol - Smart Contracts

[![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Soroban](https://img.shields.io/badge/Soroban-000000?style=for-the-badge&logo=stellar&logoColor=white)](https://soroban.stellar.org/)
[![License](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

**Production-ready** smart contracts for QuickLendX, a decentralized invoice financing protocol built on Stellar's Soroban platform. These contracts enable businesses to access working capital by selling their invoices to investors through a secure, transparent, and efficient blockchain-based marketplace.

> **Note**: This is the smart contracts repository. For the full project documentation, see the [main README](../README.md).

## 📚 Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [API Documentation](#api-documentation)
- [Code Examples](#code-examples)
- [Deployment Guide](#deployment-guide)
- [Troubleshooting](#troubleshooting)
- [Best Practices](#best-practices)
- [Contributing](#contributing)

## 🚀 Overview

QuickLendX is a comprehensive DeFi protocol that facilitates invoice financing through smart contracts. The protocol enables:

- **Invoice Management**: Upload, verify, and manage business invoices
- **Bidding System**: Investors can place bids on invoices with competitive rates
- **Escrow Management**: Secure fund handling through smart contract escrows
- **KYC/Verification**: Business verification and compliance features
- **Audit Trail**: Complete transaction history and audit capabilities
- **Backup & Recovery**: Data backup and restoration functionality

### Key Features

- ✅ **Multi-currency Support**: Handle invoices in various currencies
- ✅ **Rating System**: Community-driven invoice quality assessment
- ✅ **Category Management**: Organized invoice categorization
- ✅ **Tag System**: Flexible invoice tagging for better organization
- ✅ **Real-time Settlement**: Automated payment processing
- ✅ **Comprehensive Auditing**: Full audit trail and integrity validation

## 🏗️ Architecture

```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Frontend      │    │   Soroban       │    │   Stellar       │
│   (Next.js)     │◄──►│   Smart         │◄──►│   Network       │
│                 │    │   Contracts     │    │                 │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                              │
                    ┌─────────────────┐
                    │   Core Modules  │
                    │                 │
                    │ • Invoice       │
                    │ • Bid           │
                    │ • Payment       │
                    │ • Verification  │
                    │ • Audit         │
                    │ • Backup        │
                    └─────────────────┘
```

### Core Modules

- **`invoice.rs`**: Invoice creation, management, and lifecycle
- **`bid.rs`**: Bidding system and bid management with ranking algorithms
- **`payments.rs`**: Escrow creation, release, and refund
- **`verification.rs`**: KYC, business and investor verification with risk assessment
- **`audit.rs`**: Audit trail and integrity validation
- **`backup.rs`**: Data backup and restoration
- **`analytics.rs`**: Platform metrics, reporting, and business intelligence
- **`fees.rs`**: Fee management and revenue distribution
- **`settlement.rs`**: Invoice settlement and payment processing
- **`investment.rs`**: Investment tracking and insurance
- **`notifications.rs`**: Notification system for all parties
- **`events.rs`**: Event emission and handling
- **`errors.rs`**: Error definitions and handling

## ⚡ Quick Start

### Prerequisites

- **Rust** (1.70+): [Install via rustup](https://rustup.rs/)
- **Stellar CLI** (23.0.0+): [Installation Guide](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup)
- **Git**: [Download](https://git-scm.com/)

### Installation

```bash
# Clone the repository
git clone https://github.com/your-org/quicklendx-protocol.git
cd quicklendx-protocol/quicklendx-contracts

# Build the contracts
cargo build

# Run tests
cargo test

# Run with logs for debugging
cargo test --profile release-with-logs
```

### Basic Usage Example

```rust
use soroban_sdk::{Address, String, Vec, vec};

// Initialize contract
let contract = QuickLendXContract::new();

// Create an invoice
let invoice_id = contract.store_invoice(
    &env,
    business_address,
    10000, // $100.00 in cents
    usdc_token_address,
    due_date_timestamp,
    String::from_str(&env, "Web development services"),
    InvoiceCategory::Services,
    vec![&env, String::from_str(&env, "tech"), String::from_str(&env, "development")]
)?;

// Place a bid
let bid_id = contract.place_bid(
    &env,
    investor_address,
    invoice_id,
    9500, // $95.00 bid
    10500 // $105.00 expected return
)?;
```

## 📖 API Documentation

### Core Functions

#### Invoice Management

##### `store_invoice`

Creates and stores a new invoice in the contract.

```rust
pub fn store_invoice(
    env: Env,
    business: Address,
    amount: i128,
    currency: Address,
    due_date: u64,
    description: String,
    category: InvoiceCategory,
    tags: Vec<String>,
) -> Result<BytesN<32>, QuickLendXError>
```

**Parameters:**

- `business`: Address of the business creating the invoice
- `amount`: Invoice amount in smallest currency unit (e.g., cents)
- `currency`: Token address for the invoice currency
- `due_date`: Unix timestamp for invoice due date
- `description`: Human-readable invoice description
- `category`: Invoice category (Services, Goods, etc.)
- `tags`: Array of tags for categorization

**Returns:** Invoice ID (32-byte hash)

**Example:**

```rust
let invoice_id = contract.store_invoice(
    &env,
    business_addr,
    10000, // $100.00
    usdc_addr,
    1735689600, // Jan 1, 2025
    String::from_str(&env, "Consulting services"),
    InvoiceCategory::Services,
    vec![&env, String::from_str(&env, "consulting")]
)?;
```

##### `get_invoice`

Retrieves invoice details by ID.

```rust
pub fn get_invoice(env: Env, invoice_id: BytesN<32>) -> Result<Invoice, QuickLendXError>
```

##### `update_invoice_status`

Updates the status of an invoice.

```rust
pub fn update_invoice_status(
    env: Env,
    invoice_id: BytesN<32>,
    new_status: InvoiceStatus,
) -> Result<(), QuickLendXError>
```

#### Bidding System

##### `place_bid`

Places a bid on an available invoice.

```rust
pub fn place_bid(
    env: Env,
    investor: Address,
    invoice_id: BytesN<32>,
    bid_amount: i128,
    expected_return: i128,
) -> Result<BytesN<32>, QuickLendXError>
```

**Parameters:**

- `investor`: Address of the investor placing the bid
- `invoice_id`: ID of the invoice to bid on
- `bid_amount`: Amount the investor is willing to pay
- `expected_return`: Expected return amount

**Returns:** Bid ID

##### `accept_bid`

Accepts a bid on an invoice, creating an escrow.

```rust
pub fn accept_bid(
    env: Env,
    invoice_id: BytesN<32>,
    bid_id: BytesN<32>,
) -> Result<(), QuickLendXError>
```

#### Payment & Escrow

##### `release_escrow_funds`

Releases escrow funds to the investor upon invoice verification.

```rust
pub fn release_escrow_funds(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError>
```

##### `refund_escrow_funds`

Refunds escrow funds to the investor if conditions aren't met.

```rust
pub fn refund_escrow_funds(env: Env, invoice_id: BytesN<32>) -> Result<(), QuickLendXError>
```

#### Verification & KYC

##### `submit_kyc_application`

Submits KYC application for business verification.

```rust
pub fn submit_kyc_application(
    env: Env,
    business: Address,
    kyc_data: String,
) -> Result<(), QuickLendXError>
```

##### `verify_business`

Verifies a business (admin only).

```rust
pub fn verify_business(
    env: Env,
    admin: Address,
    business: Address,
) -> Result<(), QuickLendXError>
```

#### Query Functions (Paginated)

##### `get_business_invoices_paged`

Retrieves invoices for a business with pagination and optional status filtering.

```rust
pub fn get_business_invoices_paged(
    env: Env,
    business: Address,
    status_filter: Option<InvoiceStatus>,
    offset: u32,
    limit: u32,
) -> Vec<BytesN<32>>
```

**Parameters:**

- `business`: Address of the business
- `status_filter`: Optional status filter (None returns all statuses)
- `offset`: Starting index for pagination (0-based)
- `limit`: Maximum number of results to return

**Returns:** Vector of invoice IDs

**Example:**

```rust
// Get first 10 verified invoices for a business
let invoices = contract.get_business_invoices_paged(
    &env,
    business_addr,
    Some(InvoiceStatus::Verified),
    0,  // offset
    10  // limit
);

// Get next 10 invoices
let more_invoices = contract.get_business_invoices_paged(
    &env,
    business_addr,
    Some(InvoiceStatus::Verified),
    10, // offset
    10  // limit
);
```

##### `get_investor_investments_paged`

Retrieves investments for an investor with pagination and optional status filtering.

```rust
pub fn get_investor_investments_paged(
    env: Env,
    investor: Address,
    status_filter: Option<InvestmentStatus>,
    offset: u32,
    limit: u32,
) -> Vec<BytesN<32>>
```

**Parameters:**

- `investor`: Address of the investor
- `status_filter`: Optional investment status filter
- `offset`: Starting index for pagination
- `limit`: Maximum number of results to return

**Returns:** Vector of investment IDs

**Example:**

```rust
// Get all active investments for an investor
let active_investments = contract.get_investor_investments_paged(
    &env,
    investor_addr,
    Some(InvestmentStatus::Active),
    0,
    50
);
```

##### `get_available_invoices_paged`

Retrieves available (verified) invoices with pagination and optional filters.

```rust
pub fn get_available_invoices_paged(
    env: Env,
    min_amount: Option<i128>,
    max_amount: Option<i128>,
    category_filter: Option<InvoiceCategory>,
    offset: u32,
    limit: u32,
) -> Vec<BytesN<32>>
```

**Parameters:**

- `min_amount`: Optional minimum invoice amount filter
- `max_amount`: Optional maximum invoice amount filter
- `category_filter`: Optional category filter
- `offset`: Starting index for pagination
- `limit`: Maximum number of results to return

**Returns:** Vector of invoice IDs

**Example:**

```rust
// Get service invoices between $100 and $1000
let invoices = contract.get_available_invoices_paged(
    &env,
    Some(10000),  // $100.00 minimum
    Some(100000), // $1000.00 maximum
    Some(InvoiceCategory::Services),
    0,
    20
);
```

##### `get_bid_history_paged`

Retrieves bid history for an invoice with pagination and optional status filtering.

```rust
pub fn get_bid_history_paged(
    env: Env,
    invoice_id: BytesN<32>,
    status_filter: Option<BidStatus>,
    offset: u32,
    limit: u32,
) -> Vec<Bid>
```

**Parameters:**

- `invoice_id`: ID of the invoice
- `status_filter`: Optional bid status filter
- `offset`: Starting index for pagination
- `limit`: Maximum number of results to return

**Returns:** Vector of Bid objects

**Example:**

```rust
// Get all accepted bids for an invoice
let accepted_bids = contract.get_bid_history_paged(
    &env,
    invoice_id,
    Some(BidStatus::Accepted),
    0,
    10
);
```

##### `get_investor_bids_paged`

Retrieves bid history for an investor with pagination and optional status filtering.

```rust
pub fn get_investor_bids_paged(
    env: Env,
    investor: Address,
    status_filter: Option<BidStatus>,
    offset: u32,
    limit: u32,
) -> Vec<Bid>
```

**Parameters:**

- `investor`: Address of the investor
- `status_filter`: Optional bid status filter
- `offset`: Starting index for pagination
- `limit`: Maximum number of results to return

**Returns:** Vector of Bid objects

**Example:**

```rust
// Get investor's placed bids
let placed_bids = contract.get_investor_bids_paged(
    &env,
    investor_addr,
    Some(BidStatus::Placed),
    0,
    25
);
```

**Pagination Notes:**

- All paginated functions use overflow-safe arithmetic (`saturating_add`, `min`)
- Offset beyond data length returns empty results (no error)
- Limit of 0 returns empty results
- Filters are applied before pagination for accurate results

#### Audit & Backup

##### `get_audit_trail`

Retrieves audit trail for an invoice.

```rust
pub fn get_invoice_audit_trail(env: Env, invoice_id: BytesN<32>) -> Vec<BytesN<32>>
```

##### `create_backup`

Creates a backup of contract data.

```rust
pub fn create_backup(env: Env, description: String) -> Result<BytesN<32>, QuickLendXError>
```

### Data Structures

#### Invoice

```rust
pub struct Invoice {
    pub id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub due_date: u64,
    pub description: String,
    pub category: InvoiceCategory,
    pub tags: Vec<String>,
    pub status: InvoiceStatus,
    pub created_at: u64,
    pub updated_at: u64,
}
```

#### Bid

```rust
pub struct Bid {
    pub id: BytesN<32>,
    pub investor: Address,
    pub invoice_id: BytesN<32>,
    pub bid_amount: i128,
    pub expected_return: i128,
    pub status: BidStatus,
    pub created_at: u64,
}
```

## 💻 Code Examples

### Complete Invoice Lifecycle

```rust
use soroban_sdk::{Address, String, Vec, vec, BytesN};

// 1. Business submits KYC
contract.submit_kyc_application(
    &env,
    business_addr,
    String::from_str(&env, "{\"name\":\"Acme Corp\",\"tax_id\":\"123456789\"}")
)?;

// 2. Admin verifies business
contract.verify_business(&env, admin_addr, business_addr)?;

// 3. Business creates invoice
let invoice_id = contract.store_invoice(
    &env,
    business_addr,
    50000, // $500.00
    usdc_addr,
    1735689600,
    String::from_str(&env, "Software development services"),
    InvoiceCategory::Services,
    vec![&env, String::from_str(&env, "software"), String::from_str(&env, "development")]
)?;

// 4. Investor places bid
let bid_id = contract.place_bid(
    &env,
    investor_addr,
    invoice_id,
    48000, // $480.00 bid
    52000  // $520.00 expected return
)?;

// 5. Business accepts bid
contract.accept_bid(&env, invoice_id, bid_id)?;

// 6. Invoice gets verified
contract.verify_invoice(&env, invoice_id)?;

// 7. Release escrow to investor
contract.release_escrow_funds(&env, invoice_id)?;
```

### Query Examples

```rust
// Get all invoices for a business
let business_invoices = contract.get_business_invoices(&env, business_addr);

// Get invoices by status
let pending_invoices = contract.get_invoices_by_status(&env, InvoiceStatus::Pending);

// Get invoices with rating above threshold
let high_rated_invoices = contract.get_invoices_with_rating_above(&env, 4);

// Get audit trail
let audit_trail = contract.get_invoice_audit_trail(&env, invoice_id);

// Query audit logs
let filter = AuditQueryFilter {
    operation: Some(AuditOperation::InvoiceCreated),
    actor: Some(business_addr),
    start_time: Some(1640995200), // Jan 1, 2022
    end_time: Some(1672531200),   // Jan 1, 2023
};
let audit_logs = contract.query_audit_logs(&env, filter, 100);

// Paginated queries for large datasets
// Get first page of business invoices (10 per page)
let page1 = contract.get_business_invoices_paged(
    &env,
    business_addr,
    None, // all statuses
    0,    // offset
    10    // limit
);

// Get second page
let page2 = contract.get_business_invoices_paged(
    &env,
    business_addr,
    None,
    10,   // offset
    10    // limit
);

// Get available invoices with filters
let filtered_invoices = contract.get_available_invoices_paged(
    &env,
    Some(5000),   // min $50.00
    Some(50000),  // max $500.00
    Some(InvoiceCategory::Services),
    0,
    20
);

// Get investor's active investments
let active_investments = contract.get_investor_investments_paged(
    &env,
    investor_addr,
    Some(InvestmentStatus::Active),
    0,
    50
);

// Get bid history for an invoice
let bids = contract.get_bid_history_paged(
    &env,
    invoice_id,
    Some(BidStatus::Placed),
    0,
    25
);
```

Query limit safety:

- Public query endpoints with pagination/limits enforce `MAX_QUERY_LIMIT = 100`.
- Effective limit is always `min(limit, 100)` for `query_audit_logs`, `query_analytics_data`,
  `get_business_invoices_paged`, `get_investor_investments_paged`,
  `get_available_invoices_paged`, `get_bid_history_paged`, and `get_investor_bids_paged`.

### Error Handling

```rust
use crate::errors::QuickLendXError;

match contract.store_invoice(&env, business, amount, currency, due_date, description, category, tags) {
    Ok(invoice_id) => {
        println!("Invoice created successfully: {:?}", invoice_id);
    }
    Err(QuickLendXError::InvalidAmount) => {
        println!("Error: Invalid invoice amount");
    }
    Err(QuickLendXError::InvoiceDueDateInvalid) => {
        println!("Error: Due date must be in the future");
    }
    Err(QuickLendXError::InvalidDescription) => {
        println!("Error: Description cannot be empty");
    }
    Err(e) => {
        println!("Unexpected error: {:?}", e);
    }
}
```

## 🚀 Deployment Guide

### Local Development

1. **Set up Soroban Local Network**

```bash
# Start local network
stellar-cli network start

# Create test accounts
stellar-cli account create --name business
stellar-cli account create --name investor
stellar-cli account create --name admin
```

2. **Deploy Contract**

```bash
# Build contract
cargo build --target wasm32-unknown-unknown --release

# Deploy to local network
stellar-cli contract deploy \
    --wasm target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm \
    --source admin
```

3. **Initialize Contract**

```bash
# Set admin
stellar-cli contract invoke \
    --id <CONTRACT_ID> \
    --source admin \
    -- set_admin \
    --admin <ADMIN_ADDRESS>
```

### Testnet Deployment

1. **Configure Testnet**

```bash
# Set testnet configuration
stellar-cli network testnet

# Fund test accounts
stellar-cli account fund --source <YOUR_ACCOUNT>
```

2. **Deploy to Testnet**

```bash
# Deploy contract
stellar-cli contract deploy \
    --wasm target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm \
    --source <YOUR_ACCOUNT> \
    --network testnet
```

### Mainnet Deployment

⚠️ **CRITICAL**: Mainnet deployment requires thorough testing, security audits, and proper configuration.

#### Pre-Deployment Checklist

- [ ] All unit tests passing (`cargo test`)
- [ ] Integration tests completed
- [ ] Security audit completed by third-party auditors
- [ ] Gas optimization verified
- [ ] Contract size within limits
- [ ] Admin keys secured and backed up
- [ ] Emergency procedures documented
- [ ] Monitoring and alerting configured
- [ ] Documentation updated
- [ ] Team trained on contract operations

#### Contract size budget

The release build is tuned for minimal WASM size (`opt-level = "z"`, LTO, strip,
`codegen-units = 1`). The contract **must** stay within the size budget to be
accepted by the Stellar network.

##### Three-tier size classification

| Tier        | Range                            | Behaviour                                                                   |
| ----------- | -------------------------------- | --------------------------------------------------------------------------- |
| **OK**      | 0 – 235 929 B (≤ 90 % of budget) | Green – no action needed.                                                   |
| **Warning** | 235 930 – 262 144 B (90 – 100 %) | Yellow – plan a reduction effort; CI prints a diagnostic but does not fail. |
| **Over**    | > 262 144 B (> 256 KiB)          | Red – CI **fails**; artifact is rejected by the Stellar network.            |

##### Regression detection

In addition to the hard budget, CI compares the current build against a recorded
**baseline** (last known good size). If the artifact grows by more than **5 %**
relative to the baseline, CI fails even if the hard budget is not yet broken.
This catches incremental drift before it becomes a problem.

| Constant            | Value                     | Location                                                                                           |
| ------------------- | ------------------------- | -------------------------------------------------------------------------------------------------- |
| Hard budget (opt.)  | 262 144 B (256 KiB)       | `tests/wasm_build_size_budget.rs`, `scripts/check-wasm-size.sh`, `scripts/wasm-size-baseline.toml` |
| Raw fallback budget | 358 400 B (350 KiB)       | `tests/wasm_build_size_budget.rs` – used when `wasm-opt` absent (e.g. Windows local builds)        |
| Warning zone        | 235 929 B (90 %)          | `tests/wasm_build_size_budget.rs`                                                                  |
| Regression baseline | 240 000 B (last recorded) | `tests/wasm_build_size_budget.rs`, `scripts/wasm-size-baseline.toml`                               |
| Regression margin   | 5 %                       | `tests/wasm_build_size_budget.rs`, `scripts/check-wasm-size.sh`                                    |

##### Update procedure after a legitimate size increase

When adding features that intentionally grow the binary, update all three
locations in the **same PR** that introduces the growth:

```bash
# 1. Build and note the optimised size
cd quicklendx-contracts
./scripts/check-wasm-size.sh   # prints "WASM size: <N> bytes"

# 2. Update the baseline (replace 217668 with the new value)
#    a. tests/wasm_build_size_budget.rs  → WASM_SIZE_BASELINE_BYTES
#    b. scripts/check-wasm-size.sh       → BASELINE_BYTES
#    c. scripts/wasm-size-baseline.toml  → [baseline].bytes and recorded
```

##### Local enforcement (recommended before every push)

```bash
# Shell script – also used by CI
./scripts/check-wasm-size.sh

# Rust integration tests – also run in CI
cargo test --test wasm_build_size_budget
```

Both enforce all three tiers. The shell script uses `stellar contract build`
when the Stellar CLI is available, otherwise falls back to
`cargo build --target wasm32-unknown-unknown --release`. Either path runs
`wasm-opt -Oz` if the `binaryen` package is installed
(`brew install binaryen` / `apt install binaryen`).

Test-only code is excluded from the release artifact via `#[cfg(test)]`; the
regression tests verify this by building without the `testutils` feature.

##### Size reduction tips

- Use `stellar contract build` (produces `wasm32v1-none` binaries, typically 5–15 % smaller than `wasm32-unknown-unknown`).
- Install `wasm-opt` (`brew install binaryen`) and run the script — it applies `-Oz` post-compilation.
- Keep test helpers behind `#[cfg(test)]`; avoid `#[cfg(not(test))]` guards on large data.
- Audit `soroban-sdk` feature flags: enabling only `alloc` (not `testutils`) keeps the footprint minimal in release.
- Prefer `#[contracttype]` enums over large `String`/`Bytes` payloads for on-chain data.

#### Production Deployment Steps

1. **Final Build**

```bash
# Optimized production build (uses stellar contract build → wasm32v1-none)
stellar contract build

# Verify contract size (must be under budget)
ls -lh target/wasm32v1-none/release/quicklendx_contracts.wasm
```

2. **Deploy to Mainnet**

```bash
stellar-cli contract deploy \
    --wasm target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm \
    --source <DEPLOYER_ACCOUNT> \
    --network mainnet
```

3. **Initialize Contract**

```bash
# Set admin (CRITICAL - do this immediately)
stellar-cli contract invoke \
    --id <CONTRACT_ID> \
    --source <ADMIN_ACCOUNT> \
    --network mainnet \
    -- set_admin \
    --admin <ADMIN_ADDRESS>

# Initialize fee system
stellar-cli contract invoke \
    --id <CONTRACT_ID> \
    --source <ADMIN_ACCOUNT> \
    --network mainnet \
    -- initialize_fee_system \
    --admin <ADMIN_ADDRESS>
```

4. **Verify Deployment**

```bash
# Verify admin is set
stellar-cli contract invoke \
    --id <CONTRACT_ID> \
    --network mainnet \
    -- get_admin

# Check contract version/status
stellar-cli contract invoke \
    --id <CONTRACT_ID> \
    --network mainnet \
    -- get_total_invoice_count
```

### Environment Configuration

Create a `.env` file for your deployment:

```bash
# Network Configuration
NETWORK=testnet
CONTRACT_ID=your_contract_id_here

# Account Configuration
ADMIN_ADDRESS=your_admin_address
BUSINESS_ADDRESS=your_business_address
INVESTOR_ADDRESS=your_investor_address

# Token Addresses
USDC_TOKEN_ADDRESS=your_usdc_token_address
```

## 🔧 Troubleshooting

### Common Issues

#### Build Errors

**Error**: `error: linking with `cc` failed`

```bash
# Solution: Install build tools
sudo apt-get install build-essential  # Ubuntu/Debian
xcode-select --install                 # macOS
```

**Error**: `error: could not find `soroban-sdk``

```bash
# Solution: Update dependencies
cargo update
cargo clean
cargo build
```

#### Runtime Errors

**Error**: `QuickLendXError::InvalidAmount`

- **Cause**: Invoice amount is zero or negative
- **Solution**: Ensure amount > 0

**Error**: `QuickLendXError::InvoiceDueDateInvalid`

- **Cause**: Due date is in the past
- **Solution**: Use future timestamp

**Error**: `QuickLendXError::Unauthorized`

- **Cause**: Caller doesn't have required permissions
- **Solution**: Check caller address and permissions

#### Network Issues

**Error**: `Failed to connect to network`

```bash
# Check network status
stellar-cli network status

# Restart local network
stellar-cli network stop
stellar-cli network start
```

### Debug Mode

Enable debug logging:

```bash
# Build with debug assertions
cargo build --profile release-with-logs

# Run tests with verbose output
RUST_LOG=debug cargo test -- --nocapture
```

### Performance Optimization

1. **Gas Optimization**
   - Use efficient data structures
   - Minimize storage operations
   - Batch operations when possible

2. **Memory Management**
   - Avoid unnecessary allocations
   - Use references where possible
   - Clean up temporary data

## 🔒 Production Security

### Security Best Practices

1. **Input Validation**
   - Always validate user inputs before processing
   - Check for overflow/underflow conditions
   - Sanitize string inputs and enforce length limits
   - Validate addresses and amounts

2. **Access Control**
   - Implement proper authorization checks on all sensitive functions
   - Use role-based access control (admin, business, investor)
   - Validate caller permissions using `require_auth()`
   - Never trust external inputs

3. **Error Handling**
   - Provide meaningful error messages for debugging
   - Don't expose sensitive information in errors
   - Handle edge cases gracefully
   - Use custom error types for better error handling

4. **Audit & Monitoring**
   - Emit events for all critical operations
   - Maintain comprehensive audit trails
   - Monitor contract state changes
   - Set up alerts for suspicious activities

### Production Checklist

- ✅ All functions have proper access control
- ✅ Input validation on all user-facing functions
- ✅ Overflow/underflow protection
- ✅ Reentrancy protection (where applicable)
- ✅ Event emission for all state changes
- ✅ Comprehensive error handling
- ✅ Gas optimization verified
- ✅ Security audit completed

### Code Quality

1. **Documentation**
   - Document all public functions
   - Include parameter descriptions
   - Provide usage examples

2. **Testing**
   - Write comprehensive unit tests
   - Test edge cases and error conditions
   - **Minimum 95% test coverage** (enforced in CI when tests are enabled; run `cargo llvm-cov --lib` to check locally)

3. **Code Organization**
   - Separate concerns into modules
   - Use consistent naming conventions
   - Keep functions focused and small

### Gas Optimization

1. **Storage**
   - Minimize storage operations
   - Use efficient data structures
   - Batch storage updates

2. **Computation**
   - Avoid expensive operations in loops
   - Use efficient algorithms
   - Cache frequently accessed data

### Testing Strategy

1. **Unit Tests**
   - Test individual functions
   - Mock dependencies
   - Test error conditions

2. **Integration Tests**
   - Test complete workflows
   - Test module interactions
   - Test real-world scenarios

3. **Property Tests**
   - Test invariants
   - Test edge cases
   - Test performance characteristics

4. **Fuzz Tests** 🔬
   - Property-based testing for critical paths
   - Tests invoice creation, bid placement, and settlement
   - Validates input ranges, boundary conditions, and arithmetic safety
   - See [FUZZ_TESTING.md](FUZZ_TESTING.md) for details
   - Run with: `cargo test fuzz_`
   - Extended testing: `PROPTEST_CASES=1000 cargo test fuzz_`

### Security Testing

The protocol includes comprehensive fuzz testing for critical operations:

- **Invoice Creation**: Tests valid ranges of amount, due_date, description length
- **Bid Placement**: Tests bid_amount and expected_return validation
- **Settlement**: Tests payment_amount handling and state transitions
- **Arithmetic Safety**: Tests for overflow/underflow in calculations

See [SECURITY_ANALYSIS.md](SECURITY_ANALYSIS.md) for detailed security analysis.

## 🧪 Test Harness Authorization Guide

### Overview

Soroban's `require_auth()` enforces that the address passed to a contract function
has cryptographically signed the transaction. In the test environment this is
simulated with `env.mock_all_auths()`, which tells the host to accept every
authorization check without a real signature. **This does not weaken production
security** — `mock_all_auths()` is only available under the `testutils` feature
flag and has no effect in deployed WASM.

### Why tests were disabled

Several audit-trail and dispute-resolution tests were commented out with
`// TODO: Fix authorization issues in test environment`. The root causes were:

1. **`upload_invoice`** calls `business.require_auth()` — tests that called it
   without `mock_all_auths()` active would panic.
2. **`create_dispute`** calls `creator.require_auth()` — same issue.
3. **`set_admin`** / `verify_business` / `verify_investor` all require auth —
   calling them before `mock_all_auths()` was set up caused failures.
4. Some tests called `mock_all_auths()` mid-test (after the first contract call),
   which was too late.

### Fix applied (PR #816)

Every previously-disabled test now follows this pattern:

```rust
#[test]
fn test_example() {
    let env = Env::default();
    // Place mock_all_auths() FIRST, before any contract call.
    // This satisfies every require_auth() check for the duration of the test
    // without changing any production authorization logic.
    env.mock_all_auths();

    let contract_id = env.register(QuickLendXContract, ());
    let client = QuickLendXContractClient::new(&env, &contract_id);

    // Set up admin and KYC-verify actors before calling guarded entry points.
    let admin = Address::generate(&env);
    client.set_admin(&admin);

    let business = Address::generate(&env);
    client.submit_kyc_application(&business, &String::from_str(&env, "KYC data"));
    client.verify_business(&admin, &business);

    // Now call the auth-gated entry point normally.
    let invoice_id = client.upload_invoice(&business, /* ... */);
    // ...
}
```

### Key rules for writing auth-aware tests

| Rule | Reason |
|------|--------|
| Call `env.mock_all_auths()` **before** `env.register(...)` or any client call | The mock must be active before the first host call |
| Always call `set_admin` before any admin-only operation | `require_admin` checks the stored admin address, not just auth |
| KYC-verify businesses and investors before `upload_invoice` / `place_bid` | Production code enforces KYC independently of auth |
| For tests that exercise investor bid flows, register a real token and mint/approve | `place_bid` / `accept_bid` perform token transfers |
| `mock_all_auths()` does **not** bypass business-logic checks | Role checks (`NotAdmin`, `BusinessNotVerified`, `DisputeNotAuthorized`) still fire |

### Existing test helpers

The following public helpers in `src/test.rs` encapsulate the correct setup
pattern and should be reused in new tests:

```rust
// Sets up env + contract + admin (calls mock_all_auths internally)
pub fn setup_env() -> (Env, QuickLendXContractClient<'static>, Address, Address);

// KYC-verifies a new business address
pub fn setup_verified_business(env, client, admin) -> Address;

// KYC-verifies a new investor address with a given limit
pub fn setup_verified_investor(env, client, limit) -> Address;

// Registers a Stellar asset contract, mints tokens, and approves the contract
pub fn setup_token(env, business, investor, contract_id) -> Address;

// Creates a fully funded invoice (business + investor + token + bid + accept)
pub fn create_funded_invoice(env, client, admin)
    -> (BytesN<32>, Address, Address, Address, Address);
```

### Security invariants preserved

- `require_auth()` calls in production code are **unchanged**.
- `mock_all_auths()` is **only** compiled under `#[cfg(test)]` via the
  `soroban-sdk` `testutils` feature — it cannot appear in deployed WASM.
- Role checks (`AdminStorage::require_admin`, KYC guards, ownership checks) are
  **not** bypassed by `mock_all_auths()` and continue to be exercised by the
  tests.
- Tests that verify *rejection* of unauthorized actors (e.g.
  `test_unauthorized_dispute_creation`) still use `try_create_dispute` and
  assert `is_err()` — the business-logic authorization error is returned
  regardless of `mock_all_auths()`.



## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

### Development Workflow

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests
5. Update documentation
6. Submit a pull request

### Code Review Process

1. Automated checks must pass
2. Code review by maintainers
3. Security review for critical changes
4. Documentation updates required

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 🆘 Support

- **Documentation**: [docs.quicklendx.com](https://docs.quicklendx.com)
- **Discord**: [Join our community](https://discord.gg/quicklendx)
- **GitHub Issues**: [Report bugs](https://github.com/your-org/quicklendx-protocol/issues)
- **Email**: support@quicklendx.com

## 🔗 Links

- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Documentation](https://soroban.stellar.org/)
- [Rust Documentation](https://doc.rust-lang.org/)
- [QuickLendX Website](https://quicklendx.com)

---

**Built with ❤️ on Stellar's Soroban platform**
