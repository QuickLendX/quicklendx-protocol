# QuickLendX Protocol

[![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Soroban](https://img.shields.io/badge/Soroban-000000?style=for-the-badge&logo=stellar&logoColor=white)](https://soroban.stellar.org/)
[![Next.js](https://img.shields.io/badge/Next.js-000000?style=for-the-badge&logo=next.js&logoColor=white)](https://nextjs.org/)
[![License](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

**QuickLendX** is a decentralized invoice financing protocol built on **Stellar's Soroban** platform. It enables businesses to access working capital by selling their invoices to investors through a transparent, secure, and efficient blockchain-based marketplace.

## 🎯 What is QuickLendX?

QuickLendX revolutionizes invoice financing by leveraging blockchain technology to create a trustless, efficient marketplace where:

- **Businesses** can upload verified invoices and receive immediate funding from investors
- **Investors** can discover, evaluate, and bid on invoices with competitive rates
- **All parties** benefit from automated escrow, transparent audit trails, and comprehensive analytics

Built on Stellar's Soroban smart contract platform, QuickLendX provides enterprise-grade features including KYC/verification, dispute resolution, insurance options, and comprehensive reporting—all while maintaining the security and transparency of blockchain technology.

## 👥 Who is this for?

- **Small and Medium Businesses (SMBs)**: Companies seeking flexible working capital solutions without traditional banking constraints
- **Investors**: Individuals and institutions looking for alternative investment opportunities with transparent risk assessment
- **DeFi Enthusiasts**: Users interested in decentralized finance applications on the Stellar network
- **Developers**: Contributors looking to build on or extend the QuickLendX protocol

## 🏗️ Project Structure

```
quicklendx-protocol/
├── quicklendx-contracts/    # Soroban smart contracts (Rust)
│   ├── src/                 # Contract source code
│   ├── Cargo.toml          # Rust dependencies
│   └── README.md           # Contracts documentation
│
└── quicklendx-frontend/     # Next.js web application
    ├── app/                 # Next.js app directory
    ├── package.json         # Node.js dependencies
    └── README.md           # Frontend documentation
```

## 🚀 Quick Start

### Prerequisites

- **Rust** (1.70+): [Install via rustup](https://rustup.rs/)
- **Node.js** (18+): [Download](https://nodejs.org/)
- **Stellar CLI** (23.0.0+): [Installation Guide](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup)
- **Git**: [Download](https://git-scm.com/)

### Installation

1. **Clone the repository**

```bash
git clone https://github.com/your-org/quicklendx-protocol.git
cd quicklendx-protocol
```

2. **Set up Smart Contracts**

```bash
cd quicklendx-contracts
cargo build
cargo test
```

3. **Set up Frontend**

```bash
cd ../quicklendx-frontend
npm install
```

### Environment Setup

#### Smart Contracts

Create a `.env` file in `quicklendx-contracts/` (optional for local development):

```bash
# Network Configuration
NETWORK=testnet
CONTRACT_ID=your_contract_id_here

# Account Configuration
ADMIN_ADDRESS=your_admin_address
```

#### Frontend

Create a `.env.local` file in `quicklendx-frontend/`:

```bash
# API Configuration
NEXT_PUBLIC_CONTRACT_ID=your_contract_id_here
NEXT_PUBLIC_NETWORK=testnet
NEXT_PUBLIC_RPC_URL=https://soroban-testnet.stellar.org:443
```

### Running the Project

#### Start Local Soroban Network

```bash
stellar-cli network start
```

#### Deploy Contracts (Local)

```bash
cd quicklendx-contracts
cargo build --target wasm32-unknown-unknown --release
stellar-cli contract deploy \
    --wasm target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm \
    --source admin
```

#### Start Frontend Development Server

```bash
cd quicklendx-frontend
npm run dev
```

Open [http://localhost:3000](http://localhost:3000) in your browser.

### WASM build and size budget

The contract must stay within the network deployment size limit (256 KB). You can **run the script** and/or the **integration test**:

**Option 1 – Script (builds with Stellar CLI or cargo):**

```bash
cd quicklendx-contracts
./scripts/check-wasm-size.sh
```

**Option 2 – Integration test (builds with cargo, then asserts size):**

```bash
cd quicklendx-contracts
cargo test wasm_release_build_fits_size_budget
```

Both build the contract for Soroban (release, no test-only code) and fail if the WASM exceeds 256 KB. CI runs the script on every push/PR.

### Testing

#### Test Smart Contracts

```bash
cd quicklendx-contracts
cargo test
```

#### Test Frontend

```bash
cd quicklendx-frontend
npm run test  # If tests are configured
npm run lint
```

### Network Deployment

#### Testnet Deployment

```bash
# Configure for testnet
stellar-cli network testnet

# Deploy contract
stellar-cli contract deploy \
    --wasm target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm \
    --source <YOUR_ACCOUNT> \
    --network testnet
```

#### Mainnet Deployment

⚠️ **Important**: Mainnet deployment requires thorough testing and security audits.

```bash
stellar-cli contract deploy \
    --wasm target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm \
    --source <DEPLOYER_ACCOUNT> \
    --network mainnet
```

## 📚 Documentation

- **[Smart Contracts Documentation](./quicklendx-contracts/README.md)**: Comprehensive guide to the Soroban contracts
- **[Frontend Documentation](./quicklendx-frontend/README.md)**: Frontend setup and development guide
- **[Contributing Guide](./quicklendx-contracts/CONTRIBUTING.md)**: How to contribute to the project

## 🔗 Helpful Links

### Stellar & Soroban

- [Stellar Documentation](https://developers.stellar.org/)
- [Soroban Documentation](https://soroban.stellar.org/)
- [Soroban SDK Reference](https://docs.rs/soroban-sdk/)
- [Stellar CLI Guide](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup)

### Development Resources

- [Rust Documentation](https://doc.rust-lang.org/)
- [Next.js Documentation](https://nextjs.org/docs)
- [TypeScript Documentation](https://www.typescriptlang.org/docs/)

### Project Resources

- [GitHub Repository](https://github.com/your-org/quicklendx-protocol)
- [Issue Tracker](https://github.com/your-org/quicklendx-protocol/issues)
- [Discord Community](https://discord.gg/quicklendx) _(if available)_

## 🏛️ Architecture Overview

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
                    │ • Analytics     │
                    └─────────────────┘
```

## ✨ Key Features

- ✅ **Invoice Management**: Upload, verify, and manage business invoices
- ✅ **Bidding System**: Competitive bidding with ranking algorithms
- ✅ **Escrow Management**: Secure fund handling through smart contract escrows
- ✅ **KYC/Verification**: Business and investor verification with risk assessment
- ✅ **Audit Trail**: Complete transaction history and integrity validation
- ✅ **Analytics & Reporting**: Comprehensive metrics and business intelligence
- ✅ **Dispute Resolution**: Built-in dispute handling and resolution
- ✅ **Insurance Options**: Investment protection mechanisms
- ✅ **Multi-currency Support**: Handle invoices in various currencies
- ✅ **Notification System**: Real-time updates for all parties

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](./quicklendx-contracts/CONTRIBUTING.md) for details.

### Development Workflow

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Add tests and ensure they pass
5. Update documentation
6. Commit your changes (`git commit -m 'Add amazing feature'`)
7. Push to the branch (`git push origin feature/amazing-feature`)
8. Open a Pull Request

### Code Review Process

1. Automated checks must pass (tests, linting)
2. Code review by maintainers
3. Security review for critical changes
4. Documentation updates required

## 📋 Requirements

### Smart Contracts

- Rust 1.70+
- Stellar CLI 23.0.0+
- WASM target: `wasm32-unknown-unknown` or `wasm32v1-none` (Soroban)
- **WASM size budget**: 256 KB (enforced in CI and via `quicklendx-contracts/scripts/check-wasm-size.sh`)

### Frontend

- Node.js 18+
- npm or yarn
- Modern browser with Web3 support

## 🧪 Testing

### Smart Contracts

```bash
cd quicklendx-contracts
cargo test
cargo test --profile release-with-logs  # With debug logging
```

### Frontend

```bash
cd quicklendx-frontend
npm run test
npm run lint
npm run build  # Production build test
```

## 📄 License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## 👥 Contributors

<!-- Add contributors here -->

- Your Name - _Initial work_

## 🆘 Support

- **Documentation**: Check the [contracts README](./quicklendx-contracts/README.md) and [frontend README](./quicklendx-frontend/README.md)
- **Issues**: [GitHub Issues](https://github.com/your-org/quicklendx-protocol/issues)
- **Discussions**: [GitHub Discussions](https://github.com/your-org/quicklendx-protocol/discussions)

---

**Built with ❤️ on Stellar's Soroban platform**
