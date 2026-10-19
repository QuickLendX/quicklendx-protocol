# Multisig Signer Rotation with Timelock

**Audience:** Operators managing admin key rotation on a QuickLendX deployment where the admin is a Stellar multisig account or external governance contract.

This document walks through the complete procedure for safely rotating signers on a multisig-controlled admin account, incorporating the on-chain timelock safeguards built into the QuickLendX protocol.

---

## Background: Single-Admin Model with External Multisig

The QuickLendX smart contract uses a **single-admin** model: there is exactly one privileged `admin` address at any time. This address is stored on-chain and enforces authorization for all privileged operations (pause, protocol config, emergency recovery, admin handover).

**The contracts do not implement native multisig**. Instead, multisig governance is achieved by setting a **Stellar multisig account** (or external smart contract with M-of-N signer logic) as the admin address. All privileged entrypoints then require signatures from the multisig account's threshold of signers.

This architecture separates the concerns:
- **QuickLendX contracts** enforce single-admin authorization and timelocks
- **Stellar multisig layer** (or external contract) enforces M-of-N signer quorum

---

## Signer Rotation: Two-Step Handover Flow

When rotating signers on a multisig admin, follow the protocol's **two-step admin handover** flow. This protects against misconfigurations and proves the new multisig account is live before control moves.

### Why Two-Step?

A one-step transfer (`transfer_admin(new_admin)`) moves control immediately. If the new address is incorrect, inaccessible, or has lost signer keys, **protocol ownership is lost forever** with no recovery path.

Two-step handover forces the new admin to explicitly accept before the transfer completes. This proves:
1. The new address is reachable (signers can sign transactions)
2. The new multisig threshold is configured correctly
3. No typos in the destination address

### Enabling Two-Step Mode

Before any rotation, the current admin must enable two-step mode:

```shell
# Current multisig admin calls this with M-of-N signatures
soroban contract invoke \
  --id $CONTRACT_ID \
  --source-account $CURRENT_MULTISIG \
  -- set_two_step_enabled \
  --admin $CURRENT_MULTISIG \
  --enabled true
```

Verify it succeeded:
```shell
soroban contract invoke --id $CONTRACT_ID -- is_two_step_enabled
# Returns: true
```

---

## Rotation Procedure: Step-by-Step

### 1. Prepare the New Multisig Account

Before initiating the handover on-chain, create and configure the new Stellar multisig account:

```shell
# Generate new Stellar account
NEW_MULTISIG=GXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX

# Add N signers (off-chain Stellar account configuration)
stellar account merge --source $NEW_SIGNER_1 --destination $NEW_MULTISIG --threshold M
stellar account set-signer --source $NEW_MULTISIG --signer $NEW_SIGNER_2 --weight 1
...
stellar account set-threshold --source $NEW_MULTISIG --medium-threshold M --high-threshold M
```

**Verify the new multisig account:**
- Confirm all intended signers are added
- Confirm the M-of-N threshold is correct
- **Test signing a benign transaction** with the new multisig to prove the quorum works

### 2. Initiate the Transfer (Current Multisig Signs)

The current multisig admin proposes the new admin:

```shell
soroban contract invoke \
  --id $CONTRACT_ID \
  --source-account $CURRENT_MULTISIG \
  -- initiate_admin_transfer \
  --admin $CURRENT_MULTISIG \
  --new_admin $NEW_MULTISIG
```

**Observed behavior:**
- The contract stores `$NEW_MULTISIG` as pending admin
- The contract sets a **transfer lock** (no other transfers can be initiated until this resolves)
- Event `AdminTransferInitiated(current=$CURRENT_MULTISIG, pending=$NEW_MULTISIG)` is emitted

**Current admin retains full control** until the new admin accepts. The current admin can cancel at any time.

### 3. Verify Pending State

```shell
# Check that the pending admin is recorded correctly
soroban contract invoke --id $CONTRACT_ID -- get_pending_admin
# Returns: Some($NEW_MULTISIG)

# Confirm the transfer lock is active
soroban contract invoke --id $CONTRACT_ID -- is_transfer_locked
# Returns: true

# Confirm current admin is unchanged
soroban contract invoke --id $CONTRACT_ID -- get_current_admin
# Returns: $CURRENT_MULTISIG
```

### 4. Accept the Transfer (New Multisig Signs)

The new multisig admin must **authorize and accept** the pending transfer. This requires M-of-N signatures from the new multisig signers:

```shell
soroban contract invoke \
  --id $CONTRACT_ID \
  --source-account $NEW_MULTISIG \
  -- accept_admin_transfer \
  --pending_admin $NEW_MULTISIG
```

> **Note:** The contract's `accept_admin_transfer` implementation in `AdminStorage` is not currently exposed as a public entrypoint in `lib.rs`. This is an implementation gap. For now, operators will need to add this entrypoint or access it via a governance wrapper contract. The expected signature is:
> ```rust
> pub fn accept_admin_transfer(env: Env, pending_admin: Address) -> Result<(), QuickLendXError>
> ```
> This requires auth from the `pending_admin` address.

**Observed behavior after acceptance:**
- The current admin is now `$NEW_MULTISIG`
- Pending admin is cleared (`None`)
- Transfer lock is released (`false`)
- Event `AdminTransferred(from=$CURRENT_MULTISIG, to=$NEW_MULTISIG)` is emitted

### 5. Verify Handover Completion

```shell
# Confirm the new admin is live
soroban contract invoke --id $CONTRACT_ID -- get_current_admin
# Returns: $NEW_MULTISIG

# Confirm the pending admin is cleared
soroban contract invoke --id $CONTRACT_ID -- get_pending_admin
# Returns: None

# Confirm the transfer lock is released
soroban contract invoke --id $CONTRACT_ID -- is_transfer_locked
# Returns: false
```

### 6. Test New Admin Authority

Perform a benign admin operation to confirm the new multisig works:

```shell
# Example: toggle two-step mode (no-op if already enabled, just tests auth)
soroban contract invoke \
  --id $CONTRACT_ID \
  --source-account $NEW_MULTISIG \
  -- set_two_step_enabled \
  --admin $NEW_MULTISIG \
  --enabled true
```

If this succeeds, the rotation is complete.

---

## Emergency: Canceling a Pending Transfer

If the new multisig account is misconfigured or the rotation needs to be aborted:

```shell
# Current admin cancels the pending transfer
soroban contract invoke \
  --id $CONTRACT_ID \
  --source-account $CURRENT_MULTISIG \
  -- cancel_admin_transfer \
  --admin $CURRENT_MULTISIG
```

> **Note:** The `cancel_admin_transfer` entrypoint also needs to be added to `lib.rs` public entrypoints. The expected signature is:
> ```rust
> pub fn cancel_admin_transfer(env: Env, current_admin: Address) -> Result<(), QuickLendXError>
> ```

**Observed behavior after cancellation:**
- Pending admin is cleared (`None`)
- Transfer lock is released (`false`)
- Current admin remains unchanged (`$CURRENT_MULTISIG`)
- Event `AdminTransferCancelled(admin=$CURRENT_MULTISIG, pending=$NEW_MULTISIG)` is emitted

---

## Timelocked Treasury Rotation

The treasury address (recipient of protocol fees) can also be rotated. Unlike admin handover, treasury rotation uses a **timelock** instead of two-step acceptance.

### Procedure

**Initiate rotation** (requires admin auth):
```shell
soroban contract invoke \
  --id $CONTRACT_ID \
  --source-account $ADMIN_MULTISIG \
  -- set_treasury \
  --admin $ADMIN_MULTISIG \
  --treasury $NEW_TREASURY_ADDRESS
```

The contract records `$NEW_TREASURY_ADDRESS` as pending and sets an unlock timestamp (default timelock: check `storage::get_pending_treasury` for the actual timestamp).

**Check pending treasury:**
```shell
soroban contract invoke --id $CONTRACT_ID -- get_pending_treasury
# Returns: Some(($NEW_TREASURY_ADDRESS, unlock_timestamp))
```

**Wait for the timelock to elapse.** Once `current_ledger_time >= unlock_timestamp`, the treasury rotation executes automatically on the next fee distribution or treasury write.

**Cancel before execution** (if needed):
```shell
soroban contract invoke \
  --id $CONTRACT_ID \
  --source-account $ADMIN_MULTISIG \
  -- cancel_treasury_rotation \
  --admin $ADMIN_MULTISIG
```

This clears the pending treasury and prevents execution.

---

## Timelocked Emergency Withdrawal

The most privileged operation — recovering tokens sent to the contract by mistake — is protected by a **24-hour timelock** by default. This gives the community a window to observe and react before funds move.

### Procedure

**1. Initiate emergency withdrawal** (admin-only):
```shell
soroban contract invoke \
  --id $CONTRACT_ID \
  --source-account $ADMIN_MULTISIG \
  -- initiate_emergency_withdraw \
  --admin $ADMIN_MULTISIG \
  --token $TOKEN_ADDRESS \
  --amount 1000000 \
  --target_address $RECOVERY_ADDRESS
```

**Observed behavior:**
- The contract records the pending withdrawal with `unlock_at = now + 24 hours` and `expires_at = unlock_at + 7 days`
- A monotonically increasing `nonce` is assigned (prevents replay of old requests)
- Event `EmergencyWithdrawalInitiated(...)` is emitted

**2. Monitor the pending withdrawal:**
```shell
soroban contract invoke --id $CONTRACT_ID -- get_pending_emergency_withdraw
# Returns: PendingEmergencyWithdrawal { token, amount, target, unlock_at, expires_at, ... }

# Check time until unlock
soroban contract invoke --id $CONTRACT_ID -- time_until_unlock
# Returns: Some(seconds_remaining)  or  Some(0) if ready

# Check if execution is allowed
soroban contract invoke --id $CONTRACT_ID -- can_execute
# Returns: Some(true) if timelock elapsed and all checks pass
```

**3. Execute after timelock elapses:**
```shell
# Wait until now >= unlock_at
soroban contract invoke \
  --id $CONTRACT_ID \
  --source-account $ADMIN_MULTISIG \
  -- execute_emergency_withdraw \
  --admin $ADMIN_MULTISIG
```

**Security checks at execution time:**
- Timelock elapsed: `now >= unlock_at`
- Not expired: `now < expires_at` (7-day window after unlock)
- Not cancelled: `cancelled == false`
- Sufficient balance: the contract must have enough non-escrow surplus (held escrow reserves are protected)

**Execution window:** `[unlock_at, expires_at)` — inclusive lower bound, exclusive upper bound.

**4. Cancel if needed** (admin-only, before execution):
```shell
soroban contract invoke \
  --id $CONTRACT_ID \
  --source-account $ADMIN_MULTISIG \
  -- cancel_emergency_withdraw \
  --admin $ADMIN_MULTISIG
```

**Observed behavior after cancellation:**
- The pending withdrawal is marked `cancelled = true`
- The `nonce` is permanently burned (prevents replay even if the timelock passes)
- Event `EmergencyWithdrawalCancelled(...)` is emitted

---

## Timelock Parameters

| Parameter | Default Value | Configurable? | Notes |
|-----------|---------------|---------------|-------|
| Emergency withdraw timelock | 24 hours | Yes (1 hour - 30 days) | Time between initiate and execute |
| Emergency withdraw expiration | 7 days after unlock | Yes | Execution window duration |
| Treasury rotation timelock | Check `storage` module | Implementation-specific | Time before new treasury activates |
| Admin handover timelock | None (two-step acceptance) | N/A | No timelock; requires explicit acceptance |

---

## Operator Checklist

- [ ] Verify two-step mode is enabled: `is_two_step_enabled() == true`
- [ ] Prepare and test the new multisig account off-chain (confirm M-of-N threshold)
- [ ] Initiate transfer: `initiate_admin_transfer(current, new)`
- [ ] Verify pending admin: `get_pending_admin() == Some(new)`
- [ ] Accept transfer from new multisig: `accept_admin_transfer(new)`
- [ ] Verify handover: `get_current_admin() == new`
- [ ] Test new admin authority with a benign operation
- [ ] Monitor emergency withdrawals: `get_pending_emergency_withdraw()` should be `None` in normal operation
- [ ] If emergency withdraw is initiated, verify `unlock_at` and `time_until_unlock()` before execution

---

## See Also

- [`docs/GOVERNANCE.md`](GOVERNANCE.md) — Governance model, admin handover, and timelock parameters (operator-facing).
- [`docs/contracts/admin.md`](contracts/admin.md) — Admin role and access control (contract reference).
- [`quicklendx-contracts/docs/admin-transfer.md`](../quicklendx-contracts/docs/admin-transfer.md) — Detailed admin transfer flow with security analysis.
- [`quicklendx-contracts/src/admin.rs`](../quicklendx-contracts/src/admin.rs) — Admin storage and two-step transfer implementation.
- [`quicklendx-contracts/src/emergency.rs`](../quicklendx-contracts/src/emergency.rs) — Timelocked emergency withdrawal implementation.
- [`docs/RUNBOOK_INCIDENT_RESPONSE.md`](RUNBOOK_INCIDENT_RESPONSE.md) — Operator playbook for incident-mode recovery.

---

## Known Limitations

**Missing public entrypoints:** As of this writing, `accept_admin_transfer` and `cancel_admin_transfer` are not exposed as public contract entrypoints in `lib.rs`, though they are implemented in `AdminStorage` (`admin.rs`). Operators will need to:
- Add these entrypoints to `lib.rs`:
  ```rust
  pub fn accept_admin_transfer(env: Env, pending_admin: Address) -> Result<(), QuickLendXError> {
      AdminStorage::accept_admin_transfer(&env, &pending_admin)
  }

  pub fn cancel_admin_transfer(env: Env, current_admin: Address) -> Result<(), QuickLendXError> {
      AdminStorage::cancel_admin_transfer(&env, &current_admin)
  }
  ```
- Or access these functions via a governance wrapper contract.

This gap should be addressed in a future release. The implementation exists and is tested (see `test_admin_two_step.rs`); only the public entrypoint exposure is missing.
