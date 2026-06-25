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

- `docs/`: Project-wide design, implementation, and audit documentation.
- `docs/RUNBOOK_INCIDENT_RESPONSE.md`: Operator playbook for unexpected contract behavior and incident-mode recovery.
- `quicklendx-contracts/README.md`: Smart contract-specific documentation.
- `quicklendx-contracts/docs/contracts/deterministic-time.md`: Smart contract deterministic ledger time semantics.
- `quicklendx-backend/README.md`: Backend-specific documentation.
- `quicklendx-frontend/README.md`: Frontend-specific documentation.

## Contribution

Please follow the repository guidelines in `AGENTS.md` and include tests for any behavior changes.
