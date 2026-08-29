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

  ### Backend

    cd quicklendx-backend
    npm ci
    npm run dev

  ### Frontend

    cd quicklendx-frontend
    npm ci
    npm run dev

  ## Documentation

  •  docs/ : Project-wide design, implementation, and audit documentation.
  •  docs/VESTING.md  /docs/VESTING.md: Vesting model, edge cases, and admin protections.
  •  docs/QUERIES.md  /docs/QUERIES.md: Catalog of common read-only entrypoints with concrete invocation examples and return values — the quickest way to find the query you need.
  •  docs/contracts/platform-fee-ops.md  /docs/contracts/platform-fee-ops.md: Admin operations playbook for managing fee rates, treasury rotation, and revenue splits.
  •  docs/RUNBOOK_INCIDENT_RESPONSE.md : Operator playbook for unexpected contract behavior and incident-mode recovery.
  •  docs/INVESTOR_TIER.md : How the investor risk score, tier, and investment limit are computed — math, thresholds, and worked examples.
  •  docs/BID_OVERBID_POLICY.md : What happens when a bid exceeds the invoice amount — rejection path and error code.
  •  docs/QLX_INVOICE_LOCK_TIME_LIMITS.md : Contributor-facing summary of invoice lock duration, auto-release behavior, and the default grace-period path.
   •  docs/QLX_INVOICE_TAXONOMY.md : Canonical invoice categories and statuses with transition examples.
  •  quicklendx-contracts/README.md : Smart contract-specific documentation.
  •  quicklendx-contracts/docs/contracts/deterministic-time.md : Smart contract deterministic ledger time semantics.
  •  quicklendx-backend/README.md : Backend-specific documentation.
  •  backend/docs/REPLAY_RUNBOOK.md : Step-by-step operator runbook for replaying ingestion from a specific ledger — covers reorg recovery, gap backfill, force rebuild after schema
  migration, and troubleshooting stuck runs.
  •  quicklendx-frontend/README.md : Frontend-specific documentation.
  •  docs/PLATFORM_FEES.md : Fee schedule and tenant override documentation.
  •  docs/PROFIT_SPLIT.md : How platform and investor fees are split.
  •  docs/INDEXING_CONTRACT.md : What the indexer relies on from the Soroban smart contracts — events, topics, data structures, and storage keys.

  ## Contribution

  Please follow the repository guidelines in  AGENTS.md  and include tests for any behavior changes.

### Frontend

```bash
cd quicklendx-frontend
npm ci
npm run dev
```

## Documentation

- `docs/`: Project-wide design, implementation, and audit documentation.
- `docs/INVESTMENT_LIFECYCLE_DIAGRAM.md`: Full state machine for investments.
- `docs/GOVERNANCE_QUORUM.md`: Quorum rules and participation weighting.
- [Platform Fee & Treasury Split Operations Guide](file:///c:/Users/HP/quicklendx-protocol/docs/contracts/platform-fee-ops.md): Admin operations playbook for managing fee rates, treasury rotation, and revenue splits.
- `docs/RUNBOOK_INCIDENT_RESPONSE.md`: Operator playbook for unexpected contract behavior and incident-mode recovery.
- `docs/INVESTOR_TIER.md`: How the investor risk score, tier, and investment limit are computed — math, thresholds, and worked examples.
- `docs/KYC.md`: Business KYC vs investor KYC, what each gates.
- [`docs/QLX_INVOICE_LOCK_TIME_LIMITS.md`](docs/QLX_INVOICE_LOCK_TIME_LIMITS.md): Contributor-facing summary of invoice lock duration, auto-release behavior, and the default grace-period path.
- [`docs/QLX_INVOICE_TAXONOMY.md`](docs/QLX_INVOICE_TAXONOMY.md): Canonical invoice categories and statuses with transition examples.
- `quicklendx-contracts/README.md`: Smart contract-specific documentation.
- `quicklendx-contracts/docs/contracts/deterministic-time.md`: Smart contract deterministic ledger time semantics.
- [`docs/README.md`](docs/README.md): Full documentation index — **start here**.
  - [`docs/contracts/contributor-guide.md`](docs/contracts/contributor-guide.md): Contract contributor guide (module layout, lifecycle, error/event stability contracts, test patterns, WASM budget).
  - [`quicklendx-backend/docs/contributor-guide.md`](quicklendx-backend/docs/contributor-guide.md): Backend contributor guide (module layout, request pipeline, export/audit wiring, how to add an endpoint).
- [`docs/AUDIT_PREP.md`](docs/AUDIT_PREP.md): Security audit preparation checklist for contributors.
- [Default Flow Diagram](docs/DEFAULT_FLOW_DIAGRAM.md): State-machine diagram from invoice past-due through default to recovery — grace period, finality guards, dispute interception, and a concrete ledger-timestamp walkthrough.
- [`docs/INVOICE_LIFECYCLE_DIAGRAM.md`](docs/INVOICE_LIFECYCLE_DIAGRAM.md): Full invoice state machine — all statuses, transitions, entrypoints, and invariants in one diagram. Start here when debugging invoice state issues. Closes #1946.
- [`docs/OFF_CHAIN_SIGNATURES.md`](docs/OFF_CHAIN_SIGNATURES.md): Threat model and implementation notes for every off-chain signed operation (KYC payloads, cursor attestations, dispute evidence). Closes #1894.
- `quicklendx-contracts/README.md`: Smart contract build, deploy, and API reference.
- `quicklendx-backend/README.md`: Backend-specific documentation.
- `backend/docs/REPLAY_RUNBOOK.md`: Step-by-step operator runbook for replaying ingestion from a specific ledger — covers reorg recovery, gap backfill, force rebuild after schema migration, and troubleshooting stuck runs.
- `quicklendx-frontend/README.md`: Frontend-specific documentation.
- `docs/PLATFORM_FEES.md`: Fee schedule and tenant override documentation.
- `docs/BID_RANKING.md`: Deterministic bid ranking ordering function — tier-by-tier tie-breaker logic, invariants, and contributor workflow.
- [`docs/QLX_BID_MATCH_ALGORITHM.md`](docs/QLX_BID_MATCH_ALGORITHM.md): Deterministic bid-matching algorithm specification, 5-tier evaluation hierarchy, and total ordering axioms.
- [`docs/BID_LIFECYCLE_DIAGRAM.md`](docs/BID_LIFECYCLE_DIAGRAM.md): Full bid state machine — status diagram, entrypoints, TTL config, expiry semantics, and invariants.
- [`docs/CURRENCY_WHITELIST.md`](docs/CURRENCY_WHITELIST.md): How tokens are added to and removed from the currency whitelist — contributor guide covering entrypoints, auth model, enforcement points, and test patterns.
- [`docs/ERROR_CODES.md`](docs/ERROR_CODES.md): Complete catalog of every contract error code (QuickLendXError and FreshnessError) with numeric codes, ABI symbols, and meanings.
- [`docs/FEES_GRACE_DEFAULT.md`](docs/FEES_GRACE_DEFAULT.md): Unified contributor reference — platform fees, grace period resolution, and default trigger rules in one place.
- [`docs/GOVERNANCE.md`](docs/GOVERNANCE.md): Governance model, admin handover (one-step and two-step) flow, and the emergency-withdraw timelock — operator-facing.
- [`docs/QLX_MULTISIG_ROTATION.md`](docs/QLX_MULTISIG_ROTATION.md): Multisig signer rotation procedure with timelock — step-by-step operator guide for safely rotating signers on a multisig-controlled admin account.

## Contribution

Please follow the repository guidelines in `AGENTS.md` and include tests for any behavior changes.
- `docs/QLX_OWNERSHIP_MODEL.md`: Ownership model for invoices and investor bids.
