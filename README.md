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
- `docs/RUSTDOC.md`: Auto-published rustdoc URL for the latest tag — start here if you are integrating with the contracts.
- `docs/RUNBOOK_INCIDENT_RESPONSE.md`: Operator playbook for unexpected contract behavior and incident-mode recovery.
- `docs/UPGRADE_PATHS.md`: Which protocol versions can upgrade to which, storage migration checklist, and rollback procedure.
- `quicklendx-contracts/README.md`: Smart contract-specific documentation.
- `quicklendx-contracts/docs/ATTESTATIONS.md`: Attestation event schemas and lifecycle for off-chain indexers — covers KYC verification events, invoice verification events, the `RawEvent` wire format, and idempotency guarantees.
- `quicklendx-backend/README.md`: Backend-specific documentation.
- `quicklendx-frontend/README.md`: Frontend-specific documentation.
- `docs/STORAGE_LAYOUT.md`: Smart contract storage layout decisions.

## Contribution

Please follow the repository guidelines in `AGENTS.md` and include tests for any behavior changes.
