# Documentation Index

Categorized index of all documentation in `docs/` (including `docs/contracts/`, `docs/security/`, `docs/ux/`, and `docs/superpowers/`). Every document is linked once. Superseded or empty stub documents are explicitly marked.

## Getting Started & Contributor Guides

| Document | What it covers |
|----------|----------------|
| [contracts/contributor-guide.md](contracts/contributor-guide.md) | **Start here if you are new to the contracts codebase.** Module layout, build commands, lifecycle, KYC gates, test auth, and WASM budget. |
| [RUSTDOC.md](RUSTDOC.md) | Downstream integrator API reference and rustdoc generation guide for QuickLendX Soroban contracts. |
| [CONTRACT_VERSION_COMPATIBILITY.md](CONTRACT_VERSION_COMPATIBILITY.md) | Version interoperability rules and breaking-change guarantees across QuickLendX contract releases. |
| [GENERATION_COUNTER.md](GENERATION_COUNTER.md) | Protocol generation counter semantics, consumers, and how to read it on-chain. |
| [FEATURE_FLAGS.md](FEATURE_FLAGS.md) | Guidelines for adding and managing feature-gated capabilities in the protocol. |
| [contracts/initialization.md](contracts/initialization.md) | Secure one-time contract initialization flow and required setup parameters. |
| [contracts/errors.md](contracts/errors.md) | Canonical `QuickLendXError` error code reference with stable integer values. |
| [ERROR_CODES.md](ERROR_CODES.md) | Smart-contract error code catalog and reviewer reference for error handling. |
| [contracts/token-decimals.md](contracts/token-decimals.md) | Token decimal precision handling for contract integrators and off-chain tooling. |
| [diagnostics.md](diagnostics.md) | Uniform structured diagnostics system using the `qlx_log!` macro in Soroban contracts. |
| [contracts/notifications.md](contracts/notifications.md) | On-chain notification module for real-time protocol event alerts. |

## Lifecycle

### Invoices, Bids & Investments

| Document | What it covers |
|----------|----------------|
| [INVOICE_LIFECYCLE.md](INVOICE_LIFECYCLE.md) | End-to-end invoice lifecycle states, transitions, and operational semantics. |
| [INVOICE_LIFECYCLE_DIAGRAM.md](INVOICE_LIFECYCLE_DIAGRAM.md) | One-page invoice state-machine diagram covering all statuses, transitions, invariants, and entrypoints. |
| [contracts/invoice-lifecycle.md](contracts/invoice-lifecycle.md) | Contract-level invoice lifecycle management, transition table, and investment status integration. |
| [contracts/lifecycle.md](contracts/lifecycle.md) | Cross-module invoice lifecycle state machine and consistency invariants. |
| [LIFECYCLE_COMPOSITION.md](LIFECYCLE_COMPOSITION.md) | Composition of origination, late, default, and dispute phases with concrete entrypoint walkthroughs. |
| [contracts/invoice.md](contracts/invoice.md) | Deterministic, collision-resistant invoice ID generation and prevention rules. |
| [contracts/invoice-metadata.md](contracts/invoice-metadata.md) | Invoice metadata fields, categorization, and tagging implementation. |
| [QLX_INVOICE_TAXONOMY.md](QLX_INVOICE_TAXONOMY.md) | Canonical invoice categories and statuses with transition table and examples. |
| [QLX_INVOICE_TAGS.md](QLX_INVOICE_TAGS.md) | Invoice tag system constraints, normalization, and usage rules. |
| [INVOICE_LOCK.md](INVOICE_LOCK.md) | Invoice lock duration mechanics and auto-release conditions. |
| [QLX_INVOICE_LOCK_TIME_LIMITS.md](QLX_INVOICE_LOCK_TIME_LIMITS.md) | Summary of practical invoice lock time limits, auto-release behavior, and default grace-period path. |
| [BID_LIFECYCLE_DIAGRAM.md](BID_LIFECYCLE_DIAGRAM.md) | Complete bid lifecycle state machine from placement through acceptance, cancellation, or expiration. |
| [contracts/bidding.md](contracts/bidding.md) | Bid cancellation authorization rules (strictly investor-only) and bid lifecycle controls. |
| [contracts/bidding_cleanup.md](contracts/bidding_cleanup.md) | Multi-stage expired bid cleanup architecture and index safety guarantees. |
| [BID_RANKING.md](BID_RANKING.md) | Consensus-critical deterministic bid ranking and ordering function. |
| [contracts/bid-ranking.md](contracts/bid-ranking.md) | Multi-level `BidStorage::compare_bids` deterministic tie-breaker specification. |
| [QLX_BID_MATCH_ALGORITHM.md](QLX_BID_MATCH_ALGORITHM.md) | Deterministic 5-tier bid-matching comparison algorithm and total-ordering specification. |
| [BID_CONCURRENCY.md](BID_CONCURRENCY.md) | Bid concurrency, Soroban sequential execution model, and race-safety guarantees. |
| [BID_OVERBID_POLICY.md](BID_OVERBID_POLICY.md) | Policy rejecting investor bids whose `bid_amount` exceeds `invoice.amount` at placement. |
| [PARTIAL_FILLS.md](PARTIAL_FILLS.md) | Partial fill mechanics and atomic bid acceptance/funding semantics. |
| [INVESTMENT_LIFECYCLE_DIAGRAM.md](INVESTMENT_LIFECYCLE_DIAGRAM.md) | Comprehensive investment state-machine diagram and transitions. |
| [QLX_INVESTMENT_STATES.md](QLX_INVESTMENT_STATES.md) | Full investment state machine documentation and terminal state rules. |
| [contracts/investment.md](contracts/investment.md) | Rustdoc-style investment lifecycle management and transition invariants. |

### Escrow, Settlement, Defaults, Disputes, Insurance & Vesting

| Document | What it covers |
|----------|----------------|
| [ESCROW.md](ESCROW.md) | Escrow overview covering fund locking, release, and refund flows upon bid acceptance. |
| [contracts/escrow.md](contracts/escrow.md) | Contract-level escrow lifecycle, state transitions, and fund custody rules. |
| [contracts/escrow-refund.md](contracts/escrow-refund.md) | Security hardening improvements for the escrow refund mechanism. |
| [QLX_ESCROW_TIME_LIMITS.md](QLX_ESCROW_TIME_LIMITS.md) | Escrow `Held` status duration limits, release/refund paths, grace period, and default handling. |
| [contracts/payments.md](contracts/payments.md) | Payments module covering token transfers and escrow lifecycle operations. |
| [SETTLEMENT.md](SETTLEMENT.md) | Invoice settlement and fund distribution when a business repays a funded invoice. |
| [contracts/settlement.md](contracts/settlement.md) | Settlement module payment record storage architecture, bounded storage, and partial payments. |
| [SETTLEMENT_ACCOUNTING.md](SETTLEMENT_ACCOUNTING.md) | Partial-payment accounting model and auto-release trigger semantics. |
| [contracts/settlement-formula.md](contracts/settlement-formula.md) | Settlement math formula, inputs, and state update timing. |
| [QLX_SETTLEMENT_TERMS.md](QLX_SETTLEMENT_TERMS.md) | Settlement terms governing the transition of funded invoices from `Funded` to `Paid`. |
| [QLX_SETTLEMENT_ROUNDING.md](QLX_SETTLEMENT_ROUNDING.md) | Rounding and precision policies across currencies in the settlement engine. |
| [DEFAULT_FLOW_DIAGRAM.md](DEFAULT_FLOW_DIAGRAM.md) | State-machine diagram from past-due to default to recovery, with grace period and dispute interception. |
| [DEFAULT_ACCOUNTING.md](DEFAULT_ACCOUNTING.md) | How defaulted invoices roll into ratings and the on-chain audit trail. |
| [default-finality-matrix.md](default-finality-matrix.md) | Canonical decision matrix for when `mark_invoice_defaulted` is permitted or blocked. |
| [contracts/default-handling.md](contracts/default-handling.md) | Two-phase overdue funded invoice default handling logic. |
| [contracts/defaults.md](contracts/defaults.md) | Access control, grace period, and status validation for manual invoice default marking. |
| [DISPUTE.md](DISPUTE.md) | High-level dispute lifecycle for opening, reviewing, and resolving invoice disputes. |
| [contracts/dispute.md](contracts/dispute.md) | Contract-level dispute management and resolution mechanisms. |
| [DISPUTE_EVIDENCE_IDENTITY.md](DISPUTE_EVIDENCE_IDENTITY.md) | Dispute evidence identity, immutability, and lifecycle rules. |
| [QLX_DISPUTE_TIME_LIMITS.md](QLX_DISPUTE_TIME_LIMITS.md) | Time windows for opening disputes and `DisputeTimeLimitExceeded` enforcement. |
| [contracts/dispute-timeline.md](contracts/dispute-timeline.md) | Chronologically ordered, redacted dispute timeline endpoint specification. |
| [contracts/resolution-policy.md](contracts/resolution-policy.md) | Per-contract dispute resolution policy recorded by admins when disputes resolve. |
| [APPEALS.md](APPEALS.md) | Operator process for reviewing and resolving dispute appeals. |
| [contracts/investment-insurance.md](contracts/investment-insurance.md) | Investment insurance module for attaching coverage to investor positions. |
| [QLX_INSURANCE_CLAIM_LIFECYCLE.md](QLX_INSURANCE_CLAIM_LIFECYCLE.md) | Insurance claim lifecycle from opt-in through claim submission to closure. |
| [QLX_INSURANCE_TIME_LIMITS.md](QLX_INSURANCE_TIME_LIMITS.md) | Time limits for filing insurance claims following an invoice default. |
| [VESTING.md](VESTING.md) | Time-locked vesting model overview for protocol tokens and rewards. |
| [contracts/vesting.md](contracts/vesting.md) | Vesting contract module specification and beneficiary claim mechanics. |

## Fees, Economics, Risk & Limits

| Document | What it covers |
|----------|----------------|
| [FEES.md](FEES.md) | Protocol fee and treasury guide for operators and reviewers. |
| [PLATFORM_FEES.md](PLATFORM_FEES.md) | Fee schedule, volume tier progression, and manual operator fee processes. |
| [contracts/fees.md](contracts/fees.md) | Configurable platform fee system and treasury routing implementation. |
| [contracts/platform-fee-ops.md](contracts/platform-fee-ops.md) | Operator guide for managing platform fee rates and treasury splits. |
| [FEES_GRACE_DEFAULT.md](FEES_GRACE_DEFAULT.md) | Interaction between platform fee models, grace periods, and default triggers. |
| [PROFIT_SPLIT.md](PROFIT_SPLIT.md) | How platform fees and investor returns are split on invoice settlement. |
| [contracts/profit-fee-formula.md](contracts/profit-fee-formula.md) | Centralized profit and fee calculation formula used in invoice settlement. |
| [contracts/revenue-split.md](contracts/revenue-split.md) | Admin-configurable revenue split mechanism across protocol recipients. |
| [CAPS.md](CAPS.md) | Protocol capacity boundaries and hard limits (`CAPS`). |
| [contracts/limits.md](contracts/limits.md) | Hard limits on invoice amounts, due-date horizons, and related bounds. |
| [contracts/protocol-limits.md](contracts/protocol-limits.md) | Configurable numeric limits, string-length caps, and boundary values. |
| [QLX_RISK_PARAMETERS.md](QLX_RISK_PARAMETERS.md) | Complete catalog of risk parameters with min/max/default values across tiers, supply, bids, and fees. |
| [QLX_ONBOARDING_LIMITS.md](QLX_ONBOARDING_LIMITS.md) | Progressive onboarding limits for new businesses and investors building protocol history. |
| [INVESTOR_EXPOSURE.md](INVESTOR_EXPOSURE.md) | Atomic investor exposure caps treated as active reservation limits. |
| [INVESTOR_RISK_MODEL.md](INVESTOR_RISK_MODEL.md) | Investor numeric risk score calculation and derived tier label model. |
| [INVESTOR_TIER.md](INVESTOR_TIER.md) | Investor rating and tier algorithm implementation reference. |
| [RATING_OVERRIDES.md](RATING_OVERRIDES.md) | Operator and support policy for manual invoice rating overrides. |
| [CURRENCY_WHITELIST.md](CURRENCY_WHITELIST.md) | Contributor reference for the admin-managed currency whitelist. |
| [CURRENCY_ALLOWLIST_POLICY.md](CURRENCY_ALLOWLIST_POLICY.md) | Versioned payment-token allowlist and `payment_token_policy::authorize_payment` rules. |
| [contracts/currency-whitelist.md](contracts/currency-whitelist.md) | Multi-currency whitelist enforcement for invoice creation and bidding. |
| [contracts/currency.md](contracts/currency.md) | Concise currency whitelist summary *(superseded by `contracts/currency-whitelist.md`)*. |

## KYC, Access Control & Governance

| Document | What it covers |
|----------|----------------|
| [KYC.md](KYC.md) | Operator guide to Business KYC and Investor KYC workflows. |
| [KYC_ELIGIBILITY_POLICY.md](KYC_ELIGIBILITY_POLICY.md) | KYC eligibility enforcement across all non-terminal invoice, bid, funding, and settlement entrypoints. |
| [contracts/business-kyc.md](contracts/business-kyc.md) | Mandatory Business KYC verification flow for invoice uploaders. |
| [contracts/investor-kyc.md](contracts/investor-kyc.md) | Investor KYC verification and investment limit enforcement. |
| [QLX_BUSINESS_KYC_TIERS.md](QLX_BUSINESS_KYC_TIERS.md) | Business KYC status system and tier implementation reference. |
| [QLX_INVESTOR_KYC_TIERS.md](QLX_INVESTOR_KYC_TIERS.md) | Investor KYC tiers, requirements, and gating operations for operators and support teams. |
| [contracts/verification.md](contracts/verification.md) | Centralized `verification.rs` guard system enforcing restricted finance actions. |
| [contracts/regulatory.md](contracts/regulatory.md) | `require_regulatory_ok` compliance hook seam in the contract. |
| [contracts/access-control.md](contracts/access-control.md) | Single-admin role model for protocol configuration and emergency recovery. |
| [contracts/admin.md](contracts/admin.md) | Admin setup and two-step admin transfer safety model in `admin.rs`. |
| [GOVERNANCE.md](GOVERNANCE.md) | Operator governance guide for running a QuickLendX deployment. |
| [GOVERNANCE_QUORUM.md](GOVERNANCE_QUORUM.md) | Governance quorum rules and participation weighting mechanics. |
| [QLX_GOVERNANCE_PROPOSALS.md](QLX_GOVERNANCE_PROPOSALS.md) | Governance proposal lifecycle, status transitions, and operator workflows. |
| [QLX_MULTISIG_CONFIG.md](QLX_MULTISIG_CONFIG.md) | On-chain multisig contract setup, signer rotation, and threshold-signature verification. |
| [QLX_MULTISIG_ROTATION.md](QLX_MULTISIG_ROTATION.md) | Admin multisig signer rotation procedure with timelock protection. |
| [contracts/timelocked-operations.md](contracts/timelocked-operations.md) | Timelocked and queued sensitive operations lifecycle. |

## Security, Auditing & Formal Invariants

| Document | What it covers |
|----------|----------------|
| [contracts/security.md](contracts/security.md) | Reentrancy guard, pause circuit breaker, and access control protections. |
| [security/kyc-keys.md](security/kyc-keys.md) | KYC encryption key hierarchy for protecting sensitive PII payloads. |
| [security/settlement-notes.md](security/settlement-notes.md) | Settlement security notes covering overpay attacks and edge-case mitigations. |
| [AUDIT_PREP.md](AUDIT_PREP.md) | Security audit preparation checklist for contributors preparing contracts for external review. |
| [contracts/audit.md](contracts/audit.md) | Immutable on-chain `AuditLogEntry` audit trail and hash chain. |
| [contracts/audit-trail.md](contracts/audit-trail.md) | On-chain audit trail integrity guarantees for critical state changes. |
| [OFF_CHAIN_SIGNATURES.md](OFF_CHAIN_SIGNATURES.md) | Threat model and implementation notes for off-chain signed KYC payloads, cursors, and dispute evidence. |
| [contracts/arithmetic-safety.md](contracts/arithmetic-safety.md) | Checked arithmetic and overflow/underflow safety standards for real-money Soroban paths. |
| [no-panic-policy.md](no-panic-policy.md) | Policy prohibiting `unwrap()`, `expect()`, and `panic!()` in Soroban contract code. |
| [supply-chain-policy.md](supply-chain-policy.md) | Supply-chain security policy for Rust crate dependencies and auditing. |
| [reproducible-builds.md](reproducible-builds.md) | Reproducible WASM build procedure for Stellar mainnet audit verification. |
| [invariants.md](invariants.md) | Core smart-contract invariants for contributors and reviewers. |
| [solvency-invariant.md](solvency-invariant.md) | Protocol solvency invariant specification and accounting guarantees. |
| [invariant-self-check.md](invariant-self-check.md) | Read-only protocol heartbeat entrypoint aggregating cross-module invariant checks. |
| [FORMAL_MODEL_NOTES.md](FORMAL_MODEL_NOTES.md) | Formal verification state-model notes for protocol state transitions. |
| [freshness-policy.md](freshness-policy.md) | External oracle feed freshness and drift policy for financial invoice evaluation. |

## Operations, Runbooks, Upgrades & Recovery

| Document | What it covers |
|----------|----------------|
| [OPERATOR_HANDBOOK.md](OPERATOR_HANDBOOK.md) | Every admin/operator entrypoint with concrete CLI examples for init, pause, fees, KYC, backup, and upgrades. |
| [contracts/operations.md](contracts/operations.md) | Summary of operational controls available to protocol admins. |
| [OPERATIONAL_PLAYBOOKS.md](OPERATIONAL_PLAYBOOKS.md) | Protocol reason codes, meanings, and concrete operational playbooks. |
| [RUNBOOK_INCIDENT_RESPONSE.md](RUNBOOK_INCIDENT_RESPONSE.md) | Step-by-step operator playbook for responding to unexpected contract behavior. |
| [EMERGENCY_RESPONSE.md](EMERGENCY_RESPONSE.md) | Canonical contract emergency response runbook on Stellar Soroban. |
| [QLX_INVESTIGATION_WORKFLOW.md](QLX_INVESTIGATION_WORKFLOW.md) | End-to-end investigation workflow with SLAs for unexpected protocol behavior. |
| [QLX_FREEZE_RECOVERY.md](QLX_FREEZE_RECOVERY.md) | Operator playbook for applying, monitoring, and lifting invoice freezes. |
| [PAUSE_TAXONOMY.md](PAUSE_TAXONOMY.md) | Pause taxonomy, circuit-breaker modes, and operational guidance. |
| [contracts/emergency.md](contracts/emergency.md) | Independent emergency mode and pause circuit-breaker behavior. |
| [contracts/emergency-recovery.md](contracts/emergency-recovery.md) | Admin-only, timelocked emergency withdraw mechanism for recovering stuck tokens. |
| [ADMIN_RECOVERY_POLICY.md](ADMIN_RECOVERY_POLICY.md) | Policy governing authorized admin dry-runs and privileged recovery mutations. |
| [admin-dry-run.md](admin-dry-run.md) | Admin dry-run preview capability for simulating state changes before mainnet signing. |
| [UPGRADE_PATHS.md](UPGRADE_PATHS.md) | Operator guide for shipping and executing contract WASM upgrades. |
| [UPGRADE_QUIESCE.md](UPGRADE_QUIESCE.md) | Write-drain quiescence policy, maintenance mode, and checklist prior to contract upgrades. |
| [FEE_RECIPIENT_ROTATION.md](FEE_RECIPIENT_ROTATION.md) | Operator guide for safely rotating the treasury/fee recipient address. |
| [QLX_TREASURY_ROTATION.md](QLX_TREASURY_ROTATION.md) | Two-step validated, timelocked treasury address rotation flow. |
| [contracts/backup.md](contracts/backup.md) | Admin-controlled invoice state snapshot backup and restore module. |
| [BACKUP_RETENTION.md](BACKUP_RETENTION.md) | Operator backup retention policy for Soroban deployments. |
| [backup-format.md](backup-format.md) | Backup format versioning and schema compatibility guarantees. |
| [BATCH_OPS.md](BATCH_OPS.md) | Execution semantics and atomicity guarantees for batch operations. |
| [MONITORING.md](MONITORING.md) | Operator monitoring guide with per-event alert thresholds. |
| [EVENT_DASHBOARDS.md](EVENT_DASHBOARDS.md) | Standard Grafana dashboards, PromQL queries, SQLite queries, and alert rules. |
| [DASHBOARD_QUERIES.md](DASHBOARD_QUERIES.md) | SQL reference for indexer health and operational workload queries. |

## Indexing, Events, Queries & Storage

| Document | What it covers |
|----------|----------------|
| [INDEXING_CONTRACT.md](INDEXING_CONTRACT.md) | Contract-level schemas, event emission formats, and storage layouts for off-chain indexers. |
| [QLX_INDEXER_CONTRACT.md](QLX_INDEXER_CONTRACT.md) | Summary of on-chain contract events and structures relied upon by the off-chain indexer. |
| [contracts/events.md](contracts/events.md) | Canonical event schema stability contract and topic constants in `events.rs`. |
| [contracts/events_complete.md](contracts/events_complete.md) | Complete reference for all structured contract events emitted for indexing and monitoring. |
| [events_complete.md](events_complete.md) | Top-level complete event schema reference for every contract-emitted event. |
| [EVENTS_SCHEMA.md](EVENTS_SCHEMA.md) | Structured Soroban contract events schema specification. |
| [events.md](events.md) | Event ingestion API specification for the `POST /api/v1/events` endpoint. |
| [ON_CHAIN_LOGS.md](ON_CHAIN_LOGS.md) | Comparison of Stellar transaction-level vs contract-level event surfaces. |
| [pause-events.md](pause-events.md) | Event emission behavior when `require_unpaused` short-circuits paused calls. |
| [QUERIES.md](QUERIES.md) | Read-only query entrypoint catalog for developers and integrators. |
| [QUERY_SEMANTICS.md](QUERY_SEMANTICS.md) | Query semantics and paged read guarantees for off-chain clients and indexers. |
| [contracts/queries.md](contracts/queries.md) | Query hard caps and resilience guarantees for missing/corrupted records. |
| [CURSOR_GUARANTEES.md](CURSOR_GUARANTEES.md) | Cursor stability guarantees and snapshot generation semantics for paginated reads. |
| [READ_MODEL_CONSISTENCY.md](READ_MODEL_CONSISTENCY.md) | Read model consistency expectations between on-chain state and off-chain projections. |
| [data-freshness-semantics.md](data-freshness-semantics.md) | Data freshness semantics for indexed views and API responses. |
| [CROSS_INVOICE_ANALYTICS.md](CROSS_INVOICE_ANALYTICS.md) | Cross-invoice read patterns, supported entrypoints, and pagination bounds. |
| [contracts/analytics.md](contracts/analytics.md) | On-chain analytics metrics calculation, storage, and report generation. |
| [contracts/analytics-usage.md](contracts/analytics-usage.md) | Analytics module features and usage guide for platform admins. |
| [QLX_REPORT_LIFECYCLE.md](QLX_REPORT_LIFECYCLE.md) | Analytics report lifecycle (`Requested` → `Delivered` → `Archived`), entrypoints, and invariants. |
| [contracts/storage-schema.md](contracts/storage-schema.md) | On-chain persistent storage keys, data model, and index layout. |
| [contracts/storage.md](contracts/storage.md) | Storage layout, indexing strategy, and integrity audit mechanisms. |
| [contracts/invoice-storage.md](contracts/invoice-storage.md) | Invoice storage, indexing, and retrieval architecture. |
| [STORAGE_LAYOUT.md](STORAGE_LAYOUT.md) | Architectural decisions behind Soroban storage layout choices. |
| [STORAGE_TTL.md](STORAGE_TTL.md) | Soroban storage TTL mapping and extension policy. |
| [persistence.md](persistence.md) | Persistent storage structures for the QuickLendX backend. |
| [backend-webhook-cursor-cache.md](backend-webhook-cursor-cache.md) | Backend webhook versioning, chain cursor tracking, and caching design. |
| [cadence-retention-consumers.md](cadence-retention-consumers.md) | Ingestion polling cadences, data retention policies, and downstream consumers. |
| [reconciliation.md](reconciliation.md) | Backend reconciliation cycle for detecting drift between indexed DB and on-chain state. |
| [backfill.md](backfill.md) | Resumable drift-backfill job and progress tracking for syncing missed on-chain state. |

## Testing, Fuzzing & CI Verification

| Document | What it covers |
|----------|----------------|
| [test-determinism.md](test-determinism.md) | Unified seed management system for reproducible fuzz testing across harnesses. |
| [accept-bid-fuzz.md](accept-bid-fuzz.md) | Property-based fuzz harness for the `accept_bid_and_fund` entrypoint. |
| [accept-bid-race.md](accept-bid-race.md) | Concurrent-acceptance race regression tests for `accept_bid_and_fund`. |
| [currency-whitelist-fuzz.md](currency-whitelist-fuzz.md) | Property-based fuzz harness for admin-managed currency whitelist churn. |
| [invoice-metadata-fuzz.md](invoice-metadata-fuzz.md) | Property-based fuzz harness for invoice metadata and tag bounds. |
| [types-fuzz.md](types-fuzz.md) | `cargo-fuzz` harness testing XDR round-trip stability of `#[contracttype]` structs. |
| [dispute-timeline-invariants.md](dispute-timeline-invariants.md) | Executable property-test reference for dispute timeline invariants. |
| [profit-fee-differential.md](profit-fee-differential.md) | Differential testing harness ensuring profit and fee math changes are explicitly blessed. |
| [reentrancy-fault-injection.md](reentrancy-fault-injection.md) | Hostile-token callback fault-injection test suite for reentrancy protection. |
| [invariant-checks.md](invariant-checks.md) | Backend indexed-DB invariant check suite for detecting data integrity violations. |
| [gas-baselines.md](gas-baselines.md) | Per-entrypoint Soroban gas and CPU instruction regression baselines. |
| [coverage-drift.md](coverage-drift.md) | CI coverage drift report comparing PR code coverage against `main`. |

## Backend Reference

| Document | What it covers |
|----------|----------------|
| [backend/docs/contributor-guide.md](../backend/docs/contributor-guide.md) | **Start here if you are new to the backend.** Module layout, request pipeline, and adding endpoints. |
| [backend/docs/configuration.md](../backend/docs/configuration.md) | Backend environment variables and defaults. |
| [backend/docs/exports.md](../backend/docs/exports.md) | Authenticated export generation, download links, and implementation. |
| [backend/docs/observability.md](../backend/docs/observability.md) | Ingest-lag monitoring, alert transitions, and degraded-mode behavior. |
| [backend/docs/secrets.md](../backend/docs/secrets.md) | Local environment and committed-secret scanning. |
| [backend/docs/runbooks.md](../backend/docs/runbooks.md) | Scheduler operation and lease behavior. |

## UX / Frontend Reference

| Document | What it covers |
|----------|----------------|
| [ux/visual-direction.md](ux/visual-direction.md) | Visual philosophy, language direction, and UX principles for QuickLendX. |
| [ux/design-tokens.md](ux/design-tokens.md) | Core design tokens and visual semantics across the interface. |
| [ux/review-notes.md](ux/review-notes.md) | Rationale behind design token selections and visual direction decisions. |
| [ux/content-style-guide.md](ux/content-style-guide.md) | Unified voice, terminology, and microcopy conventions. |
| [ux/emojis.md](ux/emojis.md) | Guidelines on where emojis are allowed in UI copy. |
| [ux/risk-copy-guidelines.md](ux/risk-copy-guidelines.md) | Copywriting rules for financial disclosures, fees, risks, and returns. |
| [ux/trust-safety-pattern-library.md](ux/trust-safety-pattern-library.md) | Reusable UX patterns and copy rules for irreversible actions and safety prompts. |
| [ux/buttons.md](ux/buttons.md) | Canonical button system specification (variants, sizes, and states). |
| [ux/modals.md](ux/modals.md) | Modal and dialog component system specification. |
| [ux/toasts.md](ux/toasts.md) | Toast notification visual and screen-reader accessibility specification. |
| [ux/tooltips.md](ux/tooltips.md) | Tooltip and inline-help component behavior, content pattern, and ARIA spec. |
| [ux/skeletons.md](ux/skeletons.md) | Skeleton loader library UI/UX specification. |
| [ux/metric-card.md](ux/metric-card.md) | Metric card component specification. |
| [ux/invoice-card.md](ux/invoice-card.md) | `InvoiceCard` component specification for Marketplace and Dashboard views. |
| [ux/collateral-indicator.md](ux/collateral-indicator.md) | Collateralization ratio indicator UX specification. |
| [ux/explorer-link.md](ux/explorer-link.md) | Block explorer link (Stellar Expert / Horizon) UI/UX pattern. |
| [ux/wallet-connect.md](ux/wallet-connect.md) | Wallet connect modal and connection-state UX specification. |
| [ux/landing-page.md](ux/landing-page.md) | Marketing and entry landing page redesign specification. |
| [ux/business-dashboard.md](ux/business-dashboard.md) | Business dashboard UX specification. |
| [ux/investor-journey.md](ux/investor-journey.md) | Investor journey map across onboarding, bidding, and settlement. |
| [ux/kyc-document-upload.md](ux/kyc-document-upload.md) | KYC document upload drag-and-drop UI/UX specification. |
| [ux/defaults-spec.md](ux/defaults-spec.md) | Grace period and default status communication UX specification. |
| [ux/dispute-response-composer.md](ux/dispute-response-composer.md) | Dispute response composer UI/UX specification for businesses. |
| [ux/settlement-receipt.md](ux/settlement-receipt.md) | Settlement receipt detail layout and download UX specification. |
| [ux/future-implementation-notes.md](ux/future-implementation-notes.md) | Future frontend implementation notes for KYC onboarding (`app/onboarding/kyc/page.tsx`). |

## Design Specs & Historical / Superseded Docs

| Document | What it covers |
|----------|----------------|
| [superpowers/specs/2026-08-30-repayment-events-audit-parity-design.md](superpowers/specs/2026-08-30-repayment-events-audit-parity-design.md) | Design spec for repayment events and audit parity (2026-08-30). |
| [pr-description-2185.md](pr-description-2185.md) | *(Historical / Superseded)* PR #2185 description introducing `StorageReadCache`. |
| [id-stability.md](id-stability.md) | *(Empty stub / Superseded)* Placeholder file; see `contracts/invoice.md` for invoice ID stability. |
