# QuickLendX Protocol - Smart Contracts

[![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Soroban](https://img.shields.io/badge/Soroban-000000?style=for-the-badge&logo=stellar&logoColor=white)](https://soroban.stellar.org/)
![License](https://img.shields.io/badge/License-MIT-green.svg)

**Production-ready** smart contracts for QuickLendX, a decentralized invoice financing protocol built on Stellar's Soroban platform. These contracts enable businesses to access working capital by selling their invoices to investors through a secure, transparent, and efficient blockchain-based marketplace.

> **Note**: This is the smart contracts repository. For the full project documentation, see the [main README](../README.md).

## 📚 Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [API Reference](../docs/contracts/api-reference.md)
- [Deployment](#deployment)
- [WASM Size Budget](../docs/contracts/wasm-size.md)
- [Troubleshooting](#troubleshooting)
- [Security](#security)
- [Documentation](#documentation)
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
- **`regulatory.rs`**: Compliance seam — no-op `require_regulatory_ok` gate called on state-changing entry points (`store_invoice`, `place_bid`), reserved for future jurisdiction-specific checks.
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

## API Reference

The full entrypoint inventory, function details, data structures, and usage examples are in the [Contracts API Reference](../docs/contracts/api-reference.md).

## Deployment

Build and validate the contracts from `quicklendx-contracts/` with `cargo build` and `cargo test`. For an optimized production artifact, use `stellar contract build` and follow the current Stellar CLI deployment workflow.

Before deployment, verify the contract configuration and admin setup using the [admin operations guide](../docs/contracts/admin.md). The [WASM size budget](../docs/contracts/wasm-size.md) documents size thresholds and CI enforcement.

## Troubleshooting

- Build failures: install the Rust and Stellar CLI toolchains listed under [Quick Start](#quick-start), then run `cargo build`.
- Contract errors: consult the [error reference](../docs/contracts/errors.md) and [diagnostics guide](docs/diagnostics.md).
- For more detail, run `cargo test -- --nocapture` and review the failing test output.

## Security

Review the [contract security guide](../docs/contracts/security.md), [access-control reference](../docs/contracts/access-control.md), and repository [security analysis](SECURITY_ANALYSIS.md) before changing privileged or value-moving flows.

Tests for auth-gated calls should enable `env.mock_all_auths()` before the first contract interaction. This test-only helper does not bypass business-logic checks or change production authorization.

## 📚 Documentation

Additional documentation is available in the `docs/` directory:

- **[Regulatory Compliance Hook](../docs/contracts/regulatory.md)**: Reserved compliance seam — how `require_regulatory_ok` is wired into invoice and bid flows.
- **[Decimal Handling](docs/decimal-handling.md)**: How the contract handles different token decimal places (USDC, DAI, XLM, etc.) and how integrators should convert amounts
- **[Protocol Limits](PROTOCOL_LIMITS_README.md)**: Protocol-wide limits and configuration
- **[Admin Operations](docs/admin-dry-run.md)**: Admin function dry-run previews
- **[Bid Lifecycle](docs/bid-lifecycle.md)**: Complete bid state machine and transitions
- **[Bid Ranking](../docs/BID_RANKING.md)**: Deterministic ordering function — tier-by-tier tie-breaker logic, invariants, and contributor workflow.
- **[Escrow Invariants](docs/escrow-invariants.md)**: Escrow state guarantees and safety properties
- **[Investment Lifecycle](docs/investment-lifecycle.md)**: Investment states and transitions
- **[Settlement & Fund Distribution](../docs/SETTLEMENT.md)**: How invoice settlement splits funds across investors, treasury, and the platform.
- **[Settlement & Dispute Interaction](docs/settlement-dispute-interaction.md)**: How settlements interact with disputes
- **[Invoice Search](docs/invoice-search-ranking.md)**: Invoice search and ranking algorithms
- **[Insurance Stacking](docs/insurance-stacking.md)**: Multiple insurance providers per investment
- **[Notifications Idempotency](docs/notifications-idempotency.md)**: Notification delivery guarantees
- **[Storage TTL Policy](docs/storage-ttl-policy.md)**: Storage lifetime management
- **[Storage TTL Map](../docs/STORAGE_TTL.md)**: Detailed mapping of each contract storage key to its bump amount
- **[Rounding Strategy](Rounding.md)**: Explanation of half‑up vs. banker rounding used in financial calculations.
- **[Protocol Health](docs/protocol-health.md)**: Health check endpoints and monitoring
- **[Error Catalog](docs/error-catalog.md)**: Complete error reference

## Test Harness Authorization

For test setup patterns and shared helpers, see the [contracts contributor guide](../docs/contracts/contributor-guide.md).

## Protocol Limits

See the [protocol limits reference](docs/contracts/limits.md) for configurable limits, defaults, and admin entrypoints.

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

This project is licensed under the MIT License.

## 🆘 Support

- **Documentation**: [docs.quicklendx.com](https://docs.quicklendx.com)
- **Discord**: [Join our community](https://discord.gg/quicklendx)
- **GitHub Issues**: [Report bugs](https://github.com/your-org/quicklendx-protocol/issues)
- **Email**: support@quicklendx.com

## 🔗 Links

- [Contract Emergency Response Runbook](../docs/EMERGENCY_RESPONSE.md)
- [Token Decimals — how non-standard decimals are handled internally](../docs/contracts/token-decimals.md)
- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Documentation](https://soroban.stellar.org/)
- [Rust Documentation](https://doc.rust-lang.org/)
- [QuickLendX Website](https://quicklendx.com)

---

**Built with ❤️ on Stellar's Soroban platform**

## Windows Development Notes

### Testing on Windows
Due to mingw linker limitations on Windows, you may encounter "export ordinal too large" errors when running tests that require the `cdylib` crate type. 

**Workaround for Windows testing:**
1. Temporarily change `Cargo.toml` crate-type to `["rlib"]` for local testing
2. Run `cargo test --lib` 
3. Restore `["rlib", "cdylib"]` before committing

**Production WASM builds work fine in CI (Linux environment).**

### Toolchain
- Uses `stable` Rust toolchain 
- Local Windows development uses GNU toolchain (`stable-x86_64-pc-windows-gnu`)
- Set with: `rustup override set stable-x86_64-pc-windows-gnu` in the contracts directory
