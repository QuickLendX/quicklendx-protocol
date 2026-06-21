# Audit hash chain

`AuditLogEntry` now stores `prev_hash`, an invoice-local link to the previous
audit entry. The first entry for an invoice uses a fixed 32-byte zero sentinel.
Every later entry stores the domain-separated SHA-256 hash of the previous
entry's fields.

## Domain separation

Audit link hashes prepend the `QLX_AUDIT_CHAIN_V1` domain tag before serializing
entry fields. This prevents an audit-link digest from being confused with other
protocol hashes that may contain similar IDs, addresses, amounts, or timestamps.

## Verification

Use `verify_audit_chain(env, invoice_id)` to return a boolean for healthy versus
divergent chains. Use `first_audit_chain_divergence(env, invoice_id)` for an
admin/debug tool that returns the zero-based first divergent entry index.

The verifier detects:

- missing entries referenced from an invoice audit trail;
- tampering with an entry that changes the next entry's expected `prev_hash`;
- malformed entries that fail the existing integrity predicate; and
- broken genesis links on the first entry.

## Edge cases

- **Empty chain**: valid; there is no evidence to verify and no divergence.
- **Single entry**: valid when `prev_hash` equals the fixed genesis sentinel.
- **Tampered middle**: invalid at the first successor whose stored `prev_hash` no
  longer matches the recomputed hash of the tampered predecessor.

## Security note

The chain provides tamper evidence, not proof of who performed tampering. Evidence
quality depends on retaining historical entries and comparing the verifier result
against a trusted invoice ID. Any storage mutation, reorder, or deletion in the
middle of the trail becomes detectable by re-running the verifier.
