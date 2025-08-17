# QuickLendX Protocol - Smart Contracts

[![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Soroban](https://img.shields.io/badge/Soroban-000000?style=for-the-badge&logo=stellar&logoColor=white)](https://soroban.stellar.org/)
[![License](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

A decentralized invoice financing protocol built on Stellar's Soroban platform, enabling businesses to access working capital by selling their invoices to investors.

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
- **`bid.rs`**: Bidding system and bid management
- **`payments.rs`**: Escrow creation, release, and refund
- **`verification.rs`**: KYC and business verification
- **`audit.rs`**: Audit trail and integrity validation
- **`backup.rs`**: Data backup and restoration
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
```

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

⚠️ **Important**: Mainnet deployment requires thorough testing and security audits.

1. **Security Checklist**
   - [ ] All tests passing
   - [ ] Security audit completed
   - [ ] Gas optimization verified
   - [ ] Emergency pause functionality tested

2. **Deploy to Mainnet**
```bash
stellar-cli contract deploy \
    --wasm target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm \
    --source <DEPLOYER_ACCOUNT> \
    --network mainnet
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

## 📋 Best Practices

### Security

1. **Input Validation**
   - Always validate user inputs
   - Check for overflow conditions
   - Sanitize string inputs

2. **Access Control**
   - Implement proper authorization checks
   - Use role-based access control
   - Validate caller permissions

3. **Error Handling**
   - Provide meaningful error messages
   - Don't expose sensitive information
   - Handle edge cases gracefully

### Code Quality

1. **Documentation**
   - Document all public functions
   - Include parameter descriptions
   - Provide usage examples

2. **Testing**
   - Write comprehensive unit tests
   - Test edge cases and error conditions
   - Maintain high test coverage

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