# QuicklendX Investigation Workflow

This document outlines the end-to-end workflow for investigating unexpected behavior or failures in the QuicklendX smart contracts. It is intended for **Operators** and support teams who need to triage, investigate, and escalate issues with concrete SLAs.

## Service Level Agreements (SLAs)

| Severity | Definition | Initial Triage SLA | Investigation SLA |
|----------|------------|--------------------|-------------------|
| **P0** | Critical contract failure (e.g., locked funds, core logic panic, security breach). | 15 minutes | Continuous until resolved |
| **P1** | Major functionality broken for multiple users, no workarounds. | 1 hour | 24 hours |
| **P2** | Isolated failure, degraded performance, or clear workaround exists. | 24 hours | 1 week |

## Phase 1: Triage and Initial Gathering (0-1 hour)

When an alert fires or a user reports an issue, the first step is to gather the facts.

1. **Identify the Entrypoint:** Which contract function failed?
2. **Collect Transaction Data:**
   - Transaction hash
   - `invoice_id` or `bid_id` involved
   - Invoker account ID
3. **Check the Indexer/Dashboards:**
   - Review [EVENT_DASHBOARDS.md](EVENT_DASHBOARDS.md) for related error events or spikes.
   - Example Query: Look for `Error` events associated with the specific `invoice_id`.

**Concrete Example:**
A user reports they cannot accept a bid. You need to gather:
- The `invoice_id` (e.g., `inv_12345`)
- The `bid_id` (e.g., `bid_67890`)
- The transaction hash returning the error (e.g., `tx_abc123`).

## Phase 2: Investigation (1-24 hours)

Once triaged, begin the technical investigation.

1. **Reconstruct the State:**
   - Use the `get_invoice` query on the contract or indexer to check the current status of the entity.
   - Example output to look for: Is the invoice in `Funded` state when it should be `Open`?
2. **Review On-Chain Events:**
   - Check the event logs leading up to the failure. Look for state transitions that might have blocked the action.
   - See [ON_CHAIN_LOGS.md](ON_CHAIN_LOGS.md) for event decoding.
3. **Reproduce Locally (If applicable):**
   - Write a quick test case mimicking the state and action.
   - Example: Initialize an invoice with the exact parameters from mainnet and attempt the failing call.

## Phase 3: Resolution and Escalation

- **Known Issue:** If the error matches a known limitation or error code (see [contracts/errors.md](contracts/errors.md)), provide the documented resolution or workaround to the user.
- **Unknown Issue/Bug:** If the behavior contradicts the intended contract logic, escalate to the engineering team.
  - Create a high-priority issue detailing the steps from Phase 1 and 2.
  - Link the transaction hashes and initial state.
- **Emergency Action:** If the issue poses an immediate risk to funds, refer to the [RUNBOOK_INCIDENT_RESPONSE.md](RUNBOOK_INCIDENT_RESPONSE.md) for pause/circuit-breaker procedures.

## Cross-References

- [RUNBOOK_INCIDENT_RESPONSE.md](RUNBOOK_INCIDENT_RESPONSE.md): For emergency response procedures.
- [EVENT_DASHBOARDS.md](EVENT_DASHBOARDS.md): For monitoring and querying events.
- [contracts/errors.md](contracts/errors.md): Reference for contract error codes.
