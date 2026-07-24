# Documentation index

## Contract reference

| Document | What it covers |
|----------|---------------|
| [contributor-guide.md](contracts/contributor-guide.md) | **Start here if you are new to the contracts codebase.** Module layout, build commands, invoice lifecycle, bidding, escrow, KYC gates, error/event stability contracts, test auth pattern, WASM budget. |
| [invoice-lifecycle.md](contracts/invoice-lifecycle.md) | Full invoice state machine, transition table, investment status integration |
| [errors.md](contracts/errors.md) | Error code reference (stable integers) |
| [events.md](contracts/events.md) | Event schema and topic constants |
| [security.md](contracts/security.md) | Reentrancy guard, pause circuit breaker, access control |
| [protocol-limits.md](contracts/protocol-limits.md) | Configurable numeric limits and string-length caps |
| [fees.md](contracts/fees.md) | Fee management and revenue distribution |
| [settlement.md](contracts/settlement.md) | Settlement flows and partial payments |
| [dispute.md](contracts/dispute.md) | Dispute lifecycle and resolution |
| [escrow.md](contracts/escrow.md) | Escrow creation, release, and refund |
| [bidding.md](contracts/bidding.md) | Bid ranking, TTL, and cleanup |
| [admin.md](contracts/admin.md) | Admin setup and transfer |
| [storage-schema.md](contracts/storage-schema.md) | Persistent storage keys and index layout |
| [backup.md](contracts/backup.md) | Backup and restore |
| [audit.md](contracts/audit.md) | Audit trail and hash chain |

## Operator reference

| Document | What it covers |
|----------|---------------|
| [EVENT_DASHBOARDS.md](EVENT_DASHBOARDS.md) | Standard Grafana dashboards — panel URLs, PromQL queries, SQLite indexer queries, and alert rules for protocol health, event throughput, disputes, and settlement pipeline |
| [MONITORING.md](MONITORING.md) | Per-event alert thresholds for contract events |
| [DASHBOARD_QUERIES.md](DASHBOARD_QUERIES.md) | Full SQL reference for indexer health and workload queries |
| [RUNBOOK_INCIDENT_RESPONSE.md](RUNBOOK_INCIDENT_RESPONSE.md) | Step-by-step operator playbook for unexpected contract behavior |

## UX / frontend reference

See [ux/](ux/) for component-level design specifications.

## Security

See [security/](security/) for KYC key handling and settlement security notes.
