# Documentation index

## Contract reference

| Document | What it covers |
|----------|---------------|
| [contributor-guide.md](contracts/contributor-guide.md) | **Start here if you are new to the contracts codebase.** Module layout, build commands, invoice lifecycle, bidding, escrow, KYC gates, error/event stability contracts, test auth pattern, WASM budget. |
| [invoice-lifecycle.md](contracts/invoice-lifecycle.md) | Full invoice state machine, transition table, investment status integration |
| [INVOICE_LIFECYCLE_DIAGRAM.md](INVOICE_LIFECYCLE_DIAGRAM.md) | Full invoice state machine diagram — all statuses, transitions, invariants, and entrypoint signatures in one page (issue #1946) |
| [OFF_CHAIN_SIGNATURES.md](OFF_CHAIN_SIGNATURES.md) | Threat model and implementation notes for all off-chain signed operations: KYC payloads, cursor attestations, dispute evidence (issue #1894) |
| [DEFAULT_FLOW_DIAGRAM.md](DEFAULT_FLOW_DIAGRAM.md) | State-machine diagram from invoice past-due → default → recovery; grace period, finality guards, dispute interception, and concrete timeline example |
| [QLX_INVOICE_LOCK_TIME_LIMITS.md](QLX_INVOICE_LOCK_TIME_LIMITS.md) | Contributor-facing summary of the practical invoice lock time limits, auto-release behavior, and the default grace-period path |
| [errors.md](contracts/errors.md) | Error code reference (stable integers) |
| [events.md](contracts/events.md) | Event schema and topic constants |
| [security.md](contracts/security.md) | Reentrancy guard, pause circuit breaker, access control |
| [protocol-limits.md](contracts/protocol-limits.md) | Configurable numeric limits and string-length caps |
| [fees.md](contracts/fees.md) | Fee management and revenue distribution |
| [PROFIT_SPLIT.md](PROFIT_SPLIT.md) | How platform and investor fees are split |
| [settlement.md](contracts/settlement.md) | Settlement flows and partial payments |
| [dispute.md](contracts/dispute.md) | Dispute lifecycle and resolution |
| [escrow.md](contracts/escrow.md) | Escrow creation, release, and refund |
| [bidding.md](contracts/bidding.md) | Bid ranking, TTL, and cleanup |
| [QLX_BID_MATCH_ALGORITHM.md](QLX_BID_MATCH_ALGORITHM.md) | Deterministic 5-tier bid-matching comparison algorithm write-up and total-ordering specification |
| [admin.md](contracts/admin.md) | Admin setup and transfer |
| [storage-schema.md](contracts/storage-schema.md) | Persistent storage keys and index layout |
| [backup.md](contracts/backup.md) | Backup and restore |
| [audit.md](contracts/audit.md) | Audit trail and hash chain |
| [CROSS_INVOICE_ANALYTICS.md](CROSS_INVOICE_ANALYTICS.md) | Cross-invoice read patterns, supported entrypoints, and pagination bounds for contributors and integrators |
| [QLX_REPORT_LIFECYCLE.md](QLX_REPORT_LIFECYCLE.md) | Analytics report lifecycle — Requested → Delivered → Archived, entrypoints, storage layout, and invariants |
| [QLX_RISK_PARAMETERS.md](QLX_RISK_PARAMETERS.md) | Complete catalog of risk-related parameters with min/max/default values — investor tiers, business supply limits, bid controls, fee ceilings, and operational bounds |
| [UPGRADE_QUIESCE.md](UPGRADE_QUIESCE.md) | How writes drain before a contract upgrade — maintenance mode, drain window, and operator checklist |
| [QLX_INSURANCE_CLAIM_LIFECYCLE.md](QLX_INSURANCE_CLAIM_LIFECYCLE.md) | From opt-in to claim to close. The insurance claim lifecycle for downstream integrators and operators |
| [OPERATIONAL_PLAYBOOKS.md](OPERATIONAL_PLAYBOOKS.md) | Protocol reasons, meaning, and operational playbooks |
| [QLX_INDEXER_CONTRACT.md](QLX_INDEXER_CONTRACT.md) | What the off-chain indexer relies on from the on-chain contract (events, structures) |

## Backend reference

| Document | What it covers |
|----------|---------------|
| [quicklendx-backend/docs/contributor-guide.md](../quicklendx-backend/docs/contributor-guide.md) | **Start here if you are new to the backend.** Module layout, request pipeline, export/audit wiring, metrics, how to add an endpoint. |
| [quicklendx-backend/docs/configuration.md](../quicklendx-backend/docs/configuration.md) | All environment variables with types, defaults, and production rules |
| [quicklendx-backend/docs/exports.md](../quicklendx-backend/docs/exports.md) | Export limits, streaming, formats, and integrity digest |
| [quicklendx-backend/docs/observability.md](../quicklendx-backend/docs/observability.md) | Prometheus metrics, Grafana queries, alert rules |
| [quicklendx-backend/docs/testing.md](../quicklendx-backend/docs/testing.md) | Contract testing, fixtures, and coverage requirements |

## Operator reference

| Document | What it covers |
|----------|---------------|
| [OPERATOR_HANDBOOK.md](OPERATOR_HANDBOOK.md) | Every admin/operator-facing entrypoint with concrete CLI examples — initialization, pause, emergency, currency whitelist, disputes, fees, KYC, backup, upgrade, and common workflows |
| [EVENT_DASHBOARDS.md](EVENT_DASHBOARDS.md) | Standard Grafana dashboards — panel URLs, PromQL queries, SQLite indexer queries, and alert rules for protocol health, event throughput, disputes, and settlement pipeline |
| [QLX_GOVERNANCE_PROPOSALS.md](QLX_GOVERNANCE_PROPOSALS.md) | Governance proposal lifecycle status transitions and operator workflows |
| [QLX_TREASURY_ROTATION.md](QLX_TREASURY_ROTATION.md) | Treasury address rotation flow with two-step validation and timelock |
| [QLX_MULTISIG_CONFIG.md](QLX_MULTISIG_CONFIG.md) | Multisig setup, signer rotation, and threshold-signature verification for critical operations |
| [QLX_DISPUTE_TIME_LIMITS.md](QLX_DISPUTE_TIME_LIMITS.md) | Dispute creation deadline, grace period, and auto-close threshold for operator reference |
| [MONITORING.md](MONITORING.md) | Per-event alert thresholds for contract events |
| [DASHBOARD_QUERIES.md](DASHBOARD_QUERIES.md) | Full SQL reference for indexer health and workload queries |
| [RUNBOOK_INCIDENT_RESPONSE.md](RUNBOOK_INCIDENT_RESPONSE.md) | Step-by-step operator playbook for unexpected contract behavior |
| [QLX_INVESTIGATION_WORKFLOW.md](QLX_INVESTIGATION_WORKFLOW.md) | End-to-end investigation flow with SLA |

## UX / frontend reference

See [ux/](ux/) for component-level design specifications.

## Security

See [security/](security/) for KYC key handling and settlement security notes.
