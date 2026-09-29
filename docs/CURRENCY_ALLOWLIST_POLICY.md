# Versioned payment-token allowlist

Payment and settlement paths call `payment_token_policy::authorize_payment`
before a transfer or a new funded record. Missing assets are unsupported and
known-but-disabled assets are removed. Already-funded records remain readable
after removal so historical settlement accounting is not corrupted.

Only the authorized admin may update configuration. Every update supplies the
expected current version and emits deterministic old/new values. Stale,
future, repeated, and cross-token updates fail closed. Re-adding an asset
creates a new version and cannot replay the old configuration event.

The pure boundary uses a canonical 32-byte token identifier. The Soroban
adapter must convert an address without truncation or string normalization.
Storage should persist token, enabled, version, and admin identity together
with an atomic compare-and-set update.

Safety rules:

- validate token before state writes or transfers;
- validate admin before configuration writes;
- validate expected version before deriving the next version;
- emit old and new values after a successful update;
- keep settled records independent from current allowlist state;
- never accept an arbitrary token from an invoice;
- keep configuration events deterministic across upgrades.

Roll out storage and read-only validation first, then switch payment/funding
entrypoints, and finally enable admin updates. Existing settled records need
no rewrite. Rollback preserves the allowlist table and versions; deleting the
table would make updates replayable and could reopen an unsupported-asset path.

The tests cover missing, supported, removed, re-added, funded, unauthorized,
stale-version, future-version, cross-token, old/new event, overflow, and
canonical-identifier behavior.

## Operational invariants

The following invariants must hold after every successful configuration call:

1. every configured token has a non-zero version;
2. every token address is unique in storage;
3. every update increments exactly one token version;
4. an update cannot skip the expected version;
5. an unauthorized caller cannot change any slot;
6. a removed token is still represented for historical reads;
7. a new payment never accepts a disabled token;
8. a funded record never changes its original token identity;
9. old and new enabled values are present in the emitted event;
10. snapshot counts match the stored configured slots;
11. admin rotation increments the admin version;
12. zero addresses never become administrators or assets.

## Batch configuration

Batch updates validate administrator identity and every token identifier before
applying the first change. A database adapter must execute the complete batch
inside one transaction. If one expected version is stale, the transaction must
roll back all earlier members of the batch. The returned snapshot is suitable
for audit logging and contains counts plus a deterministic digest; it is not a
replacement for the individual configuration events.

## Admin rotation

Admin rotation requires authorization from the current administrator, rejects
self-rotation and zero addresses, and increments the admin version. A
deployment should rotate the admin only after the new signer has been tested.
The old signer must not be accepted after the stored address changes.

## Failure and recovery

If configuration storage fails, no event should be emitted. If event emission
fails after storage, both must roll back. If a transaction outcome is unknown,
retry using the same expected version; the compare-and-set check makes the
retry deterministic. Never retry with an incremented version guessed by the
caller.

## Compatibility

The allowlist is additive to existing invoice records. A removed asset can be
used to read and settle an already-funded record, but cannot fund a new one.
Re-adding the asset creates a new configuration version, allowing auditors to
distinguish the original approval from the later approval.

Configuration changes are auditable through versioned event payloads, and
snapshots provide a deterministic health check for operators.

The policy deliberately separates historical settlement reads from new payment
authorization.

Reviewers should compare the snapshot digest before and after every batch and
retain it with the configuration audit record.

This makes allowlist changes reviewable, replay-safe, and compatible with
already-settled financial records.

The implementation exposes this contract as pure functions so Soroban storage
adapters can enforce the same rules without duplicating business logic.

The caller remains responsible for persisting the returned event atomically.
