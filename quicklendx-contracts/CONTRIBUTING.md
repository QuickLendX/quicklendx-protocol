# Contributing to Quicklendx (Stellar-CLI/Soroban-sdk)

Thank you for your interest in contributing! Whether you're fixing bugs, improving documentation, or adding new smart contract **we appreciate your effort**.

This guide will help you get started quickly.

---

## Prequisities

Before you begin, make sure you have the following installed:

### Required Tools

- **Rust** Install via [rustup](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

- Install the [stellar-CLI](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup#install-the-stellar-cli)

Install with Homebrew (macOS, Linux):
```bash
brew install stellar-cli
```

Install with cargo from source:
```bash
cargo install --locked stellar-cli@23.0.0
```

- for the windows it is available via [Stellar installation for windows](https://developers.stellar.org/docs/build/smart-contracts/getting-started/setup#install-the-stellar-cli)

---

### 🧪Running Project

- From the root of the project, run the following command:

```bash
cargo build
```

This compiles the contract using the top-level workspace configuration.

- To run contract tests, use the following command:

```bash
cargo test
```

---

# ✍️ How to Contribute

1. Fork the repository on GitHub and clone it locally:

```bash
git clone https://github.com/your-username/quicklendx-contracts.git
cd quicklendx-contracts
```

2. Create a new branch for your changes:

```bash
git checkout -b feature/your-feature-name
```

3. Make your changes in the appropriate contract directory under you are assigned to.

4. Build the contract:

```bash
cargo build
```

5. Run the contract tests:

```bash
cargo test
```

6. Commit your changes and push to your fork:

```bash
git add .
git commit -m "Add your commit message"
git push origin feature/your-feature-name
```

7. Open a Pull Request to the main branch of this repository via Github.

---

# 💡Suggesting Improvements

if you encounter a bug or have an idea for improvement,feel free to open an issue and describe it. Contributions are welcome!
