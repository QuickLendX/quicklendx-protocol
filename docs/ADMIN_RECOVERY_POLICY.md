# Authorized admin dry-runs and recovery

Administrative recovery is a privileged mutation path. It is intentionally
separate from preview operations so an operator can inspect a proposed change
without accidentally writing configuration, while a recovery request must carry
an explicit reason and an optimistic state check.

## Preview contract

`preview_protocol_config` and `preview_fee_config` are read-only views. They:

1. require the current admin to authorize the request;
2. validate the proposed value with the same rules as the apply path;
3. read the current configuration; and
4. return a before/after diff and no-op flag.

They do not write storage, update balances, change lifecycle state, or emit a
recovery event. A preview is therefore safe to run during an operational review,
but it is not a reservation: another admin mutation can happen before apply.

Unauthorized previews are rejected. Protecting read-only previews prevents
untrusted callers from using validation behavior as an administrative oracle and
keeps the authorization model consistent across admin entry points.

## Recovery contract

`recover_protocol_config` and `recover_fee_config` require:

- the current admin address and its Soroban authorization;
- an `expected_current` value copied from a trusted read;
- a validated replacement value; and
- a non-empty bounded reason string.

The expected value is an optimistic concurrency guard. If the stored value has
changed since the operator prepared the recovery, the call returns
`OperationNotAllowed` and does not write anything. This prevents an incident
operator from overwriting a legitimate concurrent configuration change.

All validation occurs before the single target storage write. Invalid amounts,
fees, due dates, grace periods, and empty reasons fail closed. No partial
protocol/fee update is possible through these entry points.

## Audit event

Each successful recovery emits one `adm_rec` event with a target topic (`proto`
or `fee`) and a payload containing the administrator plus `RecoveryAudit`:

```text
target: proto | fee
reason: bounded operator-provided reason
```

The reason should reference an incident, change, or approved recovery ticket.
Do not put secrets, private keys, or raw credentials in the reason. Indexers
should retain the ledger event and correlate it with the pre-change snapshot,
expected value, replacement value, and operator approval.

Preview calls do not emit this event because they do not mutate state. Normal
configuration changes continue to emit their existing configuration events.

## Failure matrix

| Request | Result | State effect |
| --- | --- | --- |
| Wrong admin | `NotAdmin` | None |
| Missing admin setup | `NotInitialized` | None |
| Empty reason | `InvalidParameter` | None |
| Expected state differs | `OperationNotAllowed` | None |
| Invalid replacement | Existing validation error | None |
| Valid authorized recovery | `Ok(())` | Target replaced and one audit event |

The contract does not catch and convert errors after a storage write. Soroban
transaction atomicity remains the final protection against a host failure. The
explicit preconditions make normal invalid-request behavior deterministic and
easy for clients to handle.

## Operator workflow

1. Pause the affected automation if the incident requires it.
2. Read the current configuration and store the snapshot with the ticket.
3. Run the authorized preview with the proposed replacement.
4. Have an independent reviewer approve the reason and target.
5. Submit the recovery using the exact snapshot as `expected_current`.
6. Confirm the transaction finalizes and inspect the `adm_rec` event.
7. Read the configuration again and compare it with the replacement.
8. Resume automation only after the post-change invariant is confirmed.

If the state guard fails, return to step 2. Do not simply resubmit with the
latest state without a new review; the state change may be the signal that the
original recovery is no longer appropriate.

## Ownership and lifecycle validation

Configuration recovery targets protocol or fee records, not user balances or
invoice ownership. Future recovery operations for invoices must independently
validate target ownership, lifecycle eligibility, pause state, and affected
identifiers. They must not reuse these configuration methods as a generic
mutation backdoor.

Every recovery API should identify exactly one target domain. A request that
combines protocol, fee, balance, and invoice changes should be rejected or split
into separately authorized transactions with separately auditable reasons.

## Rollback

Recovery is itself a normal state transition and can be followed by another
authorized recovery if a new expected snapshot is supplied. There is no hidden
rollback key or bypass path. If a release must be rolled back, preserve the
existing admin and configuration storage layout and retain recovery events.

Never restore a previous configuration by directly deleting storage or by using
an application-side cache. Read the on-chain value, prepare a reviewed expected
snapshot, and submit a new authorized recovery with a reason such as
`rollback-to-release-<id>`.

## Compatibility

The existing initialize, transfer, set, and preview methods remain available.
The recovery methods are additive and do not change the encoding of current
configuration records. Existing clients can continue to read configurations.
New operational tooling should prefer the recovery methods for incident-driven
changes because the expected-state guard and reason are explicit in the call.

## Monitoring

Alert on:

- every `adm_rec` event;
- repeated `OperationNotAllowed` state-guard failures;
- recovery by an unexpected administrator;
- a replacement at a policy boundary; and
- preview requests followed by a replacement that differs from the reviewed
  projection.

The alert should include target, reason, transaction ID, and current/replacement
configuration hashes. It must not include secret key material or unredacted
credentials.

## Test evidence

The admin test additions cover:

- authorized protocol recovery;
- authorized fee recovery;
- one recovery audit event and target metadata;
- stale expected-state rejection;
- invalid replacement rejection before storage mutation;
- empty reason rejection;
- unauthorized recovery rejection; and
- preview state and event invariants.

The repository has unrelated pre-existing compilation failures in its broader
test surface. This change does not disable, delete, or alter those tests. The
focused admin behavior is kept small and reviewable so the baseline issues can
be addressed independently.

## Review checklist

- [ ] Preview paths have no storage or lifecycle writes.
- [ ] Preview paths require the current admin.
- [ ] Recovery paths require admin authorization and a reason.
- [ ] Expected state is checked before replacement.
- [ ] Replacement validation runs before storage mutation.
- [ ] One audit event identifies target and reason.
- [ ] Invalid requests leave state unchanged.
- [ ] Rollback uses a new reviewed recovery, never direct deletion.
- [ ] Monitoring redacts secrets.
- [ ] No unrelated tests or security gates are disabled.

## Dry-run snapshot procedure

For each preview test or operator rehearsal, capture a snapshot of every state
that could be affected: protocol configuration, fee configuration, admin, pause
state, balances, invoice lifecycle, and emitted-event count. Execute the preview
with the authorized admin and compare the snapshot byte-for-byte afterward.

The comparison should be performed against on-chain reads, not against a local
object that may have been mutated by the preview client. A successful preview
must leave the event count unchanged. A failed preview must leave it unchanged
as well; validation errors are not audit-worthy mutations.

## Recovery incident template

An incident record for a recovery should contain:

```text
incident_id:
affected_target: proto | fee
current_snapshot_hash:
replacement_snapshot_hash:
expected_current:
replacement:
preview_transaction:
operator:
independent_approver:
reason:
recovery_transaction:
post_recovery_read:
event_ledger:
```

The template separates the value observed before approval from the value sent
in the recovery call. This distinction is important when an incident lasts long
enough for a concurrent configuration change to occur.

## State and balance safety

The configuration recovery functions do not call token clients, invoice
settlement methods, or balance-moving code. This is intentional. A recovery
that changes configuration and balances in one opaque call would make it harder
to prove which invariant failed. Balance recovery belongs to a separate,
ownership-aware procedure with its own reason, identifiers, and event schema.

Before resuming payment or invoice workers, operators should check that the
replacement configuration is within the protocol bounds and that any dependent
worker has reloaded it from storage. An application cache must never be treated
as proof that the on-chain mutation succeeded.

## Authorization review

The admin address is read from instance storage and compared exactly with the
address supplied to the entry point. The supplied address must also authorize
the invocation. A caller cannot pass a valid admin address while signing as a
different identity. Admin transfer should be completed before recovery if the
original key is unavailable; recovery must not become an alternate key-rotation
mechanism.

Reviewers should compare the authorization entry, expected snapshot, reason,
and emitted event. Mismatches are a deployment blocker even if the replacement
value is technically valid.

## Failure drills

Run these drills on a disposable deployment before enabling automated recovery:

1. Preview a valid protocol change and prove no state or event changes.
2. Preview as an impostor and verify `NotAdmin`.
3. Recover with an empty reason and verify no write.
4. Recover with an invalid replacement and verify no write.
5. Change the configuration, then submit an old expected snapshot and verify
   `OperationNotAllowed`.
6. Recover with the latest snapshot and verify one event and the new value.
7. Replay the same recovery and confirm it requires a new expected state/review.
8. Verify balance and invoice lifecycle state remained untouched throughout.

Record the transaction IDs and event count for every drill. A green happy-path
test is not sufficient evidence that an invalid recovery cannot partially write.

## Release and rollback gates

Before release, CI should compile the admin contract, run its focused tests,
validate the event schema, and lint the changed documentation. Deployment should
verify the admin address and read both configuration records. The release record
should include the contract artifact hash and the exact protocol version.

If rollback is required, stop automated configuration writers, verify the last
successful recovery event, and submit a new reviewed recovery only when the
expected state is known. Do not rely on a stale deployment script that assumes
the previous config is still present.

## Privacy and evidence handling

Reasons are public contract data. Use ticket IDs and concise operational text,
not customer names, wallet secrets, API credentials, or incident payloads. Keep
full incident evidence in the restricted incident system and reference it from
the on-chain reason. Monitoring pipelines should redact any fields beyond the
documented target and reason.

## Ownership boundary

The protocol admin authorizes configuration recovery, but does not automatically
own every invoice, business, treasury, or investor record. A future recovery
entry point must carry an explicit owner or lifecycle proof and must reject a
target that is already settled, canceled, frozen, or otherwise ineligible.

This boundary prevents a generic admin convenience method from becoming a
universal state override. It also keeps incident evidence precise: reviewers can
identify the exact configuration target without inferring which business state
was intended.

When an ownership-aware method is introduced, it should follow the same pattern
as these configuration methods: authenticate first, validate ownership and
lifecycle before writing, emit the target identifier and reason, and rely on the
host transaction for rollback. It should not silently fall back to an admin-only
override when the ownership proof is absent.

Security review should treat any new recovery target as a separate threat-model
entry, with explicit abuse cases for unauthorized callers, stale state, duplicate
requests, and partial external side effects.

This review is required before recovery is exposed through an application or
operator CLI.

The CLI should display the expected snapshot hash and target before requesting
the admin signature. It should require an explicit confirmation that the reason
matches the approved incident record, then print the resulting audit event for
post-deployment verification.
