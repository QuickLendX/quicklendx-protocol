# Fee Recipient Rotation Guide

This document is an **operator's guide** to safely rotating the treasury/fee recipient address in the QuickLendX Soroban contracts.

To prevent human error (like sending protocol fees to a typo'd or inaccessible address) and to give the community/team time to verify changes, the protocol enforces a **two-step rotation flow with a timelock**.

## The Two-Step Flow

### Step 1: Initiate Rotation (Admin)

The current protocol admin initiates the rotation by proposing a new address. This records the intent on-chain and starts the timelock.

**Entrypoint:** `initiate_treasury_rotation(new_treasury)`

**Example (Soroban CLI):**
```bash
soroban contract invoke \
  --id CD... \
  --source admin \
  -- \
  initiate_treasury_rotation \
  --new_treasury GBNEWTREASURYADDRESS...
```

**Timelock:**
Once initiated, a **1-day (86,400 seconds) timelock** begins. The rotation cannot be confirmed until this delay has elapsed.

### Step 2: Confirm Rotation (New Treasury)

After the 1-day timelock has elapsed, the rotation must be confirmed. 
**Crucially, this entrypoint must be authorized by the `new_treasury` address, not the admin.** This proves that the operator actually holds the private key to the new destination address.

**Entrypoint:** `confirm_treasury_rotation(new_treasury)`

**Example (Soroban CLI):**
```bash
soroban contract invoke \
  --id CD... \
  --source new_treasury_key \
  -- \
  confirm_treasury_rotation \
  --new_treasury GBNEWTREASURYADDRESS...
```

**Deadline (TTL):**
The confirmation must happen before the rotation request expires (currently 7 days after initiation). If the deadline passes, the rotation request is dropped, and you must start over from Step 1.

## Cancelling a Pending Rotation

If a rotation was initiated by mistake, or if a security concern arises during the 1-day timelock window, the admin can cancel the pending rotation.

**Entrypoint:** `cancel_treasury_rotation()`

**Example (Soroban CLI):**
```bash
soroban contract invoke \
  --id CD... \
  --source admin \
  -- \
  cancel_treasury_rotation
```

If cancelled, the active treasury remains unchanged, and fees continue to route to the old address.
