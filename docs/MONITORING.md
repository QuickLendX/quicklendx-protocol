# QuickLendX Operator Monitoring Guide

This document outlines the key protocol events that node operators and support teams should monitor, along with recommended alert thresholds.

## Audience
This guide is written for **Operators** running off-chain indexers and alert pipelines. For downstream integrators consuming these events, refer to [events.md](events.md). If an alert requires action, refer to the [Incident Response Runbook](RUNBOOK_INCIDENT_RESPONSE.md).

## Counters and Alert Thresholds

Operators should track the following Soroban contract events (emitted from `quicklendx-contracts/src/events.rs`):

### 1. `DisputeCreated`
Emitted when a user opens a dispute on an invoice or settlement.
* **Topic**: `dispute_created`
* **Threshold**: **1 (Immediate Alert)**
* **Why**: Any dispute requires manual triage by the support team to freeze funds and assess the claim.
* **Example Event**:
  ```json
  {
    "id": "event-12345",
    "ledger": 1234567,
    "txHash": "0xabc...",
    "type": "DisputeCreated",
    "payload": {
      "invoice_id": "inv_9876",
      "initiator": "G...USERADDRESS"
    },
    "timestamp": 1690000000,
    "complianceHold": true
  }
  ```

### 2. `InvoiceDefaulted`
Emitted when a financed invoice misses its final settlement deadline.
* **Topic**: `invoice_defaulted`
* **Threshold**: **1 (Immediate Alert)**
* **Why**: Defaults are critical credit events. Operators must notify investors and initiate off-chain recovery procedures.

### 3. `EscrowRefunded`
Emitted when locked investor funds are refunded due to a cancelled bid or expired invoice.
* **Topic**: `escrow_refunded`
* **Threshold**: **> 5 per hour**
* **Why**: An occasional refund is normal (e.g., a business rejects a bid). A spike in refunds indicates a potential systemic issue where bids are consistently failing to settle.

## Cross-References
* Main documentation: [README.md](../README.md)
* Incident procedures: [RUNBOOK_INCIDENT_RESPONSE.md](RUNBOOK_INCIDENT_RESPONSE.md)
* API Schemas: [events.md](events.md)
