# QuickLendX Platform Fees

This document outlines the QuickLendX protocol's fee schedule, volume tier progression, and the manual process for operators to override fee tiers for specific tenants. It is intended primarily for **Operators**.

## 1. Fee Schedule Overview

The QuickLendX smart contract implements a standardized fee structure based on basis points (bps), where 10,000 bps = 100%. All calculations use integer arithmetic and round down in favor of investors.

### Default Base Fees

- **Platform Fee**: `200 bps` (2.0%) — Assessed on the gross profit of a settled invoice.
- **Processing Fee**: `50 bps` (0.5%) — Base fee for transaction processing.
- **Verification Fee**: `100 bps` (1.0%) — Applicable to verified transactions.

### Adjustments and Modifiers

Fees can be modulated by transaction timing relative to the invoice's due date and grace period:

- **Early Payment Discount**: If a payment settles early, a `1000 bps` (10%) discount is subtracted directly from the calculated **Platform Fee**.
- **Late Payment Surcharge**: If an invoice is paid late, a `2000 bps` (20%) surcharge is added directly to the calculated **Late Payment** fee.

*Note: All base fees have a hard protocol maximum of `1000 bps` (10%). No base fee can exceed this threshold, enforced by `MAX_FEE_BPS`.*

## 2. Volume Tiers

QuickLendX automatically grants fee discounts based on a tenant's cumulative transaction volume (recorded in stroops). These volume tiers apply to all fee types **except** the Late Payment surcharge.

| Tier | Volume Threshold | Fee Discount |
| :--- | :--- | :--- |
| **Standard** | 0 stroops | 0 bps (0%) |
| **Silver** | 100,000,000,000 stroops (100k XLM) | 500 bps (5%) |
| **Gold** | 500,000,000,000 stroops (500k XLM) | 1000 bps (10%) |
| **Platinum** | 1,000,000,000,000 stroops (1M XLM) | 1500 bps (15%) |

When a transaction occurs, the user's volume is automatically updated, and the new tier takes effect for any subsequent transactions.

## 3. How to Override per Tenant

Because the protocol currently derives fee tiers strictly from accumulated volume, there is no direct mechanism (like a `set_tenant_fee` function) to assign a custom percentage to a specific user.

However, operators can manually **override a tenant's fee tier** by artificially inflating their recorded volume, thereby granting them VIP fee discounts immediately.

### Procedure

Operators invoke the publicly accessible `update_user_transaction_volume` endpoint on the contract. By passing a large simulated `transaction_amount`, the tenant's cumulative volume is pushed past the threshold for Silver, Gold, or Platinum tiers.

**Example: Granting a Platinum Override**

To force a tenant into the Platinum tier (granting a 15% discount on standard fees), an operator submits a transaction adding `1,000,000,000,000` to the user's volume:

```bash
soroban contract invoke \
  --id <CONTRACT_ID> \
  --source-account <OPERATOR_SECRET> \
  --network <NETWORK> \
  -- \
  update_user_transaction_volume \
  --user <TENANT_PUBLIC_KEY> \
  --transaction_amount 1000000000000
```

### Important Considerations

- **Cumulative**: The `update_user_transaction_volume` operation adds to the user's existing recorded volume.
- **No Demotion**: Because volume only grows monotonically, an operator cannot currently demote a tenant to a lower tier using this method (as you cannot supply a negative value).
- **Public Endpoint Warning**: The `update_user_transaction_volume` function does not currently enforce `admin.require_auth()`. Operators should be aware of this when relying on tier boundaries.
- **Analytics Skew**: Manual inflation of user volume will cause global platform analytics (e.g., total volume metrics) to report higher off-chain values unless specifically filtered out by the backend indexer.
