# QuickLendX Freeze and Recovery Playbook

This document is an **operator playbook** describing how an invoice freeze is applied, monitored, and lifted within the QuickLendX protocol.

## 1. What is an Invoice Freeze?

An invoice freeze is an administrative action that temporarily suspends all state transitions for a specific invoice. This is typically used for compliance checks, fraud investigations, or legal holds. 

## 2. Applying a Freeze

An administrator can apply a freeze by calling the `freeze_invoice` entrypoint on the contract. A freeze requires a valid `BusinessFreezeReason` to document why the action was taken.

### Valid Freeze Reasons (`BusinessFreezeReason`)
* `AdminAction`: Generic administrative freeze (admin's discretion).
* `KYCRejected`: Business KYC was rejected or revoked.
* `ComplianceViolation`: Legal or compliance policy violation.
* `SuspiciousActivity`: Fraud or suspicious activity detected.
* `LegalHold`: Court order or legal hold applied.

### Concrete Example: Applying a Freeze

If an operator detects suspicious activity for an invoice, they can invoke the `freeze_invoice` endpoint. Here is how that looks via the CLI:

```bash
# Example Soroban CLI command to freeze an invoice
soroban contract invoke \
  --id C... \
  --source-account admin \
  --network testnet \
  -- \
  freeze_invoice \
  --admin G... \
  --invoice_id 0000000000000000000000000000000000000000000000000000000000000001 \
  --reason SuspiciousActivity
```

## 3. Monitoring a Freeze

Operators can query the current freeze status and metadata of any invoice using the `get_invoice_freeze_info` entrypoint.

### Concrete Example: Querying Freeze Info

```bash
# Example Soroban CLI command to query freeze info
soroban contract invoke \
  --id C... \
  --network testnet \
  -- \
  get_invoice_freeze_info \
  --invoice_id 0000000000000000000000000000000000000000000000000000000000000001
```

**Example Output:**
```json
{
  "reason": "SuspiciousActivity",
  "frozen_by": "G...",
  "frozen_at": 1690000000
}
```
If the invoice is not frozen, the response is empty (`None` in the contract).

## 4. Lifting a Freeze (Recovery)

Once the investigation is resolved or the hold is lifted, an administrator can resume normal operations by calling `unfreeze_invoice`. This completely removes the frozen flag and the stored freeze metadata.

### Concrete Example: Lifting a Freeze

```bash
# Example Soroban CLI command to unfreeze an invoice
soroban contract invoke \
  --id C... \
  --source-account admin \
  --network testnet \
  -- \
  unfreeze_invoice \
  --admin G... \
  --invoice_id 0000000000000000000000000000000000000000000000000000000000000001
```

After this command succeeds, the invoice state machine continues from where it was suspended, allowing normal bids, escrow releases, and settlements to resume.
