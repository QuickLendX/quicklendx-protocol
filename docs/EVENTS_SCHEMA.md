# QuickLendX — Canonical Event Reference

> **Single source of truth.** This document is the canonical, consolidated
> reference for every QuickLendX event: topic strings, payload fields, ingestion
> API contracts, and operator dashboards. The older event documents
> (`events.md`, `events_complete.md`, `contracts/events.md`,
> `contracts/events_complete.md`, `pause-events.md`, and
> `EVENT_DASHBOARDS.md`) are now one-line pointers to this file.
>
> **Authority:** payloads and topic strings below were verified against
> [`quicklendx-contracts/src/events.rs`](../quicklendx-contracts/src/events.rs).
> `PauseBlocked` is defined in the root [`src/events.rs`](../src/events.rs) via
> the `EventEmitter` trait. Legacy short symbols such as `inv_up` come from
> [`quicklendx-contracts/src/audit.rs`](../quicklendx-contracts/src/audit.rs)
> and are a **separate audit-log stream**, not contract-event topics.

## Contents

- [Overview](#overview)
- [Stability policy](#stability-policy)
- [Field type reference](#field-type-reference)
- [Complete event index](#complete-event-index)
- [Payload catalog](#payload-catalog)
- [Semantic aliases](#semantic-aliases)
- [Pause-blocked events](#pause-blocked-events)
- [Ingestion API (`POST /api/v1/events`)](#ingestion-api-post-apiv1events)
- [Operator dashboards and alerts](#operator-dashboards-and-alerts)
- [Privacy and security notes](#privacy-and-security-notes)
- [Source reference and verification](#source-reference-and-verification)
- [Related documentation](#related-documentation)

---

## Overview

The contract publishes events through the Soroban
`env.events().publish(topics, data)` API. Most events use the
`#[contractevent]` macro, which publishes data as an `Map<Symbol, Val>` keyed by
the Rust field names and uses the snake_case struct name as `topic_0`. A few
administrative events publish raw tuples with a `symbol_short!` topic.

| Element | Shape | Purpose |
|---|---|---|
| `topics` | `Vec<Val>` | `[Symbol(topic_0), …]`; `topic_0` is the stable event identifier used for filtering |
| `data` | `Val` | `Map<Symbol, Val>` for structured events, or a positional tuple for raw publishers |

Events are available on-chain through `getEvents` RPC and off-chain through
Horizon transaction-result meta. Backend delivery wraps them in the ingestion
envelope documented [below](#ingestion-api-post-apiv1events).

### Subscribe example (JavaScript/TypeScript)

```typescript
import { SorobanRpc } from "@stellar/stellar-sdk";

const server = new SorobanRpc.Server("https://soroban-testnet.stellar.org");

const events = await server.getEvents({
  startLedger: 1000000,
  filters: [
    {
      type: "contract",
      contractIds: ["<CONTRACT_ID>"],
      topics: [["invoice_uploaded"]], // topic_0 filter
    },
  ],
});

for (const event of events.events) {
  console.log(event.topic, event.value);
}
```

> Every string in the index below is `topic_0`. Constants prefixed `TOPIC_`
> are compile-time pinned in `events.rs`; renaming one is a breaking change.

---

## Stability policy

> **Topics are frozen.** Once deployed on a network, no existing topic string
> will be renamed or removed. New events may be added in future contract
> upgrades. Payload fields are **append-only**: existing positions are frozen
> and new fields are appended at the end.

- Filter by topic, never by position in the transaction's event array.
- Deserialize structured payloads by field name; positional decoding is safe
  only for documented raw-tuple events.
- Missing a newly appended field must be tolerated by older consumers.
- An unrecognised `MigrationStarted` without a matching completion or rollback
  means a migration is still in progress; writes to migrated entities may be
  rejected until it finishes.
- Read-only entrypoints emit no events.

### Indexer upgrade checklist

1. Check the changelog for newly pinned topics; add them to the index below.
2. Re-verify field order for raw-tuple events after each upgrade.
3. Keep replay cursors per contract and ledger; never assume event ordering.
4. Treat appended fields and additive events (for example
   `repayment_allocated`) as optional for old indexer versions.
5. Topics are permanent once deployed — dropped events do not occur.

---

## Field type reference

| Schema type | Soroban / XDR type | Notes |
|---|---|---|
| `bytes32` | `BytesN<32>` | Hex-encoded 32-byte identifier |
| `address` | `Address` | Stellar account (`G…`) or contract (`C…`) address |
| `i128` | `i128` | Signed 128-bit integer, smallest currency unit |
| `u32` | `u32` | Unsigned 32-bit integer |
| `u64` | `u64` | Unix timestamp (seconds) or duration |
| `string` | `soroban_sdk::String` | UTF-8, bounded by protocol limits |
| `bool` | `bool` | `true` / `false` |
| `enum` | `#[contracttype]` enum | `FeeType`, `InvoiceCategory`, `InvoiceStatus`, `TransitionPhase`, `OpType` |
| `addresses` | `Vec<Address>` | Ordered list of contract addresses |

---

## Complete event index

All 84 public emitter functions in `quicklendx-contracts/src/events.rs` are
listed here, plus five administrative publishers whose functions do not use the
standard public-emitter prefix. Topics in this table are the observed
`topic_0` values.

| Event | Topic (`topic_0`) | Emitter | Payload fields (in order) |
|---|---|---|---|
| **Invoice events** | | | |
| InvoiceUploaded | `invoice_uploaded` | `emit_invoice_uploaded` | invoice_id, business, amount, currency, due_date, timestamp |
| InvoiceVerified | `invoice_verified` | `emit_invoice_verified` | invoice_id, business, timestamp |
| InvoiceCancelled | `invoice_cancelled` | `emit_invoice_cancelled` | invoice_id, business, timestamp |
| InvoiceSettled | `invoice_settled` | `emit_invoice_settled` | invoice_id, amount, ledger, business, investor, investor_return, platform_fee, timestamp |
| InvoiceSettledFinal | `invoice_settled_final` | `emit_invoice_settled_final` | invoice_id, business, investor, total_paid, timestamp |
| InvoiceDefaulted | `invoice_defaulted` | `emit_invoice_defaulted` | invoice_id, business, investor, timestamp |
| InvoiceExpired | `invoice_expired` | `emit_invoice_expired` | invoice_id, business, due_date |
| InvoiceFunded | `invoice_funded` | `emit_invoice_funded` | invoice_id, investor, amount, timestamp |
| InvoiceFrozen | `invoice_frozen` | `emit_invoice_frozen` | invoice_id, frozen_by, reason, freeze_appeal_channel, timestamp |
| PartialPayment | `partial_payment` | `emit_partial_payment` | invoice_id, business, payment_amount, total_paid, progress, transaction_id |
| PaymentRecorded | `payment_recorded` | `emit_payment_recorded` | invoice_id, payer, amount, transaction_id, timestamp |
| RepaymentAllocated | `repayment_allocated` | `emit_repayment_allocated` | schema_version, operation_id, phase, invoice_id, payer, applied_*, cumulative_*, total_paid, total_due, fee_bps |
| InvoiceMetadataUpdated | `invoice_metadata_updated` | `emit_invoice_metadata_updated` | invoice_id, line_item_count, total_value, timestamp |
| InvoiceMetadataCleared | `invoice_metadata_cleared` | `emit_invoice_metadata_cleared` | invoice_id, business |
| InvoiceCategoryUpdated | `invoice_category_updated` | `emit_invoice_category_updated` | invoice_id, business, old_category, new_category |
| InvoiceTagAdded | `invoice_tag_added` | `emit_invoice_tag_added` | invoice_id, business, tag |
| InvoiceTagRemoved | `invoice_tag_removed` | `emit_invoice_tag_removed` | invoice_id, business, tag |
| InvoiceStatusUpdated | `invoice_status_updated` | `emit_invoice_status_updated` | invoice_id, status |
| **Bid events** | | | |
| BidPlaced | `bid_placed` | `emit_bid_placed` | bid_id, invoice_id, investor, bid_amount, expected_return, timestamp, expiration_timestamp |
| BidAccepted | `bid_accepted` | `emit_bid_accepted` | bid_id, invoice_id, investor, business, bid_amount, expected_return, timestamp |
| BidWithdrawn | `bid_withdrawn` | `emit_bid_withdrawn` | bid_id, invoice_id, investor, bid_amount, timestamp |
| BidCancelled | `bid_cancelled` | `emit_bid_cancelled` | bid_id, invoice_id, investor, bid_amount, timestamp |
| BidExpired | `bid_expired` | `emit_bid_expired` | bid_id, invoice_id, investor, bid_amount, expiration_timestamp |
| BidTtlUpdated | `bid_ttl_updated` | `emit_bid_ttl_updated` | old_days, new_days, admin, timestamp |
| BidExpiryGraceUpdated | `bid_expiry_grace_updated` | `emit_bid_expiry_grace_updated` | old_seconds, new_seconds, admin, timestamp |
| **Escrow and investment events** | | | |
| EscrowCreated | `escrow_created` | `emit_escrow_created` | escrow_id, invoice_id, investor, business, amount |
| EscrowReleased | `escrow_released` | `emit_escrow_released` | escrow_id, invoice_id, business, amount |
| EscrowRefunded | `escrow_refunded` | `emit_escrow_refunded` | escrow_id, invoice_id, investor, amount |
| InvestmentWithdrawn | `investment_withdrawn` | `emit_investment_withdrawn` | investment_id, invoice_id, investor, amount |
| **KYC and participant-identity events** | | | |
| InvestorVerified | `investor_verified` | `emit_investor_verified` | investor, investment_limit, verified_at, verified_by, tier, risk_level, risk_score |
| KycSubmitted | `kyc_submitted` | `emit_kyc_submitted` | subject, is_resubmit, timestamp |
| KycVerified | `kyc_verified` | `emit_kyc_verified` | subject, admin, timestamp |
| KycRejected | `kyc_rejected` | `emit_kyc_rejected` | subject, admin, reason, timestamp |
| KycRevoked | `kyc_revoked` | `emit_kyc_revoked` | investor, admin, reason, timestamp |
| InvestorFrozen | `investor_frozen` | `emit_investor_frozen` | investor, admin, reason, timestamp |
| InvestorUnfrozen | `investor_unfrozen` | `emit_investor_unfrozen` | investor, admin, timestamp |
| **Dispute events** | | | |
| DisputeCreated | `dispute_created` | `emit_dispute_created` | invoice_id, created_by, reason, timestamp |
| DisputeUnderReview | `dispute_under_review` | `emit_dispute_under_review` | invoice_id, reviewed_by, timestamp |
| DisputeResolved | `dispute_resolved` | `emit_dispute_resolved` | invoice_id, resolved_by, resolution, timestamp |
| DisputeRejected | `dispute_rejected` | `emit_dispute_rejected` | invoice_id, rejected_by, reason, timestamp |
| **Fee, treasury, and revenue events** | | | |
| PlatformFeeUpdated | `platform_fee_updated` | `emit_platform_fee_updated` | fee_bps, updated_at, updated_by |
| FeeStructureUpdated | `fee_structure_updated` | `emit_fee_structure_updated` | fee_type, old_fee_bps, new_fee_bps, updated_by, timestamp |
| PlatformFeeRouted | `platform_fee_routed` | `emit_platform_fee_routed` | invoice_id, recipient, fee_amount, timestamp |
| PlatformFeeConfigUpdated | `platform_fee_config_updated` | `emit_platform_fee_config_updated` | old_fee_bps, new_fee_bps, updated_by, timestamp |
| TreasuryConfigured | `treasury_configured` | `emit_treasury_configured` | treasury_address, configured_by, timestamp |
| TreasuryRotationInitiated | `tr_rot_i` | `emit_treasury_rotation_initiated` | topic_1 = admin; new_address, confirmation_deadline |
| TreasuryRotationConfirmed | `tr_rot_f` | `emit_treasury_rotation_confirmed` | topic_1 = admin; new_address, timestamp |
| TreasuryRotationCancelled | `tr_rot_cn` | `treasury_rotation_cancelled` | topic_1 = admin; empty data |
| ProfitFeeBreakdown | `profit_fee_breakdown` | `emit_profit_fee_breakdown` | invoice_id, investment_amount, payment_amount, gross_profit, platform_fee, investor_return, fee_bps_applied, timestamp |
| RevenueDistributed | `revenue_distributed` | `emit_revenue_distributed` | period, treasury_amount, developer_amount, platform_amount |
| **Insurance events** | | | |
| InsuranceAdded | `insurance_added` | `emit_insurance_added` | investment_id, invoice_id, investor, provider, coverage_percentage, coverage_amount, premium_amount |
| InsurancePremiumCollected | `insurance_premium_collected` | `emit_insurance_premium_collected` | investment_id, provider, premium_amount |
| InsuranceClaimed | `insurance_claimed` | `emit_insurance_claimed` | investment_id, invoice_id, provider, coverage_amount |
| **Backup, audit, and classification events** | | | |
| BackupCreated | `backup_created` | `emit_backup_created` | backup_id, invoice_count, timestamp |
| BackupRestored | `backup_restored` | `emit_backup_restored` | backup_id, invoice_count, timestamp |
| BackupValidated | `backup_validated` | `emit_backup_validated` | backup_id, success, timestamp |
| BackupArchived | `backup_archived` | `emit_backup_archived` | backup_id, timestamp |
| BackupsCleaned | `backups_cleaned` | `emit_backups_cleaned` | removed_count, timestamp |
| RetentionPolicyUpdated | `retention_policy_updated` | `emit_retention_policy_updated` | max_backups, max_age_seconds, auto_cleanup_enabled, timestamp |
| AuditValidation | `audit_validation` | `emit_audit_validation` | invoice_id, is_valid, timestamp |
| AuditQuery | `audit_query` | `emit_audit_query` | query_type, result_count |
| **Protocol lifecycle and administration** | | | |
| ProtocolInitialized | `protocol_initialized` | `emit_protocol_initialized` | admin, treasury, fee_bps, min_invoice_amount, max_due_date_days, grace_period_seconds, backfill_max_batch_size, corridors, timestamp |
| AdminInitialized | `adm_init` | `emit_admin_initialized` | admin |
| AdminSet | `admin_set` | `emit_admin_set` | admin, timestamp |
| AdminTransferred | `adm_trf` | `emit_admin_transferred` | old_admin, new_admin, timestamp |
| AdminTransferInitiated | `adm_req` | `emit_admin_transfer_initiated` | current_admin, pending_admin, timestamp |
| AdminTransferCancelled | `adm_cnl` | `emit_admin_transfer_cancelled` | current_admin, pending_admin, timestamp |
| AdminTwoStepUpdated | `adm_2st` | `emit_admin_two_step_updated` | admin, enabled, timestamp |
| EmergencyWithdrawalInitiated | `emergency_withdrawal_initiated` | `emit_emergency_withdrawal_initiated` | token, amount, target, unlock_at, admin |
| EmergencyWithdrawalExecuted | `emergency_withdrawal_executed` | `emit_emergency_withdrawal_executed` | token, amount, target, admin |
| EmergencyWithdrawalCancelled | `emergency_withdrawal_cancelled` | `emit_emergency_withdrawal_cancelled` | token, amount, target, admin |
| IncidentModeEntered | `incident_mode_entered` | `emit_incident_mode_entered` | admin, reason, timestamp |
| IncidentModeExited | `incident_mode_exited` | `emit_incident_mode_exited` | admin, timestamp |
| Paused | `paused` | `emit_paused` | admin |
| Unpaused | `unpaused` | `emit_unpaused` | admin |
| ScanItemFailed | `scan_item_failed` | `emit_scan_item_failed` | invoice_id, error_code |
| TtlExtended | `ttl_extended` | `emit_ttl_extended` | kind, count |
| ArbiterRegistered | `arb_reg` | `arbiter_registered` | admin, arbiter, timestamp |
| ArbiterRevoked | `arb_rvk` | `arbiter_revoked` | admin, arbiter, timestamp |
| BackfillStarted | `bkf_sta` | `backfill_started` | actor, timestamp |
| BackfillFinished | `bkf_end` | `backfill_finished` | actor, restored_count, timestamp |
| **Upgrades and schema migrations** | | | |
| UpgradeScheduled | `upg_sch` | `emit_upgrade_scheduled` | admin, wasm_hash, timestamp, executable_at |
| UpgradeCancelled | `upg_can` | `emit_upgrade_cancelled` | admin, wasm_hash |
| UpgradeExecuted | `upg_exe` | `emit_upgrade_executed` | admin, wasm_hash |
| SchemaVersionSet | `schema_version_set` | `emit_schema_version_set` | schema_version, set_by, timestamp |
| MigrationStarted | `migration_started` | `emit_migration_started` | schema_from, schema_to, initiated_by, timestamp |
| MigrationCompleted | `migration_completed` | `emit_migration_completed` | schema_from, schema_to, records_migrated, completed_by, timestamp |
| MigrationRolledBack | `migration_rolled_back` | `emit_migration_rolled_back` | schema_from, schema_to, rolled_back_by, timestamp |
| MigrationFailed | `migration_failed` | `emit_migration_failed` | schema_from, schema_to, records_migrated, next_offset, reason, timestamp |

---

## Payload catalog

Field lists below are taken directly from the `#[contractevent]` structs or the
raw `publish` calls in `quicklendx-contracts/src/events.rs`. See the index above
for the emitter function and `topic_0`.

### Invoice events

| Event | Topic | Payload (field: type, in order) | Notes |
|---|---|---|---|
| InvoiceUploaded | `invoice_uploaded` | `invoice_id: bytes32`, `business: address`, `amount: i128`, `currency: address`, `due_date: u64`, `timestamp: u64` | Semantic alias: `InvoiceCreated` |
| InvoiceVerified | `invoice_verified` | `invoice_id: bytes32`, `business: address`, `timestamp: u64` | Admin approval for bidding |
| InvoiceCancelled | `invoice_cancelled` | `invoice_id: bytes32`, `business: address`, `timestamp: u64` | Before funding |
| InvoiceSettled | `invoice_settled` | `invoice_id: bytes32`, `amount: i128`, `ledger: u32`, `business: address`, `investor: address`, `investor_return: i128`, `platform_fee: i128`, `timestamp: u64` | `amount` is `invoice.total_paid`; `ledger` is the sequence number. Semantic alias: `LoanSettled` |
| InvoiceSettledFinal | `invoice_settled_final` | `invoice_id: bytes32`, `business: address`, `investor: address`, `total_paid: i128`, `timestamp: u64` | Exactly once per settled invoice |
| InvoiceDefaulted | `invoice_defaulted` | `invoice_id: bytes32`, `business: address`, `investor: address`, `timestamp: u64` | After grace period |
| InvoiceExpired | `invoice_expired` | `invoice_id: bytes32`, `business: address`, `due_date: u64` | No timestamp field |
| InvoiceFunded | `invoice_funded` | `invoice_id: bytes32`, `investor: address`, `amount: i128`, `timestamp: u64` | Atomic with `bid_accepted` |
| InvoiceFrozen | `invoice_frozen` | `invoice_id: bytes32`, `frozen_by: address`, `reason: string`, `freeze_appeal_channel: string`, `timestamp: u64` | Additive appeal channel; see below |
| PartialPayment | `partial_payment` | `invoice_id: bytes32`, `business: address`, `payment_amount: i128`, `total_paid: i128`, `progress: u32`, `transaction_id: string` | `progress` is 0–100 percent (not basis points) |
| PaymentRecorded | `payment_recorded` | `invoice_id: bytes32`, `payer: address`, `amount: i128`, `transaction_id: string`, `timestamp: u64` | Durable payment record |
| RepaymentAllocated | `repayment_allocated` | `schema_version: u32`, `operation_id: bytes32`, `phase: enum` (`Committed`), `invoice_id: bytes32`, `payer: address`, `applied_principal: i128`, `applied_investor_profit: i128`, `applied_platform_fee: i128`, `applied_late_penalty: i128`, `cumulative_principal: i128`, `cumulative_investor_profit: i128`, `cumulative_platform_fee: i128`, `cumulative_late_penalty: i128`, `total_paid: i128`, `total_due: i128`, `fee_bps: i128` | Additive; emitted after storage commit. `operation_id` matches the `PaymentProcessed` audit entry; legacy `partial_payment`/`payment_recorded` are unchanged |
| InvoiceMetadataUpdated | `invoice_metadata_updated` | `invoice_id: bytes32`, `line_item_count: u32`, `total_value: i128`, `timestamp: u64` | Aggregates only — never customer name or tax ID |
| InvoiceMetadataCleared | `invoice_metadata_cleared` | `invoice_id: bytes32`, `business: address` | No timestamp field |
| InvoiceCategoryUpdated | `invoice_category_updated` | `invoice_id: bytes32`, `business: address`, `old_category: enum`, `new_category: enum` | `InvoiceCategory` variants from `types.rs` |
| InvoiceTagAdded / InvoiceTagRemoved | `invoice_tag_added` / `invoice_tag_removed` | `invoice_id: bytes32`, `business: address`, `tag: string` | Identical payload shape |
| InvoiceStatusUpdated | `invoice_status_updated` | `invoice_id: bytes32`, `status: enum` | `InvoiceStatus` from `types.rs` |

#### InvoiceUploaded example

```json
{
  "invoice_id":  "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "business":    "GBUSINESS1EXAMPLEADDRESSXXXXXXXXXXXXXXXXXXXXXXXXXXXXXV5Z",
  "amount":      50000000000,
  "currency":    "CDLZFC3SYJYDZT7K67VZ75HPJVOKJ4OQTF7S54XYDNKXL7SVN39DQNI",
  "due_date":    1751328000,
  "timestamp":   1748649600
}
```

#### InvoiceFrozen details

`reason` is a machine-readable label from `BusinessFreezeReason::label()`, never
a free-text admin comment:

| Variant | `reason` string |
|---|---|
| `AdminAction` | `admin_action` |
| `KYCRejected` | `kyc_rejected` |
| `ComplianceViolation` | `compliance_violation` |
| `SuspiciousActivity` | `suspicious_activity` |
| `LegalHold` | `legal_hold` |
| `FraudSuspected` | `fraud_suspected` |
| `Dispute` | `dispute` |
| `Voluntary` | `voluntary` |

`freeze_appeal_channel` is always `"docs/APPEALS.md"`; consumers should surface
it to the affected business. Older indexers may ignore this appended field.
See [`APPEALS.md`](APPEALS.md).

```json
{
  "invoice_id":            "a1b2c3d4...",
  "frozen_by":             "GC5DNYW4...",
  "reason":                "compliance_violation",
  "freeze_appeal_channel": "docs/APPEALS.md",
  "timestamp":             1720000000
}
```

### Bid events

| Event | Topic | Payload (field: type, in order) | Notes |
|---|---|---|---|
| BidPlaced | `bid_placed` | `bid_id: bytes32`, `invoice_id: bytes32`, `investor: address`, `bid_amount: i128`, `expected_return: i128`, `timestamp: u64`, `expiration_timestamp: u64` | `expected_return` is total expected repayment |
| BidAccepted | `bid_accepted` | `bid_id: bytes32`, `invoice_id: bytes32`, `investor: address`, `business: address`, `bid_amount: i128`, `expected_return: i128`, `timestamp: u64` | Triggers escrow creation |
| BidWithdrawn | `bid_withdrawn` | `bid_id: bytes32`, `invoice_id: bytes32`, `investor: address`, `bid_amount: i128`, `timestamp: u64` | Investor cancels an unaccepted bid |
| BidCancelled | `bid_cancelled` | `bid_id: bytes32`, `invoice_id: bytes32`, `investor: address`, `bid_amount: i128`, `timestamp: u64` | Distinct topic from `bid_withdrawn` |
| BidExpired | `bid_expired` | `bid_id: bytes32`, `invoice_id: bytes32`, `investor: address`, `bid_amount: i128`, `expiration_timestamp: u64` | No timestamp field |
| BidTtlUpdated | `bid_ttl_updated` | `old_days: u64`, `new_days: u64`, `admin: address`, `timestamp: u64` | Bid TTL window change |
| BidExpiryGraceUpdated | `bid_expiry_grace_updated` | `old_seconds: u64`, `new_seconds: u64`, `admin: address`, `timestamp: u64` | Expiry grace window change |

### Escrow and investment events

| Event | Topic | Payload (field: type, in order) | Notes |
|---|---|---|---|
| EscrowCreated | `escrow_created` | `escrow_id: bytes32`, `invoice_id: bytes32`, `investor: address`, `business: address`, `amount: i128` | Semantic alias: `FundsLocked`; no PII |
| EscrowReleased | `escrow_released` | `escrow_id: bytes32`, `invoice_id: bytes32`, `business: address`, `amount: i128` | No timestamp field |
| EscrowRefunded | `escrow_refunded` | `escrow_id: bytes32`, `invoice_id: bytes32`, `investor: address`, `amount: i128` | No timestamp field |
| InvestmentWithdrawn | `investment_withdrawn` | `investment_id: bytes32`, `invoice_id: bytes32`, `investor: address`, `amount: i128` | Before settlement |

### KYC and participant-identity events

| Event | Topic | Payload (field: type, in order) | Notes |
|---|---|---|---|
| InvestorVerified | `investor_verified` | `investor: address`, `investment_limit: i128`, `verified_at: u64`, `verified_by: address`, `tier: u32`, `risk_level: u32`, `risk_score: u32` | Tier: 0 Basic, 1 Silver, 2 Gold, 3 Platinum, 4 VIP. Risk level: 0 Low, 1 Medium, 2 High, 3 VeryHigh |
| KycSubmitted | `kyc_submitted` | `subject: address`, `is_resubmit: bool`, `timestamp: u64` | Business or investor |
| KycVerified | `kyc_verified` | `subject: address`, `admin: address`, `timestamp: u64` | Approval |
| KycRejected | `kyc_rejected` | `subject: address`, `admin: address`, `reason: string`, `timestamp: u64` | Auditable reason code |
| KycRevoked | `kyc_revoked` | `investor: address`, `admin: address`, `reason: string`, `timestamp: u64` | Previously verified investor |
| InvestorFrozen | `investor_frozen` | `investor: address`, `admin: address`, `reason: string`, `timestamp: u64` | Reason is the enum label (for example `AdminAction`) |
| InvestorUnfrozen | `investor_unfrozen` | `investor: address`, `admin: address`, `timestamp: u64` | Restores investment actions |

### Dispute events

| Event | Topic | Payload (field: type, in order) | Notes |
|---|---|---|---|
| DisputeCreated | `dispute_created` | `invoice_id: bytes32`, `created_by: address`, `reason: string`, `timestamp: u64` | Semantic alias: `DisputeOpened`; reason must not contain PII |
| DisputeUnderReview | `dispute_under_review` | `invoice_id: bytes32`, `reviewed_by: address`, `timestamp: u64` | Admin review queue |
| DisputeResolved | `dispute_resolved` | `invoice_id: bytes32`, `resolved_by: address`, `resolution: string`, `timestamp: u64` | Closes dispute |
| DisputeRejected | `dispute_rejected` | `invoice_id: bytes32`, `rejected_by: address`, `reason: string`, `timestamp: u64` | Dismissed dispute |

### Fee, treasury, and revenue events

| Event | Topic | Payload (field: type, in order) | Notes |
|---|---|---|---|
| PlatformFeeUpdated | `platform_fee_updated` | `fee_bps: u32`, `updated_at: u64`, `updated_by: address` | 1 bps = 0.01%; capped by `MAX_PLATFORM_FEE_BPS` |
| FeeStructureUpdated | `fee_structure_updated` | `fee_type: enum`, `old_fee_bps: u32`, `new_fee_bps: u32`, `updated_by: address`, `timestamp: u64` | `FeeType` from `fees.rs` |
| PlatformFeeRouted | `platform_fee_routed` | `invoice_id: bytes32`, `recipient: address`, `fee_amount: i128`, `timestamp: u64` | Forwarded to treasury |
| PlatformFeeConfigUpdated | `platform_fee_config_updated` | `old_fee_bps: u32`, `new_fee_bps: u32`, `updated_by: address`, `timestamp: u64` | Distinct from `platform_fee_updated` |
| TreasuryConfigured | `treasury_configured` | `treasury_address: address`, `configured_by: address`, `timestamp: u64` | Configuration audit |
| TreasuryRotationInitiated | `tr_rot_i` | topics: `tr_rot_i`, `admin`; data: `new_address: address`, `confirmation_deadline: u64` | Raw two-topic publish |
| TreasuryRotationConfirmed | `tr_rot_f` | topics: `tr_rot_f`, `admin`; data: `new_address: address`, `timestamp: u64` | Raw two-topic publish |
| TreasuryRotationCancelled | `tr_rot_cn` | topics: `tr_rot_cn`, `admin`; data: empty | Raw publisher name has no `emit` prefix |
| ProfitFeeBreakdown | `profit_fee_breakdown` | `invoice_id: bytes32`, `investment_amount: i128`, `payment_amount: i128`, `gross_profit: i128`, `platform_fee: i128`, `investor_return: i128`, `fee_bps_applied: i128`, `timestamp: u64` | `investor_return + platform_fee == payment_amount` |
| RevenueDistributed | `revenue_distributed` | `period: u64`, `treasury_amount: i128`, `developer_amount: i128`, `platform_amount: i128` | Revenue split |

> The `TreasuryRotation*` structs defined in `events.rs` are not used by the
> raw two-topic publishers above; decode the actual published tuples.

### Insurance events

| Event | Topic | Payload (field: type, in order) | Notes |
|---|---|---|---|
| InsuranceAdded | `insurance_added` | `investment_id: bytes32`, `invoice_id: bytes32`, `investor: address`, `provider: address`, `coverage_percentage: u32`, `coverage_amount: i128`, `premium_amount: i128` | Coverage is 0–100 percent |
| InsurancePremiumCollected | `insurance_premium_collected` | `investment_id: bytes32`, `provider: address`, `premium_amount: i128` | No timestamp field |
| InsuranceClaimed | `insurance_claimed` | `investment_id: bytes32`, `invoice_id: bytes32`, `provider: address`, `coverage_amount: i128` | No timestamp field |

### Backup, audit, and classification events

| Event | Topic | Payload (field: type, in order) | Notes |
|---|---|---|---|
| BackupCreated | `backup_created` | `backup_id: bytes32`, `invoice_count: u32`, `timestamp: u64` | Backup frequency metric |
| BackupRestored | `backup_restored` | `backup_id: bytes32`, `invoice_count: u32`, `timestamp: u64` | Recovery audit |
| BackupValidated | `backup_validated` | `backup_id: bytes32`, `success: bool`, `timestamp: u64` | Alert on `success == false` |
| BackupArchived | `backup_archived` | `backup_id: bytes32`, `timestamp: u64` | Long-term retention |
| BackupsCleaned | `backups_cleaned` | `removed_count: u32`, `timestamp: u64` | Retention compliance |
| RetentionPolicyUpdated | `retention_policy_updated` | `max_backups: u32`, `max_age_seconds: u64`, `auto_cleanup_enabled: bool`, `timestamp: u64` | Configuration audit |
| AuditValidation | `audit_validation` | `invoice_id: bytes32`, `is_valid: bool`, `timestamp: u64` | Data-quality check |
| AuditQuery | `audit_query` | `query_type: enum` (`OpType`), `result_count: u32` | `OpType` is defined in `audit.rs` |

`OpType` variants: `InvoiceCreated`, `InvoiceUploaded`, `InvoiceVerified`,
`InvoiceFunded`, `InvoicePaid`, `InvoiceDefaulted`, `InvoiceStatusChanged`,
`InvoiceRated`, `BidPlaced`, `BidAccepted`, `BidWithdrawn`, `EscrowCreated`,
`EscrowReleased`, `EscrowRefunded`, `PaymentProcessed`, `SettlementCompleted`.

### Protocol lifecycle and administration

| Event | Topic | Payload (field: type, in order) | Notes |
|---|---|---|---|
| ProtocolInitialized | `protocol_initialized` | `admin: address`, `treasury: address`, `fee_bps: u32`, `min_invoice_amount: i128`, `max_due_date_days: u64`, `grace_period_seconds: u64`, `backfill_max_batch_size: u32`, `corridors: addresses`, `timestamp: u64` | One-time setup; structurally extended in current code |
| AdminInitialized | `adm_init` | `admin: address` | Raw single-topic publish |
| AdminSet | `admin_set` | `admin: address`, `timestamp: u64` | Structured event |
| AdminTransferred | `adm_trf` | `old_admin: address`, `new_admin: address`, `timestamp: u64` | Raw single-topic publish |
| AdminTransferInitiated | `adm_req` | `current_admin: address`, `pending_admin: address`, `timestamp: u64` | Two-step transfer request |
| AdminTransferCancelled | `adm_cnl` | `current_admin: address`, `pending_admin: address`, `timestamp: u64` | Two-step transfer cancelled |
| AdminTwoStepUpdated | `adm_2st` | `admin: address`, `enabled: bool`, `timestamp: u64` | Two-step toggle |
| EmergencyWithdrawalInitiated | `emergency_withdrawal_initiated` | `token: address`, `amount: i128`, `target: address`, `unlock_at: u64`, `admin: address` | Time-locked withdrawal |
| EmergencyWithdrawalExecuted | `emergency_withdrawal_executed` | `token: address`, `amount: i128`, `target: address`, `admin: address` | Completed withdrawal |
| EmergencyWithdrawalCancelled | `emergency_withdrawal_cancelled` | `token: address`, `amount: i128`, `target: address`, `admin: address` | Cancelled withdrawal |
| IncidentModeEntered | `incident_mode_entered` | `admin: address`, `reason: string`, `timestamp: u64` | Coordinated pause + maintenance |
| IncidentModeExited | `incident_mode_exited` | `admin: address`, `timestamp: u64` | Incident closed |
| Paused | `paused` | `admin: address` | Protocol pause |
| Unpaused | `unpaused` | `admin: address` | Protocol resumed |
| ScanItemFailed | `scan_item_failed` | `invoice_id: bytes32`, `error_code: u32` | Backfill/scan failure |
| TtlExtended | `ttl_extended` | `kind: string`, `count: u32` | Internal TTL keeper |
| ArbiterRegistered | `arb_reg` | `admin: address`, `arbiter: address`, `timestamp: u64` | Dispute arbiter registry |
| ArbiterRevoked | `arb_rvk` | `admin: address`, `arbiter: address`, `timestamp: u64` | Dispute arbiter registry |
| BackfillStarted | `bkf_sta` | `actor: address`, `timestamp: u64` | Blocks upgrade scheduling while active |
| BackfillFinished | `bkf_end` | `actor: address`, `restored_count: u32`, `timestamp: u64` | Clears backfill flag |

### Upgrade and schema migration events

| Event | Topic | Payload (field: type, in order) | Notes |
|---|---|---|---|
| UpgradeScheduled | `upg_sch` | `admin: address`, `wasm_hash: bytes32`, `timestamp: u64`, `executable_at: u64` | Two timestamps, in this order |
| UpgradeCancelled | `upg_can` | `admin: address`, `wasm_hash: bytes32` | No timestamp field |
| UpgradeExecuted | `upg_exe` | `admin: address`, `wasm_hash: bytes32` | Contract code replaced |
| SchemaVersionSet | `schema_version_set` | `schema_version: u32`, `set_by: address`, `timestamp: u64` | Additive; old indexers can ignore |
| MigrationStarted | `migration_started` | `schema_from: u32`, `schema_to: u32`, `initiated_by: address`, `timestamp: u64` | One migration in progress at a time |
| MigrationCompleted | `migration_completed` | `schema_from: u32`, `schema_to: u32`, `records_migrated: u32`, `completed_by: address`, `timestamp: u64` | Reconcile record counts |
| MigrationRolledBack | `migration_rolled_back` | `schema_from: u32`, `schema_to: u32`, `rolled_back_by: address`, `timestamp: u64` | No partial new-schema state |
| MigrationFailed | `migration_failed` | `schema_from: u32`, `schema_to: u32`, `records_migrated: u32`, `next_offset: u32`, `reason: string`, `timestamp: u64` | Resumable from `next_offset` |

---

## Semantic aliases

The following type aliases exist in `events.rs` for domain clarity. Indexers
must subscribe using the canonical topic.

| Alias | Canonical type | Canonical topic |
|---|---|---|
| `InvoiceCreated` | `InvoiceUploaded` | `invoice_uploaded` |
| `FundsLocked` | `EscrowCreated` | `escrow_created` |
| `LoanSettled` | `InvoiceSettled` | `invoice_settled` |
| `DisputeOpened` | `DisputeCreated` | `dispute_created` |

---

## Pause-blocked events

> **Modules:** root [`src/events.rs`](../src/events.rs),
> [`src/pause.rs`](../src/pause.rs) · **Tests:** `src/test_pause.rs` ·
> **Taxonomy:** [`PAUSE_TAXONOMY.md`](PAUSE_TAXONOMY.md)

When the protocol is paused, `require_unpaused` short-circuits every protected
entrypoint and returns `PauseError::ContractPaused`. Each rejected call also
publishes one `PauseBlocked` event so monitors can count rejected traffic,
attribute it to callers, and correlate it with ledger timestamps.

### Topic and payload

- **Topic:** `PauseBlocked` — exported as `TOPIC_PAUSE_BLOCKED` in the root
  `src/events.rs` (not one of the `quicklendx-contracts` topics above).
- **Shape:** two topics — `PauseBlocked` and the `entrypoint` symbol; data is
  `(caller, ledger_ts)`.

| Field | Type | Description |
|---|---|---|
| `entrypoint` | `string` | Stable symbol of the blocked entrypoint |
| `caller` | `u64` | Numeric ID of the calling account |
| `ledger_ts` | `u64` | Ledger timestamp in seconds since the epoch |

### Entrypoint symbols

| Constant | Value | Protected action |
|---|---|---|
| `EP_INVOICE_UPLOAD` | `invoice_upload` | Business uploads an invoice |
| `EP_BID_PLACEMENT` | `bid_placement` | Investor places a bid |
| `EP_SETTLEMENT_INITIATION` | `settlement_initiation` | Business initiates settlement |
| `EP_ESCROW_RELEASE` | `escrow_release` | Business triggers escrow release |
| `EP_INVESTMENT_ACTION` | `investment_action` | Investor takes an investment action |

All five are enumerated in `pause::ALL_ENTRYPOINTS`. Symbols are ASCII,
lowercase, and underscore-separated — safe as metric labels or map keys.

### Hot-path guarantee

`require_unpaused` returns `Ok(())` before touching the emitter while unpaused.
Production call sites use a zero-sized `NoopEmitter` whose single `EventEmitter`
trait method is `#[inline(always)]` with an empty body. Cost on the unpaused
path: one boolean comparison and a return.

### Indexer notes

1. Subscribe to the exact byte sequence `PauseBlocked`.
2. Field order is `entrypoint`, `caller`, `ledger_ts`; future fields are appended.
3. One event per blocked call — no deduplication or batching.
4. Absence of these events means either the protocol was live or no guarded
   entrypoint received traffic.
5. Use the event to drive rejected-traffic dashboards during an incident.

### Tests

```bash
cargo test test_pause
```

| Test group | Count | Verification |
|---|---:|---|
| Live-path, no emission | 5 | Each entrypoint returns `Ok(())`, sink empty |
| Blocked-path per entrypoint | 5 | Each entrypoint returns `ContractPaused`, fields correct |
| Field propagation | 2 | `caller` and `ledger_ts` pass through unmodified |
| Topic stability | 6 | Each symbol equals its literal; topic constant is `PauseBlocked` |
| Multiple blocked calls | 2 | Events accumulate; five entrypoints produce five events |
| Mixed live/blocked | 1 | Only blocked calls produce events |
| State immutability | 1 | `PauseState` unchanged after call |
| Entry-point completeness | 1 | `ALL_ENTRYPOINTS` contains every constant |
| Collector reset | 1 | Buffer clears on `clear` |
| **Total** | **24** | |

---

## Ingestion API (`POST /api/v1/events`)

The backend accepts a single event object or an array. Unknown or malformed
events are rejected with a structured 400 response; raw payload values are
never echoed in validation errors. For alert thresholds, see
[`MONITORING.md`](MONITORING.md).

### Envelope

```json
{
  "id":             "string (unique per event)",
  "ledger":         0,
  "txHash":         "string",
  "type":           "<EventType>",
  "payload":        {},
  "timestamp":      0,
  "complianceHold": false,
  "indexedAt":      "ISO 8601"
}
```

`type` selects the payload schema. The supported ingestion types are
`InvoiceSettled`, `PaymentRecorded`, `DisputeCreated`, and `DisputeResolved`;
their canonical on-chain payload fields are in the catalog above.

### InvoiceSettled payload example

```json
{
  "invoice_id":      "string",
  "amount":          "string",
  "ledger":          0,
  "business":        "string",
  "investor":        "string",
  "investor_return": "string",
  "platform_fee":    "string",
  "timestamp":       0
}
```

### Validation rules

- All fields are required unless marked optional.
- Unknown event types are rejected.
- Each event needs a stable unique `id`; replaying a known `id` is a no-op.
- Processed IDs are persisted in the raw event log backing store.
- Batches larger than 100 events are rejected before processing.
- Raw payloads are never echoed in error responses.

### Response shape

| Outcome | HTTP status | `status` field |
|---|---|---|
| Accepted and processed | 200 | `processed` |
| Known duplicate | 200 | `duplicate` |
| Validation failure | 400 | `rejected` |

Mixed batches return a per-event `results` array. If any item is rejected, the
HTTP status is 400 while valid non-duplicate items in the same batch may still
be processed.

### Example batch

```json
[
  { "...InvoiceSettled": {} },
  { "...PaymentRecorded": {} },
  { "...DisputeCreated": {} },
  { "...DisputeResolved": {} }
]
```

---

## Operator dashboards and alerts

> Audience: operators running the backend and indexer who build Grafana
> dashboards for protocol health, event throughput, and alert triage. See
> [`backend/docs/observability.md`](../backend/docs/observability.md) for scrape
> configuration and the full metric reference.

### Prerequisites

| Component | Required |
|---|---|
| Prometheus scraping `/v1/metrics` | Yes |
| Grafana connected to Prometheus | Yes |
| Backend API key for `/v1/metrics` Bearer auth | Yes |
| SQLite replica for indexer queries | Recommended |

```bash
curl -s -H "Authorization: Bearer $QLX_API_KEY" \
  http://localhost:3000/v1/metrics
```

### Metric reference

| Metric | Type | Measures |
|---|---|---|
| `qlx_ingest_lag_ledgers` | Gauge | Ledgers the indexer is behind the network tip |
| `qlx_webhook_queue_depth` | Gauge | Webhooks queued but not yet delivered |
| `qlx_webhook_overflow_total` | Counter | Cumulative webhook queue overflows since startup |
| `qlx_rpc_circuit_state` | Gauge | `0` closed, `1` open, `2` half-open |
| `qlx_invariant_violations_total` | Counter | Cumulative invariant violations |

### Dashboard 1 — Protocol Health Overview

`https://<grafana-host>/d/qlx-health/protocol-health-overview`

| Panel | PromQL | Visualization |
|---|---|---|
| Ingest lag | `qlx_ingest_lag_ledgers` | Stat: green < 20, yellow 20–100, red > 100 |
| Webhook queue depth | `qlx_webhook_queue_depth` | Stat: green < 100, yellow 100–1000, red > 1000 |
| Webhook overflow rate | `rate(qlx_webhook_overflow_total[5m])` | Time-series |
| RPC circuit breaker | `qlx_rpc_circuit_state` | State timeline: 0 green, 1 red, 2 yellow |
| Invariant violations (5 min) | `increase(qlx_invariant_violations_total[5m])` | Stat; any non-zero value is red |

```yaml
groups:
  - name: qlx-health
    rules:
      - alert: IngestLagHigh
        expr: qlx_ingest_lag_ledgers > 100
        for: 5m
        annotations:
          summary: "Indexer is {{ $value }} ledgers behind the tip"
          runbook: "https://github.com/QuickLendX/quicklendx-protocol/blob/main/docs/RUNBOOK_INCIDENT_RESPONSE.md"
      - alert: WebhookQueueBackpressure
        expr: qlx_webhook_queue_depth > 1000
        for: 2m
        annotations:
          summary: "Webhook queue depth is {{ $value }}"
      - alert: RPCCircuitOpen
        expr: qlx_rpc_circuit_state == 1
        for: 1m
        annotations:
          summary: "RPC circuit breaker is open"
      - alert: InvariantViolationDetected
        expr: increase(qlx_invariant_violations_total[5m]) > 0
        for: 1m
        annotations:
          summary: "{{ $value }} invariant violation(s) in the past 5 min — page on-call"
```

### Dashboard 2 — Event Throughput

`https://<grafana-host>/d/qlx-events/event-throughput`

These panels use the indexer SQLite database, not Prometheus:

```bash
cd backend
sqlite3 "${DATABASE_PATH:-.data/dev.db}"
```

```sql
.headers on
.mode column
.timeout 5000
```

```sql
-- events per minute (last hour)
SELECT strftime('%Y-%m-%dT%H:%M:00', indexed_at) AS minute,
       COUNT(*) AS event_count
FROM raw_events
WHERE indexed_at >= datetime('now', '-1 hour')
GROUP BY minute ORDER BY minute;

-- top event types (last 24 h)
SELECT type, COUNT(*) AS event_count
FROM raw_events
WHERE indexed_at >= datetime('now', '-24 hours')
GROUP BY type ORDER BY event_count DESC LIMIT 20;

-- newest indexed ledger and freshness checkpoint
SELECT (SELECT MAX(ledger) FROM raw_events) AS newest_indexed_ledger,
       (SELECT cursor FROM freshness_state WHERE id = 1) AS freshness_cursor,
       (SELECT CAST((julianday('now') - julianday(timestamp)) * 86400 AS INTEGER)
        FROM freshness_state WHERE id = 1) AS checkpoint_age_seconds;
```

`checkpoint_age_seconds` above 120 warrants investigation; an absent
`freshness_state` row means no checkpoint has been written since startup.

### Dashboard 3 — Dispute and Default Monitor

`https://<grafana-host>/d/qlx-risk/dispute-and-default-monitor`

```sql
-- open disputes
SELECT COUNT(*) AS open_disputes
FROM raw_events
WHERE type = 'DisputeCreated'
  AND id NOT IN (
    SELECT DISTINCT json_extract(payload, '$.invoice_id')
    FROM raw_events WHERE type = 'DisputeResolved'
  );

-- disputes created per day (7 days)
SELECT date(indexed_at) AS day, COUNT(*) AS disputes_created
FROM raw_events
WHERE type = 'DisputeCreated' AND indexed_at >= datetime('now', '-7 days')
GROUP BY day ORDER BY day;

-- defaults per day (30 days)
SELECT date(indexed_at) AS day, COUNT(*) AS defaults
FROM raw_events
WHERE type = 'InvoiceDefaulted' AND indexed_at >= datetime('now', '-30 days')
GROUP BY day ORDER BY day;

-- escrow refund spikes
SELECT strftime('%Y-%m-%dT%H:00:00', indexed_at) AS hour,
       COUNT(*) AS refund_count
FROM raw_events
WHERE type = 'EscrowRefunded' AND indexed_at >= datetime('now', '-24 hours')
GROUP BY hour ORDER BY hour;
```

Replace `json_extract` column references with the actual `raw_events` schema
names in your deployment. More than five refunds in one hour warrants
investigation.

### Dashboard 4 — Settlement Pipeline

`https://<grafana-host>/d/qlx-settlement/settlement-pipeline`

```sql
SELECT status, COUNT(*) AS count FROM settlements GROUP BY status ORDER BY count DESC;

SELECT id, invoice_id, amount, payer, recipient, indexed_at
FROM settlements
WHERE status IN ('Pending', 'Processing')
  AND indexed_at < datetime('now', '-10 minutes')
ORDER BY indexed_at LIMIT 50;

SELECT status, COUNT(*) AS bid_count FROM bids GROUP BY status ORDER BY bid_count DESC;

SELECT bid_id, invoice_id, investor,
       datetime(expiration_timestamp, 'unixepoch') AS expired_at
FROM bids
WHERE status = 'Placed' AND expiration_timestamp < unixepoch()
ORDER BY expiration_timestamp LIMIT 50;
```

### Alert routing

| Alert | Owner | First action |
|---|---|---|
| `IngestLagHigh` | Indexer on-call | Check `freshness_state`, review indexer RPC logs |
| `WebhookQueueBackpressure` | Backend on-call | Check consumer lag; scale webhook workers |
| `RPCCircuitOpen` | Backend on-call | Verify Soroban node reachability |
| `InvariantViolationDetected` | Security on-call | Page immediately; follow the incident runbook |
| `DisputeCreated` | Support team | Open dispute ticket, freeze funds if needed |
| `InvoiceDefaulted` | Support team | Notify investors, start off-chain recovery |

---

## Privacy and security notes

- **No PII is emitted.** Customer names, tax IDs, and KYC documents are never
  included. `invoice_metadata_updated` contains aggregate counts/totals only.
- `dispute_created.reason` is a short reason code; callers must not write PII
  into it. Maximum length is enforced by `MAX_DISPUTE_REASON_LENGTH`.
- `invoice_frozen.freeze_appeal_channel` is a static path, not user data.
- All `address` values are Stellar public keys or contract IDs — pseudonymous,
  not personally identifiable.
- Timestamps come from `env.ledger().timestamp()` except `invoice_settled.ledger`
  (sequence number) and events copied from stored bid data.
- State-mutating events are gated by `require_auth()` upstream; getters emit none.

---

## Source reference and verification

- Payload structs and topic constants:
  [`quicklendx-contracts/src/events.rs`](../quicklendx-contracts/src/events.rs).
- Pause events: root [`src/events.rs`](../src/events.rs) and
  [`src/pause.rs`](../src/pause.rs).
- Legacy audit symbols: [`quicklendx-contracts/src/audit.rs`](../quicklendx-contracts/src/audit.rs).
- Field-order tests: `quicklendx-contracts/src/test_events.rs`; pause tests:
  `src/test_pause.rs`.

The event index contains one row for each of the 84 public emitter functions
in `events.rs`; a grep-based check comparing the two counts must report `84`
on each side. The index also lists the five administrative publishers that do
not use that naming convention. Any change to a `TOPIC_` constant is a
breaking schema change and requires a version bump.

---

## Related documentation

| Document | What it covers |
|---|---|
| [`MONITORING.md`](MONITORING.md) | Per-event alert thresholds |
| [`ON_CHAIN_LOGS.md`](ON_CHAIN_LOGS.md) | Subscribing to on-chain events via Soroban RPC |
| [`DASHBOARD_QUERIES.md`](DASHBOARD_QUERIES.md) | Full SQL reference for indexer health |
| [`RUNBOOK_INCIDENT_RESPONSE.md`](RUNBOOK_INCIDENT_RESPONSE.md) | Operator incident playbook |
| [`backend/docs/observability.md`](../backend/docs/observability.md) | Backend metrics and degraded-mode behavior |
