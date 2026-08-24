# Dispute evidence identity and lifecycle

Dispute evidence is part of the dispute record and must not be treated as an
unbounded replaceable string. The contract now reserves a content-addressed
identity for every accepted evidence payload and binds that identity to the
invoice where it was submitted.

## Identity model

The evidence identity is the SHA-256 digest of the exact Soroban string bytes.
The digest is stored under `DataKey::DisputeEvidence` with the owning invoice ID
as its value. This creates a global uniqueness boundary:

- a duplicate payload on the same invoice is rejected;
- the same payload on another invoice is rejected;
- different payloads receive different identities; and
- the creator and invoice remain visible in the evidence event.

The payload itself remains in the dispute record for compatibility. The digest
is the stable identifier for audit and replay checks. Applications should retain
the digest alongside the upload metadata and use it when correlating an event,
invoice, and external attachment.

## Authorization and state

Evidence acceptance is downstream of the existing authorization checks. Creating
a dispute requires the creator to authorize and to be the business or investor
eligible for that invoice. Updating evidence requires the original dispute
creator to authorize and requires the dispute to remain in `Disputed` state.

Once an admin moves a dispute to `UnderReview`, evidence updates are rejected.
Once a dispute is `Resolved`, the terminal state rejects all further evidence
mutation. This preserves the evidence that was available to the reviewer and
prevents post-resolution rewriting.

Validation and state checks run before the content identity is reserved. An
unauthorized, cross-invoice, invalid-state, or oversized request therefore does
not consume an evidence identity or emit an evidence event.

## Bounded metadata

The existing evidence validator enforces the protocol maximum before reservation.
The payload is a bounded `String`; arbitrary maps, binary blobs, and unbounded
attachments are not accepted by this entry point. External attachments should be
stored in an approved content-addressed service and represented on-chain by a
bounded reference whose digest is included in the evidence string.

Do not put secrets, API keys, private documents, or raw credentials into on-chain
evidence. A digest does not make sensitive plaintext safe. The operational record
should store only the minimum reference needed to retrieve evidence under its
access policy.

## Event correlation

Each accepted evidence reservation emits one `evidence` event carrying:

```text
(invoice_id, creator, evidence_digest)
```

The event is emitted after all authorization, lifecycle, and validation checks
and during the same invocation as the persistent reservation. Indexers should
join the event to the dispute-created or dispute-updated lifecycle event using
the invoice ID and ledger transaction.

The event is not emitted for duplicate, unauthorized, oversized, cross-invoice,
or post-resolution submissions. A failed transaction cannot leave a reservation
or an orphan event because Soroban rolls back the invocation.

## Replay behavior

An exact retry uses the same payload digest. The persistent key is already present
and the retry returns `InvalidDisputeEvidence` rather than replacing the payload
or creating a second event. Clients should treat this as a deterministic replay
rejection and reconcile using the original evidence event.

Changing one byte changes the digest. A changed payload is not a safe retry; it
is a new evidence item and must pass the open-dispute creator authorization and
state checks. The original evidence remains reserved and cannot be reused on a
different invoice.

## Lifecycle matrix

| Dispute state | Original creator | Other participant | Admin/reviewer |
| --- | --- | --- | --- |
| None | May create if eligible | May create if eligible | Cannot create without eligibility |
| Disputed | May submit a new unique payload | Rejected for update | May move to review |
| UnderReview | Rejected for evidence update | Rejected | May resolve according to arbiter rules |
| Resolved | Rejected | Rejected | Rejected for evidence mutation |

The create path still rejects a second dispute on the same invoice. The evidence
identity layer adds protection for reusing one attachment across unrelated
invoices and for submitting the same attachment repeatedly under a new client
request.

## Failure behavior

The evidence path fails closed for:

- missing invoice;
- ineligible creator;
- missing creator authorization;
- existing dispute on create;
- invalid or oversized payload;
- non-`Disputed` update state;
- creator mismatch on update; or
- an already-reserved content identity.

All failure paths preserve the invoice record, evidence reservation map, dispute
timeline, and event stream. The caller should read the current dispute details
after a failure rather than assuming a partial update occurred.

## Storage and retention

Evidence reservations use persistent storage and the repository's standard TTL
extension. The reservation is correctness state: it must outlive ordinary client
retry windows and must not be removed merely because the external attachment has
expired from a cache. A retention migration must preserve the digest-to-invoice
relationship or explicitly archive it into an immutable evidence ledger.

Deleting reservation keys while retaining dispute records would reopen the replay
path. Deleting them while retaining external attachments could also allow the
same evidence to be attached to another invoice. Any archival process must copy
the digest, invoice ID, creator, event ledger, and retention decision together.

## Cross-invoice protection

The digest map is global to the contract, not scoped only by invoice. This is
intentional. If a payload is a shared public document that genuinely belongs to
multiple invoices, clients should submit distinct bounded references or a
reviewed composite reference instead of bypassing the uniqueness invariant.

The contract does not infer ownership from a filename or off-chain URL. The
invoice binding is the on-chain owner boundary; external systems must enforce
their own tenant and access controls for the attachment contents.

## Migration and compatibility

Existing `Dispute` storage encoding is unchanged. The new `DisputeEvidence`
storage variant is additive, and existing dispute reads continue to return the
original evidence string. Historical evidence created before this change has no
reservation entry; a migration may backfill only when the historical payload and
invoice binding are authoritative.

Backfill must be idempotent by digest and must quarantine collisions rather than
choosing an owner silently. If two historical invoices contain the same payload,
the migration should create reviewed external references or record an explicit
legacy exception. It must not overwrite a reservation to make the migration
green.

## Rollback

Rolling back the application code must not delete `DisputeEvidence` entries.
Preserve the storage key variant and reservations until all clients stop using
the new evidence path. If a rollback removes the reservation check while keeping
new events, operators will see duplicate evidence events without a way to
reconstruct the original identity.

If a bug is discovered, pause evidence updates, preserve the event and storage
snapshot, and release a forward fix. Do not repair by replacing the evidence
string in a resolved dispute. Any recovery should use a new audited process that
references the original digest and incident reason.

## Test matrix

The focused evidence tests cover:

- one reservation per payload;
- duplicate replay on the same invoice;
- cross-invoice reuse rejection;
- independent identities for different payloads;
- unauthorized creator rejection through the existing lifecycle guards;
- duplicate, oversized, and invalid-state rejection;
- post-resolution immutability; and
- stable event correlation by invoice and digest.

Full dispute lifecycle tests remain responsible for creator eligibility,
under-review transitions, arbiter authorization, resolution, escrow interaction,
and terminal-state behavior. The new identity tests complement those checks and
do not remove or disable existing coverage.

## Review checklist

- [ ] Evidence is bounded before hashing and storage.
- [ ] Identity is content-addressed and globally reserved.
- [ ] Invoice ID is the reservation owner.
- [ ] Creator authorization and dispute state are checked first.
- [ ] Resolved disputes cannot mutate evidence.
- [ ] Duplicate and cross-invoice reuse are rejected.
- [ ] Events include invoice, creator, and digest.
- [ ] Reservation and event are atomic with the lifecycle call.
- [ ] TTL and retention preserve replay correctness.
- [ ] Migration collisions are quarantined.
- [ ] Rollback preserves reservations.

## Submission workflow

The client should construct an evidence reference once, keep the exact bytes,
and submit the same value when retrying an ambiguous transaction. It should
store the returned transaction ID and digest together. A client that normalizes,
trims, or re-encodes the payload before retrying may create a new identity and
must obtain a new review decision.

Before submission, the client should confirm the invoice ID, dispute creator,
current dispute status, payload length, and attachment reference. After
finalization, it should read dispute details and compare the on-chain evidence
with the local digest. A mismatch is an incident, not a reason to overwrite the
dispute record.

## Indexer behavior

Indexers should treat the evidence digest as an immutable correlation key. They
should index invoice ID, creator, digest, ledger sequence, and transaction hash.
They should not deduplicate only by invoice ID because one open dispute may have
multiple distinct evidence submissions over time.

When an evidence event is observed twice during ledger replay, the indexer may
deduplicate by transaction hash and digest. If two different transactions carry
the same digest, the second should be marked as a rejected or anomalous event
according to the operation result; it must not be displayed as a successful
second attachment.

Indexers must preserve event ordering relative to dispute creation, review, and
resolution. A digest observed after resolution should trigger a consistency
alert because the contract rejects post-resolution mutation.

## Attachment services

Off-chain attachment storage should use the same digest as its object identity
or store a separately documented mapping. The service must bind access to the
invoice tenant and dispute participants, enforce malware/content policy, and
retain deletion evidence. An off-chain object URL alone is not an on-chain
ownership proof.

If an object is replaced, the replacement receives a new digest and must be
submitted while the dispute is still `Disputed`. Replacing the object behind an
unchanged URL is prohibited because it would make historical evidence mutable.

## Operational alerts

Alert on repeated invalid-evidence responses for one actor or invoice, attempts
to reuse a digest across invoices, submissions after `UnderReview` or `Resolved`,
unexpected creators, and evidence events without a matching dispute lifecycle
record. Include digest and invoice ID in the alert, but do not include plaintext
attachment contents.

The on-call responder should first query dispute status and event history, then
compare the digest-to-invoice reservation. Do not clear the reservation as a
quick fix. If the event history and storage disagree, preserve both snapshots
and escalate to the contract owner.

## Governance review

Any future change to evidence size, digest algorithm, storage key encoding, or
allowed lifecycle state is a protocol change. It requires an ABI review, indexer
migration plan, collision analysis, and a rollback plan. Changing the digest
algorithm without a versioned key would make old reservations unreadable or
allow old and new identities to coexist unexpectedly.

If a stronger hash is required, introduce a versioned evidence identity type and
retain the original digest in the event. Do not silently recompute historical
digests during an unrelated deployment.

## Threat model notes

The identity reservation mitigates accidental retry, cross-invoice association,
and post-resolution replacement. It does not prove that the external document
is truthful, that a creator is acting in the user's best interest, or that an
off-chain storage service is available. Those concerns remain in the dispute
review and attachment-access controls.

A compromised eligible creator can still submit a new unique payload while the
dispute is open. Reviewers should therefore treat creator authorization as
necessary but not sufficient evidence quality. The contract's responsibility is
to preserve who submitted what, for which invoice, and at what lifecycle point.

## Release verification

Before release, reviewers should create two disposable invoices and exercise the
full lifecycle: create a dispute with one payload, retry it, submit a different
payload, move the dispute under review, resolve it, and attempt another update.
The first payload must remain associated with its invoice, the retry must fail,
the second payload must receive a different digest, and the post-resolution
attempt must fail without a new event.

The same payload should then be attempted on the second invoice. It must fail at
the global reservation boundary even when the second actor is authorized for the
second invoice. This confirms that authorization and content ownership are two
different checks rather than interchangeable protections.

The release record should include the digest algorithm, maximum evidence size,
storage-key encoding, event topic, and the result of the collision test. Any
change to one of these values needs a new migration and indexer review.

## Recovery constraints

If a client loses the evidence transaction response, it must query the invoice
and event stream before retrying. A failed lookup must not be “repaired” by
submitting a new payload under a new reference because that could create a
second evidence item. Operators should use the original payload when the
operation outcome is ambiguous and retain the resulting error or duplicate
response in the incident record.

If an external attachment is unavailable, preserve its digest and on-chain
record while the storage team restores the object. The contract record is not a
permission to replace the object silently. A replacement must be new evidence,
newly reviewed, and accepted only while the dispute remains open.

## Client integration contract

Clients should calculate the evidence digest locally only for display and
correlation. The contract remains the source of truth for acceptance, and a
client must not infer success from a locally calculated digest. The receipt,
transaction result, and emitted event are the authoritative result of a write.

Before submitting, a client should load the current dispute state and confirm
that the caller is still an allowed participant. This check improves user
feedback but does not replace the contract's authorization check, since state
can change between simulation and submission.

Clients should keep the original bytes available until the transaction is
finalized. Converting a document to a different serialization, changing line
endings, or normalizing metadata changes the digest and is a new evidence
submission. The UI should make that distinction explicit instead of presenting
the operation as an ordinary retry.

For a duplicate response, clients should show the existing evidence identity
and invoice correlation when available. They should not automatically retry a
duplicate with a random identifier, timestamp, or regenerated metadata. Such
a retry would defeat the protection by making the payload different.

## Review checklist

Reviewers verifying an evidence change should answer each of these questions:

1. Is the caller authorized for the target dispute at the time of the write?
2. Is the target dispute in a state that permits evidence mutation?
3. Is the evidence within the configured byte limit before hashing and storage?
4. Is the content digest calculated from exactly the submitted byte sequence?
5. Is the digest reservation global across invoice and dispute identifiers?
6. Does a duplicate reservation fail without changing the existing record?
7. Does a failed lifecycle update roll back the reservation and event together?
8. Does resolution prevent both replacement and additional evidence writes?
9. Can an indexer correlate the event to one invoice and one creator?
10. Are logs free of plaintext evidence and sensitive attachment contents?

The checklist should be recorded with release evidence, including the ledger
network, contract version, test transaction identifiers, and configured size
limit. A passing unit test alone does not demonstrate that an indexer or
attachment service preserves the same identity semantics.

## Compatibility and migration

Existing dispute records without an evidence reservation remain valid. A
migration must never invent a digest from a URL, database identifier, or
truncated content because those values do not necessarily represent the bytes
that were originally submitted. If legacy records need indexing, mark their
identity as unavailable and require a new reviewed submission for future
updates.

Any client or indexer that consumes the new event should tolerate unknown
future digest versions. It must preserve the version marker and raw digest
without attempting to reinterpret it as the current algorithm. This keeps
historical records queryable if the protocol later adopts a new hash scheme.

The compatibility boundary is deliberately narrow: old records may be read,
but they may not bypass current authorization, size, lifecycle, or replay
checks. A migration that weakens any of those checks requires a separate
security review and an explicit governance decision.
