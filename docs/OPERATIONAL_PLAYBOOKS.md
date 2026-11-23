# Protocol Reasons, Meaning, and Operational Playbooks

## Target Audience: Operators

This document formalizes the internal operational tribal knowledge of the QuickLendX protocol. It provides concrete playbooks and explains the reasoning behind core protocol behaviors for operators managing the live system.

---

## 1. Dispute Resolution and Fund Freezes

**Meaning and Reason:**
When an invoice is marked as disputed (`open_dispute`), the protocol immediately freezes all associated bids and collateral. This is a deliberate design choice to prevent capital flight while mediation is ongoing.

**Operational Playbook: Handling Stale Disputes**

Operators are responsible for monitoring disputes that have exceeded the standard 14-day mediation window and forcing a timeout resolution if neither party has advanced the state.

1. **Identify Stale Disputes**
   Query the contract for disputes that haven't been resolved.
   ```bash
   soroban contract invoke \
     --id $CONTRACT_ID \
     --source operator_account \
     --network mainnet \
     -- \
     get_dispute_status --invoice_id 1042
   ```

2. **Execute Timeout Resolution**
   If the dispute is beyond the deadline, execute the timeout fallback to release the funds back to the investors.
   ```bash
   soroban contract invoke \
     --id $CONTRACT_ID \
     --source operator_account \
     --network mainnet \
     -- \
     resolve_dispute_timeout --invoice_id 1042
   ```

---

## 2. Platform Fee Adjustments

**Meaning and Reason:**
The platform fee is a dynamic percentage taken from successful settlements. It is kept adjustable rather than hardcoded so the DAO can respond to market conditions. 

**Operational Playbook: Rotating the Fee Rate**

When governance votes to change the platform fee (e.g., to 250 basis points, which is 2.5%), the operator must update the contract state.

1. **Verify Current Rate**
   Ensure you know the starting state before making adjustments.
   ```bash
   soroban contract invoke \
     --id $CONTRACT_ID \
     --source operator_account \
     --network mainnet \
     -- \
     get_platform_fee
   ```
   *Expected output: `[150]` (1.5%)*

2. **Set the New Rate**
   Apply the new approved rate of 250 bps.
   ```bash
   soroban contract invoke \
     --id $CONTRACT_ID \
     --source operator_account \
     --network mainnet \
     -- \
     set_platform_fee --bps 250
   ```

3. **Verify the Change**
   Always read back the state to confirm the update succeeded.
   ```bash
   soroban contract invoke \
     --id $CONTRACT_ID \
     --source operator_account \
     --network mainnet \
     -- \
     get_platform_fee
   ```
   *Expected output: `[250]`*
