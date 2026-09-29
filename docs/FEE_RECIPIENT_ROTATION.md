# Fee Recipient Rotation Guide

This document is an **operator's guide** to safely rotating the treasury/fee recipient address in the QuickLendX Soroban contracts.

To prevent human error (like sending protocol fees to a typo'd or inaccessible address) and to give the community/team time to verify changes, the protocol enforces a **two-step rotation flow with a timelock**.

## The Two-Step Flow

### Step 1: Initiate Rotation (Admin)

The current protocol admin initiates the rotation by proposing a new address. This records the intent on-chain and starts the timelock. The initial recipient may be configured once during fee-system setup; replacing an active recipient must use this delayed flow.

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

### Step 2: Confirm Rotation (Admin and New Treasury)

After the 1-day timelock has elapsed, the rotation must be confirmed. The configured administrator must authorize finalization, and the proposed `new_treasury` address must also authorize the underlying confirmation. This provides both governance approval and proof that the destination key is controlled by the operator.

**Entrypoint:** `confirm_treasury_rotation(new_treasury)`

**Example (Soroban CLI):**
```bash
soroban contract invoke \
  --id CD... \
  --source admin_key \
  -- \
  confirm_treasury_rotation \
  --new_treasury GBNEWTREASURYADDRESS...
```

The new recipient must authorize the same transaction as required by the
contract's two-party confirmation guard.

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

Initiation, cancellation, and successful confirmation each emit a distinct
control-plane event. Indexers should retain all three event types, including
cancellations, so operators can reconstruct the complete review history.

## Security and compatibility notes

The active fee route is stored in the fee-system platform configuration. Its
initial recipient can be established once; later changes through the legacy
single-step configuration entrypoint are rejected. The rotation request is
stored separately, so a rejected or cancelled request leaves the active
recipient untouched.

An attacker cannot finalize a request by supplying the proposed address alone:
the configured administrator must authorize the finalization and the proposed
recipient must authorize the confirmation. Wrong-address, early, expired, or
replayed finalization attempts return before the active configuration changes.

No migration is required. Existing platform fee configuration records keep
their serialized shape, and existing recipients remain active until a valid
rotation is finalized. To roll back a mistaken finalized change, the admin
can initiate a new rotation back to the previous address and wait through the
same review delay; there is no emergency single-step bypass.

## Validation matrix

The contract-level regression suite covers the following state transitions:

| Scenario | Required invariant |
| --- | --- |
| Bootstrap configuration | The first recipient can be established once |
| Legacy reconfiguration | Rejected; active recipient is unchanged |
| Proposal | Pending request is queryable and routing is unchanged |
| Duplicate proposal | Rejected; the first request remains intact |
| Same-address proposal | Rejected without creating a request |
| Early finalization | Rejected; request and routing remain intact |
| Exact minimum delay | Finalization succeeds |
| Wrong proposed address | Rejected without consuming the request |
| Exact confirmation deadline | Finalization succeeds |
| Expired confirmation | Request is cleared and routing is unchanged |
| Cancellation | Request is cleared, old recipient stays active, event is emitted |
| Replayed finalization | Rejected after the request is consumed |
| Replacement after cancellation/expiry | A new timelock starts |
| Sequential rotations | Every proposal and confirmation remains indexable |

Settlement resolves the recipient from the active platform fee configuration
at the time the fee is routed. Settlements before finalization use the old
address, while settlements after finalization use the new address; a pending
proposal does not affect either path. This keeps the review window auditable
without changing already-routed funds.

For incident response, cancel the pending request during the review window and
verify the active address with `get_treasury_address`. If the request has
already been finalized, record the confirmation event and initiate a compensating
rotation to the intended address. Operators should compare the initiated,
confirmed, and cancelled event sequence against the pending-request view before
approving a second rotation.
