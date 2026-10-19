# Invoice Tag System

This document describes how invoice tags work in the QuickLendX protocol:
normalisation rules, storage layout, and the threat model behind each
validation constraint.  Audience: **contributors** who need to understand
why tags behave the way they do, and **integrators** who call the
`store_invoice` or `update_invoice_tags` entrypoints.

## Table of Contents

- [Purpose](#purpose)
- [Tag Normalisation](#tag-normalisation)
- [Validation Rules](#validation-rules)
- [Threat Model](#threat-model)
- [Storage Layout](#storage-layout)
- [Future / Reserved Tags](#future--reserved-tags)

## Purpose

Tags are user-supplied short strings attached to an invoice at creation
time.  They serve two roles:

1. **Categorisation** — investors and the protocol use tags for
   cross-invoice filtering, analytics, and risk bucketing.
2. **Searchability** — the `search_invoices` entrypoint can query by
   tag (see [QUERIES.md](QUERIES.md)).

Tags are distinct from `InvoiceCategory` (a fixed enum).  While the
category is a single value that every invoice must carry, tags are a
flexible, multi-valued metadata field.

## Tag Normalisation

Every tag goes through a deterministic normalisation pipeline before it
is stored or compared.  The pipeline is implemented in
`verification.rs::normalize_tag`:

```
Raw input → trim ASCII whitespace → ASCII-lowercase → reject if empty
           → reject if > 50 bytes
```

- **Trimming**: leading and trailing ASCII whitespace (byte values 0x09–0x0D
  and 0x20) are removed.  Internal whitespace is *preserved* (e.g.
  `"quick   lend"` normalises to `"quick   lend"` with the internal spaces
  intact).
- **Lowercasing**: each uppercase ASCII byte (`A`–`Z`) is shifted to its
  lowercase counterpart (`a`–`z`).  Non-ASCII bytes are passed through
  unchanged (tags are byte sequences, not Unicode text).
- **Length enforcement**: the *normalised* form must be at least 1 byte and
  at most `MAX_TAG_LENGTH` (50) bytes.  A tag that is only whitespace
  produces an empty normalised form and is rejected.

### Examples

| Raw input | Normalised form | Valid? |
|---|---|---|
| `"Tech"` | `"tech"` | ✓ |
| `"  TECH  "` | `"tech"` | ✓ |
| `"   "` | *(empty)* | ✗ |
| `"Technology-SaaS-2024-LongName"` (≤50 bytes) | lowercased | ✓ |
| `"A"` × 51 | *(exceeds 50)* | ✗ |

## Validation Rules

### Tag Count

An invoice may carry **up to 10 tags** (`MAX_INVOICE_TAG_COUNT`).
Attempting to store an invoice with 11 or more tags returns
`TagLimitExceeded` (1801).

### Duplicate Detection

After normalisation, all tags for a given invoice must be unique.
`"Tech"` and `"tech"` normalise to the same value and are therefore
duplicates.  Duplicate tags cause the entire `store_invoice` (or
`update_invoice_tags`) call to fail with `InvalidTag` (1800).

### Character Set

Tags are **byte strings**, not UTF-8 strings.  The normalisation
pipeline operates on raw bytes using ASCII rules.  There is no Unicode
case folding or whitespace classification — only ASCII whitespace
(0x09–0x20) is trimmed, and only ASCII uppercase letters (0x41–0x5A)
are lowercased.

This means that:
- Two tags that differ only in non-ASCII case (e.g. `"É"` vs `"é"`) are
  **not** considered duplicates.
- Non-ASCII whitespace (e.g. U+00A0 non-breaking space) is **not**
  trimmed.
- Control characters (bytes 0x00–0x08, 0x0B, 0x0C, 0x0E–0x1F) are
  permitted as long as the total normalised length does not exceed
  50 bytes.

This design is intentional: it keeps the gas cost of normalisation
predictable and avoids pulling in Unicode tables on-chain.

## Threat Model

| Constraint | Threat mitigated |
|---|---|
| Max 10 tags | Prevents storage-bloat attacks where a single invoice carries thousands of tags, inflating storage rent and making `search_invoices` expensive. |
| Max 50 bytes per tag | Prevents individual tags from consuming excessive storage.  A single 50-byte tag costs the same as any other; an unbounded tag could push the per-invoice storage footprint past the Soroban entry TTL limits. |
| Normalisation → duplicate rejection | Without deduplication, the same semantic tag could be stored multiple times (e.g. `"tech"`, `"Tech"`, `"  TECH  "`), inflating storage and producing misleading count-based analytics. |
| ASCII-only normalisation | Avoids non-deterministic or gas-expensive Unicode processing.  Two callers with different locale settings would produce the same normalised output. |
| Leading/trailing whitespace stripped | Prevents visually identical tags from being treated as distinct (e.g. `"tech"` vs `" tech "`). |

## Storage Layout

Tags are stored as part of the `Invoice` struct, which is persisted
under the `InvoiceStorage` key:

```rust
pub struct Invoice {
    // ... other fields ...
    pub tags: Vec<String>,
    // ...
}
```

The entire `Invoice` (including its tags) is stored as a single
instance entry under `DataKey::Invoice(invoice_id)` (see
[STORAGE_LAYOUT.md](STORAGE_LAYOUT.md)).

Because tags are embedded in the invoice record, updating tags requires
a full invoice read-modify-write cycle.  There is no separate tag index;
tag-based queries (`search_invoices` by tag) perform a linear scan over
the active invoice set.

For performance reasons, invoices with many tags are more expensive to
read and write than invoices with few tags.  Keeping the tag count well
below the `MAX_INVOICE_TAG_COUNT` limit is recommended for callers
creating high-volume invoice streams.

## Future / Reserved Tags

As of this writing there are no reserved tag names — any normalised tag
that passes length, count, and duplicate validation is accepted.

Future protocol upgrades may introduce a blocklist of reserved tags
(e.g. tags that overlap with internal system identifiers, or tags that
violate a naming convention).  If such a blocklist is added, the
rejection point will be in `validate_invoice_tags` in `verification.rs`,
returning `InvalidTag` (1800) for any reserved match.

## Related Documents

- [INVOICE_LIFECYCLE.md](INVOICE_LIFECYCLE.md) — the full invoice state machine
- [QUERIES.md](QUERIES.md) — how tags are used in search/filter entrypoints
- [STORAGE_LAYOUT.md](STORAGE_LAYOUT.md) — low-level key layout for invoice records
- [ERROR_CODES.md](ERROR_CODES.md) — tag-related error codes (1800–1801)
- `quicklendx-contracts/src/verification.rs` — `normalize_tag` and `validate_invoice_tags` implementation
