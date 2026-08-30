# Events and audit observability parity

## Design and invariants

Protocol audit records use schema version `1` and carry an `operation_id`. The
id is also the audit storage key, so committed records cannot overwrite an
existing record. The audit hash-chain preimage includes both fields, making a
schema or correlation change detectable by integrity validation.

A record represents the `Committed` phase. Domain validation and authorization
must complete before the state transition and its event/audit records are
considered committed. Soroban transaction atomicity means a trap or returned
failure rolls back domain storage, audit storage, and events together; rejected,
stale, repeated, or failed operations therefore do not leave a committed
observability record.

Within an invoice trail, records are appended in execution order. Consumers
should reconcile by `(schema_version, operation_id)` and then compare the
payload with the final canonical state, rather than infer state from event
arrival order across transactions.

## Compatibility and migration

The existing audit fields and query/index layout remain unchanged; the new
metadata is additive. Consumers that decode the old shape must migrate to the
versioned shape before relying on correlation. Existing records created before
this change are legacy/unversioned and should be treated as non-reconcilable
until a controlled migration or replay has been completed. No storage-key rename
or automatic migration is performed by this change.

Rollback is safe at the deployment level only before records using schema `1`
are produced, or with a reader that understands both schemas. Operators should
pause reconciliation during a mixed-version rollout and resume after all
consumers support version `1`.

## Operational limitations

Soroban event history is externally indexed and is not a replacement for
canonical contract storage. Large audit queries remain bounded by the existing
query limit. Correlation ids identify operations, but they do not encode a
transaction hash because Soroban does not expose one to contract code here.

## Security assumptions

Authorization, validation, and reentrancy guards remain the source of truth for
whether an operation may commit. The envelope does not grant authority. Audit
records intentionally contain identifiers and operational values only; callers
must avoid putting PII into free-form descriptions or reason strings.
