# QuickLendX — Contract Events Schema

> **Audience:** Off-chain integrators (indexers, dashboards, webhook consumers)
> who need to subscribe to and decode events emitted by the QuickLendX Soroban
> contract.

---

## Overview

The QuickLendX contract emits structured events via the Soroban
`env.events().publish(topics, data)` API. Each event has:

| Field     | Type       | Description                                                  |
|-----------|------------|--------------------------------------------------------------|
| `topics`  | `Vec<Val>` | `[Symbol(topic_name), ...]` — used for filtering             |
| `data`    | `Val`      | The event payload, serialised as an XDR `ScVal` map/struct   |

Events are available on-chain via `getEvents` RPC and off-chain via
[Horizon](https://developers.stellar.org/api) transaction result meta.

---

## How to Subscribe (JavaScript/TypeScript)

```typescript
import { SorobanRpc } from "@stellar/stellar-sdk";

const server = new SorobanRpc.Server("https://soroban-testnet.stellar.org");

const events = await server.getEvents({
  startLedger: 1000000,
  filters: [
    {
      type: "contract",
      contractIds: ["<CONTRACT_ID>"],
      topics: [["invoice_uploaded"]],   // topic_0 filter
    },
  ],
});

for (const event of events.events) {
  console.log(event.topic, event.value);
}
```

> All topic strings below are the **first topic** (`topic_0`) of the event.
> They are pinned as compile-time constants in `src/events.rs` and must not
> be changed without a schema version bump.

---

## Event Index

| Event Name                  | Topic (`topic_0`)          | Emitter             |
|-----------------------------|----------------------------|---------------------|
| [InvoiceUploaded]           | `invoice_uploaded`         | `store_invoice`     |
| [InvoiceVerified]           | `invoice_verified`         | `verify_invoice`    |
| [InvoiceCancelled]          | `invoice_cancelled`        | `cancel_invoice`    |
| [InvoiceFunded]             | `invoice_funded`           | `accept_bid_and_fund` |
| [InvoiceSettled]            | `invoice_settled`          | `settle_invoice`    |
| [InvoiceSettledFinal]       | `invoice_settled_final`    | `settle_invoice`    |
| [InvoiceDefaulted]          | `invoice_defaulted`        | `mark_invoice_defaulted` |
| [InvoiceExpired]            | `invoice_expired`          | `prune_terminal_invoices` |
| [PartialPayment]            | `partial_payment`          | `record_partial_payment` |
| [PaymentRecorded]           | `payment_recorded`         | `record_payment`    |
| [BidPlaced]                 | `bid_placed`               | `place_bid`         |
| [BidAccepted]               | `bid_accepted`             | `accept_bid_and_fund` |
| [BidWithdrawn]              | `bid_withdrawn`            | `withdraw_bid`      |
| [BidExpired]                | `bid_expired`              | `cleanup_expired_bids` |
| [EscrowCreated]             | `escrow_created`           | `accept_bid_and_fund` |
| [EscrowReleased]            | `escrow_released`          | `release_escrow`    |
| [EscrowRefunded]            | `escrow_refunded`          | `refund_escrow`     |
| [InvestmentWithdrawn]       | `investment_withdrawn`     | `withdraw_investment` |
| [InvestorVerified]          | `investor_verified`        | `verify_investor`   |
| [DisputeCreated]            | `dispute_created`          | `open_dispute`      |
| [DisputeUnderReview]        | `dispute_under_review`     | `escalate_dispute`  |
| [DisputeResolved]           | `dispute_resolved`         | `resolve_dispute`   |
| [PlatformFeeUpdated]        | `platform_fee_updated`     | `update_platform_fee` |
| [FeeStructureUpdated]       | `fee_structure_updated`    | `update_fee_structure` |
| [PlatformFeeRouted]         | `platform_fee_routed`      | `route_platform_fee` |
| [ProfitFeeBreakdown]        | `profit_fee_breakdown`     | `settle_invoice`    |
| [InsuranceAdded]            | `insurance_added`          | `add_insurance`     |
| [InsurancePremiumCollected] | `insurance_premium_collected` | `collect_premium` |
| [InsuranceClaimed]          | `insurance_claimed`        | `claim_insurance`   |
| [InvoiceMetadataUpdated]    | `invoice_metadata_updated` | `update_invoice_metadata` |
| [InvoiceMetadataCleared]    | `invoice_metadata_cleared` | `clear_invoice_metadata` |
| [InvoiceCategoryUpdated]    | `invoice_category_updated` | `update_invoice_category` |
| [InvoiceTagAdded]           | `invoice_tag_added`        | `add_invoice_tag`   |
| [InvoiceTagRemoved]         | `invoice_tag_removed`      | `remove_invoice_tag` |
| [ProtocolInitialized]       | `protocol_initialized`     | `initialize`        |
| [AdminInitialized]          | `admin_initialized`        | `initialize_admin`  |
| [BackupCreated]             | `backup_created`           | `create_backup`     |
| [BackupRestored]            | `backup_restored`          | `restore_backup`    |
| [BackupValidated]           | `backup_validated`         | `validate_backup`   |
| [BackupArchived]            | `backup_archived`          | `archive_backup`    |
| [BackupsCleaned]            | `backups_cleaned`          | `clean_old_backups` |
| [RetentionPolicyUpdated]    | `retention_policy_updated` | `update_retention_policy` |
| [RevenueDistributed]        | `revenue_distributed`      | `distribute_revenue` |
| [TtlExtended]               | `ttl_extended`             | internal TTL keeper |
| [BidTtlUpdated]             | `bid_ttl_updated`          | `update_bid_ttl`    |

---

## Field Type Reference

| Schema Type   | Soroban / XDR type    | Notes                                   |
|---------------|-----------------------|-----------------------------------------|
| `bytes32`     | `BytesN<32>`          | Hex-encoded 32-byte identifier          |
| `address`     | `Address`             | Stellar account or contract address (G…/C…) |
| `i128`        | `i128`                | Signed 128-bit integer, smallest unit   |
| `u32`         | `u32`                 | Unsigned 32-bit integer                 |
| `u64`         | `u64`                 | Unix timestamp (seconds since epoch) or duration |
| `string`      | `soroban_sdk::String` | UTF-8, length-bounded by protocol limits |
| `bool`        | `bool`                | `true` / `false`                        |

---

## Invoice Events

### `InvoiceUploaded`

Emitted when a business uploads a new invoice.

**Topic:** `invoice_uploaded`  
**Semantic alias:** `InvoiceCreated`

```json
{
  "invoice_id":  "<bytes32>",
  "business":    "<address>",
  "amount":      "<i128>",
  "currency":    "<address>",
  "due_date":    "<u64>",
  "timestamp":   "<u64>"
}
```

**Example:**
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

---

### `InvoiceVerified`

Emitted when an admin approves an invoice for bidding.

**Topic:** `invoice_verified`

```json
{
  "invoice_id": "<bytes32>",
  "business":   "<address>",
  "timestamp":  "<u64>"
}
```

---

### `InvoiceCancelled`

Emitted when a business cancels an invoice (before funding).

**Topic:** `invoice_cancelled`

```json
{
  "invoice_id": "<bytes32>",
  "business":   "<address>",
  "timestamp":  "<u64>"
}
```

---

### `InvoiceFunded`

Emitted when an investor's bid is accepted and the invoice transitions to
`Funded` status.

**Topic:** `invoice_funded`

```json
{
  "invoice_id": "<bytes32>",
  "investor":   "<address>",
  "amount":     "<i128>",
  "timestamp":  "<u64>"
}
```

---

### `InvoiceSettled`

Emitted when an invoice is fully settled and proceeds are distributed.

**Topic:** `invoice_settled`  
**Semantic alias:** `LoanSettled`

```json
{
  "invoice_id":       "<bytes32>",
  "business":         "<address>",
  "investor":         "<address>",
  "investor_return":  "<i128>",
  "platform_fee":     "<i128>",
  "timestamp":        "<u64>"
}
```

> `investor_return + platform_fee` equals the total collected from the business.

---

### `InvoiceSettledFinal`

Emitted after all disbursements complete — final acknowledgement of settlement.

**Topic:** `invoice_settled_final`

```json
{
  "invoice_id": "<bytes32>",
  "business":   "<address>",
  "investor":   "<address>",
  "total_paid": "<i128>",
  "timestamp":  "<u64>"
}
```

---

### `InvoiceDefaulted`

Emitted when an invoice is marked as defaulted (business failed to repay).

**Topic:** `invoice_defaulted`

```json
{
  "invoice_id": "<bytes32>",
  "business":   "<address>",
  "investor":   "<address>",
  "timestamp":  "<u64>"
}
```

---

### `InvoiceExpired`

Emitted when an invoice passes its `due_date` without settlement or default.

**Topic:** `invoice_expired`

```json
{
  "invoice_id": "<bytes32>",
  "business":   "<address>",
  "due_date":   "<u64>"
}
```

---

### `PartialPayment`

Emitted on each instalment received towards an invoice.

**Topic:** `partial_payment`

```json
{
  "invoice_id":      "<bytes32>",
  "business":        "<address>",
  "payment_amount":  "<i128>",
  "total_paid":      "<i128>",
  "progress":        "<u32>",
  "transaction_id":  "<string>"
}
```

> `progress` is an integer percentage `0–100` indicating how much of the
> invoice face value has been paid.

---

### `PaymentRecorded`

Emitted when a payment record is durably stored on-chain.

**Topic:** `payment_recorded`

```json
{
  "invoice_id":     "<bytes32>",
  "payer":          "<address>",
  "amount":         "<i128>",
  "transaction_id": "<string>",
  "timestamp":      "<u64>"
}
```

---

## Bid Events

### `BidPlaced`

Emitted when an investor places a bid on a verified invoice.

**Topic:** `bid_placed`

```json
{
  "bid_id":                "<bytes32>",
  "invoice_id":            "<bytes32>",
  "investor":              "<address>",
  "bid_amount":            "<i128>",
  "expected_return":       "<i128>",
  "timestamp":             "<u64>",
  "expiration_timestamp":  "<u64>"
}
```

> `expected_return > bid_amount` always. The delta is the investor's fee.
> Bids expire after the TTL set by `update_bid_ttl`.

---

### `BidAccepted`

Emitted when the business accepts a bid (triggers escrow creation).

**Topic:** `bid_accepted`

```json
{
  "bid_id":          "<bytes32>",
  "invoice_id":      "<bytes32>",
  "investor":        "<address>",
  "business":        "<address>",
  "bid_amount":      "<i128>",
  "expected_return": "<i128>",
  "timestamp":       "<u64>"
}
```

---

### `BidWithdrawn`

Emitted when an investor cancels their unaccepted bid.

**Topic:** `bid_withdrawn`

```json
{
  "bid_id":      "<bytes32>",
  "invoice_id":  "<bytes32>",
  "investor":    "<address>",
  "bid_amount":  "<i128>",
  "timestamp":   "<u64>"
}
```

---

### `BidExpired`

Emitted when a bid's TTL elapses and it is cleaned up.

**Topic:** `bid_expired`

```json
{
  "bid_id":                "<bytes32>",
  "invoice_id":            "<bytes32>",
  "investor":              "<address>",
  "bid_amount":            "<i128>",
  "expiration_timestamp":  "<u64>"
}
```

---

## Escrow & Investment Events

### `EscrowCreated`

Emitted when investor funds are locked in escrow (atomically with `BidAccepted`).

**Topic:** `escrow_created`  
**Semantic alias:** `FundsLocked`

```json
{
  "escrow_id":  "<bytes32>",
  "invoice_id": "<bytes32>",
  "investor":   "<address>",
  "business":   "<address>",
  "amount":     "<i128>"
}
```

> No PII is included. Funds are locked atomically; no partial state is possible.

---

### `EscrowReleased`

Emitted when escrowed funds are released to the business after settlement.

**Topic:** `escrow_released`

```json
{
  "escrow_id":  "<bytes32>",
  "invoice_id": "<bytes32>",
  "business":   "<address>",
  "amount":     "<i128>"
}
```

---

### `EscrowRefunded`

Emitted when escrowed funds are returned to the investor (e.g. after
`withdraw_investment` or invoice cancellation).

**Topic:** `escrow_refunded`

```json
{
  "escrow_id":  "<bytes32>",
  "invoice_id": "<bytes32>",
  "investor":   "<address>",
  "amount":     "<i128>"
}
```

---

### `InvestmentWithdrawn`

Emitted when an investor withdraws their active investment before settlement.

**Topic:** `investment_withdrawn`

```json
{
  "investment_id": "<bytes32>",
  "invoice_id":    "<bytes32>",
  "investor":      "<address>",
  "amount":        "<i128>"
}
```

---

## KYC & Verification Events

### `InvestorVerified`

Emitted when an admin verifies an investor's KYC submission and sets their
investment limit.

**Topic:** `investor_verified`

```json
{
  "investor":         "<address>",
  "investment_limit": "<i128>",
  "verified_at":      "<u64>"
}
```

---

## Dispute Events

### `DisputeCreated`

Emitted when a business or investor opens a dispute on a funded invoice.

**Topic:** `dispute_created`  
**Semantic alias:** `DisputeOpened`

```json
{
  "invoice_id":  "<bytes32>",
  "created_by":  "<address>",
  "reason":      "<string>",
  "timestamp":   "<u64>"
}
```

> **Privacy:** `reason` must not contain PII. It is a reason code or a brief
> description. Max length enforced by `MAX_DISPUTE_REASON_LENGTH`.

---

### `DisputeUnderReview`

Emitted when an admin moves a dispute into the review state.

**Topic:** `dispute_under_review`

```json
{
  "invoice_id":  "<bytes32>",
  "reviewed_by": "<address>",
  "timestamp":   "<u64>"
}
```

---

### `DisputeResolved`

Emitted when an admin closes a dispute with a resolution.

**Topic:** `dispute_resolved`

```json
{
  "invoice_id":  "<bytes32>",
  "resolved_by": "<address>",
  "resolution":  "<string>",
  "timestamp":   "<u64>"
}
```

---

## Fee Events

### `PlatformFeeUpdated`

Emitted when the global platform fee (in basis points) is changed.

**Topic:** `platform_fee_updated`

```json
{
  "fee_bps":    "<u32>",
  "updated_at": "<u64>",
  "updated_by": "<address>"
}
```

> 1 bps = 0.01%. Maximum value enforced by `MAX_PLATFORM_FEE_BPS`.

---

### `FeeStructureUpdated`

Emitted when a named fee type (e.g. `ProtocolFee`, `LiquidityFee`) is updated.

**Topic:** `fee_structure_updated`

```json
{
  "fee_type":    "<string>",
  "old_fee_bps": "<u32>",
  "new_fee_bps": "<u32>",
  "updated_by":  "<address>",
  "timestamp":   "<u64>"
}
```

---

### `PlatformFeeRouted`

Emitted when collected platform fees are forwarded to the treasury.

**Topic:** `platform_fee_routed`

```json
{
  "invoice_id":  "<bytes32>",
  "recipient":   "<address>",
  "fee_amount":  "<i128>",
  "timestamp":   "<u64>"
}
```

---

### `ProfitFeeBreakdown`

Emitted alongside `InvoiceSettled` to provide a detailed fee breakdown for
auditing.

**Topic:** `profit_fee_breakdown`

```json
{
  "invoice_id":        "<bytes32>",
  "investment_amount": "<i128>",
  "payment_amount":    "<i128>",
  "gross_profit":      "<i128>",
  "platform_fee":      "<i128>",
  "investor_return":   "<i128>",
  "fee_bps_applied":   "<i128>",
  "timestamp":         "<u64>"
}
```

> `investor_return + platform_fee == payment_amount` (conservation invariant).

---

## Insurance Events

### `InsuranceAdded`

Emitted when an investor adds insurance coverage to their investment.

**Topic:** `insurance_added`

```json
{
  "investment_id":       "<bytes32>",
  "invoice_id":          "<bytes32>",
  "investor":            "<address>",
  "provider":            "<address>",
  "coverage_percentage": "<u32>",
  "coverage_amount":     "<i128>",
  "premium_amount":      "<i128>"
}
```

---

### `InsurancePremiumCollected`

Emitted when the insurance premium is transferred to the provider.

**Topic:** `insurance_premium_collected`

```json
{
  "investment_id":  "<bytes32>",
  "provider":       "<address>",
  "premium_amount": "<i128>"
}
```

---

### `InsuranceClaimed`

Emitted when an investor successfully claims insurance after a default.

**Topic:** `insurance_claimed`

```json
{
  "investment_id":   "<bytes32>",
  "invoice_id":      "<bytes32>",
  "provider":        "<address>",
  "coverage_amount": "<i128>"
}
```

---

## Invoice Metadata Events

### `InvoiceMetadataUpdated`

Emitted when structured line-item metadata is attached to an invoice.

**Topic:** `invoice_metadata_updated`

> **Privacy:** `customer_name` and `tax_id` are **not** emitted. Only aggregate
> statistics are included to prevent PII leakage.

```json
{
  "invoice_id":      "<bytes32>",
  "line_item_count": "<u32>",
  "total_value":     "<i128>",
  "timestamp":       "<u64>"
}
```

---

### `InvoiceMetadataCleared`

Emitted when metadata is removed from an invoice.

**Topic:** `invoice_metadata_cleared`

```json
{
  "invoice_id": "<bytes32>",
  "business":   "<address>"
}
```

---

### `InvoiceCategoryUpdated`

Emitted when the business changes an invoice's category.

**Topic:** `invoice_category_updated`

```json
{
  "invoice_id":   "<bytes32>",
  "business":     "<address>",
  "old_category": "<string>",
  "new_category": "<string>"
}
```

---

### `InvoiceTagAdded` / `InvoiceTagRemoved`

Emitted when searchable tags are added or removed.

**Topics:** `invoice_tag_added` / `invoice_tag_removed`

```json
{
  "invoice_id": "<bytes32>",
  "business":   "<address>",
  "tag":        "<string>"
}
```

---

## Protocol Lifecycle Events

### `ProtocolInitialized`

Emitted once when the contract is first initialised.

**Topic:** `protocol_initialized`

```json
{
  "admin":                "<address>",
  "treasury":             "<address>",
  "fee_bps":              "<u32>",
  "min_invoice_amount":   "<i128>",
  "max_due_date_days":    "<u64>",
  "grace_period_seconds": "<u64>",
  "timestamp":            "<u64>"
}
```

---

### `AdminInitialized`

Emitted when the admin key is set or transferred.

**Topic:** `admin_initialized`

```json
{
  "admin": "<address>"
}
```

---

## Backup & Retention Events

### `BackupCreated`

```json
{ "backup_id": "<bytes32>", "invoice_count": "<u32>", "timestamp": "<u64>" }
```

### `BackupRestored`

```json
{ "backup_id": "<bytes32>", "invoice_count": "<u32>", "timestamp": "<u64>" }
```

### `BackupValidated`

```json
{ "backup_id": "<bytes32>", "success": "<bool>", "timestamp": "<u64>" }
```

### `BackupArchived`

```json
{ "backup_id": "<bytes32>", "timestamp": "<u64>" }
```

### `BackupsCleaned`

```json
{ "removed_count": "<u32>", "timestamp": "<u64>" }
```

### `RetentionPolicyUpdated`

```json
{
  "max_backups":           "<u32>",
  "max_age_seconds":       "<u64>",
  "auto_cleanup_enabled":  "<bool>",
  "timestamp":             "<u64>"
}
```

---

## Miscellaneous Events

### `RevenueDistributed`

Emitted when accumulated revenue is split between treasury, developer, and
platform pools.

**Topic:** `revenue_distributed`

```json
{
  "period":            "<u64>",
  "treasury_amount":   "<i128>",
  "developer_amount":  "<i128>",
  "platform_amount":   "<i128>"
}
```

---

### `TtlExtended`

Emitted by the internal TTL keeper when storage entries are bumped.

**Topic:** `ttl_extended`

```json
{
  "kind":  "<string>",
  "count": "<u32>"
}
```

---

### `BidTtlUpdated`

Emitted when an admin changes the bid time-to-live window.

**Topic:** `bid_ttl_updated`

```json
{
  "old_days":  "<u64>",
  "new_days":  "<u64>",
  "admin":     "<address>",
  "timestamp": "<u64>"
}
```

---

## Semantic Aliases

The following type aliases exist in `src/events.rs` for domain clarity.
Indexers should subscribe using the **canonical topic** listed in the table.

| Alias           | Canonical Type      | Canonical Topic      |
|-----------------|---------------------|----------------------|
| `InvoiceCreated`| `InvoiceUploaded`   | `invoice_uploaded`   |
| `FundsLocked`   | `EscrowCreated`     | `escrow_created`     |
| `LoanSettled`   | `InvoiceSettled`    | `invoice_settled`    |
| `DisputeOpened` | `DisputeCreated`    | `dispute_created`    |

---

## Privacy & Security Notes

- **No PII is emitted.** Customer names, tax IDs, and KYC data are never
  included in any event payload.
- `InvoiceMetadataUpdated` emits only aggregate counts and totals, not raw
  line items.
- `DisputeCreated.reason` is a short reason code. Engineers must not write PII
  into this field when calling `open_dispute`.
- All `address` values are Stellar public keys (G…) or contract IDs (C…).
  They are pseudonymous, not personally identifiable.

---

## Source Reference

All event struct definitions live in
[`quicklendx-contracts/src/events.rs`](../quicklendx-contracts/src/events.rs).
Topic constants are prefixed `TOPIC_` and are the authoritative source of truth
for topic strings. Any change to a topic constant is a **breaking schema change**
and requires a version bump.

---

*See also: [`docs/events.md`](events.md) — event ingestion API for the backend
webhook endpoint.*
