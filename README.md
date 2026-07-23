# QuickLendX Protocol

QuickLendX is a monorepo containing the complete protocol stack for decentralized invoice financing on Stellar Soroban.

## Packages

- `quicklendx-contracts/`: Smart contracts and contract tests for the QuickLendX protocol.
- `quicklendx-backend/`: Backend services, API schema, and server implementation.
- `quicklendx-frontend/`: Next.js frontend application for user interaction.

## Getting Started

### Smart Contracts

```bash
cd quicklendx-contracts
cargo build
cargo test
```

### Backend

```bash
cd quicklendx-backend
npm ci
npm run dev
```

### Frontend

```bash
cd quicklendx-frontend
npm ci
npm run dev
```

## Documentation

- [`docs/README.md`](docs/README.md): Full documentation index — **start here**.
  - [`docs/contracts/contributor-guide.md`](docs/contracts/contributor-guide.md): Contract contributor guide (module layout, lifecycle, error/event stability contracts, test patterns, WASM budget).
  - [`quicklendx-backend/docs/contributor-guide.md`](quicklendx-backend/docs/contributor-guide.md): Backend contributor guide (module layout, request pipeline, export/audit wiring, how to add an endpoint).
- `quicklendx-contracts/README.md`: Smart contract build, deploy, and API reference.
- `quicklendx-backend/README.md`: Backend-specific documentation.
- `quicklendx-frontend/README.md`: Frontend-specific documentation.

## Contribution

Please follow the repository guidelines in `AGENTS.md` and include tests for any behavior changes.
