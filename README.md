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
- `docs/BACKUP_RETENTION.md`: Retention policy for contract backups and integrity verification.
- `docs/RUNBOOK_INCIDENT_RESPONSE.md`: Operator playbook for unexpected contract behavior and incident-mode recovery.
- `docs/INVESTOR_TIER.md`: How the investor risk score, tier, and investment limit are computed — math, thresholds, and worked examples.
- `quicklendx-contracts/README.md`: Smart contract-specific documentation.
- `quicklendx-contracts/docs/ATTESTATIONS.md`: Attestation event schemas and lifecycle for off-chain indexers — covers KYC verification events, invoice verification events, the `RawEvent` wire format, and idempotency guarantees.
- `quicklendx-backend/README.md`: Backend-specific documentation.
- `backend/docs/REPLAY_RUNBOOK.md`: Step-by-step operator runbook for replaying ingestion from a specific ledger — covers reorg recovery, gap backfill, force rebuild after schema migration, and troubleshooting stuck runs.
- `quicklendx-frontend/README.md`: Frontend-specific documentation.
- `docs/STORAGE_LAYOUT.md`: Smart contract storage layout decisions.

## Contribution

Please follow the repository guidelines in `AGENTS.md` and include tests for any behavior changes.
