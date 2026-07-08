# Audit Hash Chain

This document describes the tamper-evidence model implemented in
[`src/audit.rs`](../src/audit.rs). The audit trail is append-only, indexed for
queries, and linked per invoice by storing the previous audit entry hash in each
`AuditLogEntry`.

The design proves local consistency of the stored trail. It does not replace
off-chain retention, event indexing, or an external checkpoint for proving that
an entire historical chain was not replayed or truncated.

## Relevant Signatures

The current implementation exposes these chain primitives:

```rust
impl AuditLogEntry {
    pub const HASH_DOMAIN_TAG: &'static [u8] = b"QLX_AUDIT_CHAIN_V1";

    pub fn genesis_prev_hash(env: &Env) -> BytesN<32>;

    pub fn entry_hash(&self, env: &Env) -> BytesN<32>;

    pub fn validate_integrity(&self, env: &Env) -> Result<bool, QuickLendXError>;
}

impl AuditStorage {
    pub fn last_entry_hash(env: &Env, invoice_id: &BytesN<32>) -> BytesN<32>;

    pub fn verify_audit_chain(env: &Env, invoice_id: &BytesN<32>) -> bool;

    pub fn first_audit_chain_divergence(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> Option<u32>;

    pub fn validate_invoice_audit_integrity(
        env: &Env,
        invoice_id: &BytesN<32>,
    ) -> Result<bool, QuickLendXError>;
}
```

`validate_invoice_audit_integrity` combines each entry's local integrity checks
with `verify_audit_chain`.

## Genesis Hash

Every invoice-local trail begins with the same fixed previous-hash sentinel:

```rust
pub const AUDIT_CHAIN_GENESIS: [u8; 32] = [0u8; 32];
```

`AuditLogEntry::genesis_prev_hash(env)` converts that byte array to
`BytesN<32>`. The first entry for an invoice must have
`prev_hash == genesis_prev_hash(env)`.

The contract also defines:

```rust
pub const CONFIG_AUDIT_SENTINEL: [u8; 32] = [0xCFu8; 32];
```

That value is a virtual `invoice_id` for admin configuration changes. It is not
the genesis hash. Config-change audit entries share that virtual trail key so
protocol, fee, treasury, fee-structure, and revenue-distribution changes can be
verified with the same chain logic.

## Per-Entry Linking

`AuditLogEntry::new` calls:

```rust
let prev_hash = AuditStorage::last_entry_hash(env, &invoice_id);
```

`last_entry_hash` reads the invoice trail:

- If the trail is empty, it returns the genesis hash.
- Otherwise, it loads the last audit entry and returns `entry.entry_hash(env)`.
- If the last stored ID is missing, it falls back to the genesis hash. The later
  verifier detects the missing entry at that trail index.

When `AuditStorage::store_audit_entry` persists the new entry, it appends the
entry ID to the invoice trail and query indexes. It does not rewrite older audit
entries.

The resulting chain for three entries is:

```text
entry[0].prev_hash = genesis
entry[1].prev_hash = hash(entry[0])
entry[2].prev_hash = hash(entry[1])
```

## Hash Preimage

`AuditLogEntry::entry_hash(env)` delegates to private `hash_audit_entry`.

The preimage is serialized in this exact order:

1. `AuditLogEntry::HASH_DOMAIN_TAG` as raw bytes.
2. `entry.prev_hash` encoded with `to_xdr(env)`.
3. `entry.audit_id` encoded with `to_xdr(env)`.
4. `entry.invoice_id` encoded with `to_xdr(env)`.
5. One byte from `operation_tag(&entry.operation)`.
6. `entry.actor` encoded with `to_xdr(env)`.
7. `entry.timestamp` as big-endian bytes.
8. `entry.old_value` via `append_optional_string`.
9. `entry.new_value` via `append_optional_string`.
10. `entry.amount` via `append_optional_amount`.
11. `entry.additional_data` via `append_optional_string`.
12. `entry.block_height` as big-endian bytes.
13. `entry.transaction_hash` via `append_optional_hash`.

Each `append_optional_*` helper writes a one-byte presence marker first:

- `0x00` for `None`.
- `0x01` for `Some(...)`, followed by the encoded value.

String and hash values use Soroban XDR encoding. Amounts use `i128` big-endian
bytes. The domain tag prevents these audit hashes from being confused with other
protocol digests over similar fields.

## Operation Tags

`operation_tag` is private but consensus-relevant for audit-chain hashes. Current
mapping:

| Tag | Operation |
| --- | --- |
| 0 | `InvoiceCreated` |
| 1 | `InvoiceUploaded` |
| 2 | `InvoiceVerified` |
| 3 | `InvoiceFunded` |
| 4 | `InvoicePaid` |
| 5 | `InvoiceDefaulted` |
| 6 | `InvoiceStatusChanged` |
| 7 | `InvoiceRated` |
| 8 | `BidPlaced` |
| 9 | `BidAccepted` |
| 10 | `BidWithdrawn` |
| 11 | `EscrowCreated` |
| 12 | `EscrowReleased` |
| 13 | `EscrowRefunded` |
| 14 | `PaymentProcessed` |
| 15 | `SettlementCompleted` |
| 16 | `ConfigProtocolChanged` |
| 17 | `ConfigFeeChanged` |
| 18 | `ConfigTreasuryChanged` |
| 19 | `ConfigFeeStructureChanged` |
| 20 | `ConfigRevenueDistributionChanged` |

Do not renumber existing tags. A changed tag changes every hash for entries with
that operation.

## Verification

Use the boolean verifier for health checks:

```rust
let ok = AuditStorage::verify_audit_chain(env, &invoice_id);
```

Use the divergence helper when an operator or test needs the first bad index:

```rust
let first_bad = AuditStorage::first_audit_chain_divergence(env, &invoice_id);
```

`first_audit_chain_divergence` walks the invoice trail from index `0` with
`expected_prev = genesis_prev_hash(env)`. For each audit ID it:

1. Loads the audit entry. Missing storage returns `Some(index)`.
2. Compares `entry.prev_hash` to `expected_prev`.
3. Runs `entry.validate_integrity(env)`.
4. Sets `expected_prev = entry.entry_hash(env)` and continues.

Return values:

- `None`: no divergence found.
- `Some(0)`: the first entry is missing, malformed, or not linked to genesis.
- `Some(n)`: entry `n` is the first entry whose stored `prev_hash`, storage
  record, or local integrity predicate does not match expectations.

Edge cases:

- Empty trail: valid; there are no entries to contradict the chain.
- Single-entry trail: valid only when its `prev_hash` is genesis and the entry
  passes `validate_integrity`.
- Missing referenced audit ID: divergence at that referenced index.

## Worked Example

Assume invoice `I` has three audit IDs in order: `A`, `B`, `C`.

Initial healthy state:

```text
A.prev_hash = genesis
B.prev_hash = hash(A)
C.prev_hash = hash(B)
first_audit_chain_divergence(I) = None
verify_audit_chain(I) = true
```

If `A.amount` is edited after `B` and `C` already exist:

```text
hash(A) changes
B.prev_hash still equals the old hash(A)
first_audit_chain_divergence(I) = Some(1)
verify_audit_chain(I) = false
```

If the audit ID for `B` remains in the invoice trail but the entry storage is
deleted:

```text
first_audit_chain_divergence(I) = Some(1)
verify_audit_chain(I) = false
```

If the first entry is rewritten so `A.prev_hash != genesis`:

```text
first_audit_chain_divergence(I) = Some(0)
verify_audit_chain(I) = false
```

## Threat Model

Detected by the current verifier:

- Editing any non-terminal entry field included in `entry_hash`, because the next
  entry's `prev_hash` no longer matches.
- Reordering entries inside an invoice trail, because at least one `prev_hash`
  comparison changes.
- Missing audit entry storage for an ID still present in the invoice trail.
- Broken genesis link at index `0`.
- Locally malformed entries caught by `validate_integrity`, including future
  timestamps, future block heights, missing positive amounts for funding/payment
  operations, and incomplete status-change values.

Not fully detected without an external checkpoint or expected-length source:

- Replaying a whole internally consistent chain.
- Truncating the tail of a trail and deleting the corresponding IDs from the
  trail index.
- Editing the terminal entry when no later entry or external terminal hash is
  available to compare against, unless the edit violates `validate_integrity`.
- Tampering with off-chain exports after they leave contract storage.

For stronger operational assurance, indexers should checkpoint at least the
invoice ID, trail length, and terminal hash after each block or export batch.

## Operator Usage

For an invoice:

```rust
let ok = AuditStorage::verify_audit_chain(env, &invoice_id);
if !ok {
    let first_bad = AuditStorage::first_audit_chain_divergence(env, &invoice_id);
    // Inspect the entry at first_bad and the previous entry if first_bad > 0.
}
```

For admin config changes, use `CONFIG_AUDIT_SENTINEL` as the trail key:

```rust
let config_invoice_id = BytesN::from_array(env, &CONFIG_AUDIT_SENTINEL);
let ok = AuditStorage::verify_audit_chain(env, &config_invoice_id);
```

When exporting audit evidence, include ordered audit IDs, full entries, the
terminal `entry_hash`, and the result of `first_audit_chain_divergence`.
