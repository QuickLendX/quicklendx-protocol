# Cursor Guarantees and Snapshot Generations

Pagination cursors are opaque tokens. Clients must return the exact `next_cursor`
value from a response to request the next page and must not parse, construct, or
modify cursor values.

## Cursor format

The contract currently encodes a cursor as URL-safe base64 over a versioned
payload containing a generation, an offset, and an integrity checksum. This
format is an implementation detail and may change; consumers must treat it as
an opaque string.

Malformed, unsupported-version, or modified cursors are rejected with
`InvalidCursor`. A cursor from a prior generation is rejected with
`UnstableCursor`. The checksum detects accidental or direct payload changes; it
is not an authentication mechanism.

## Snapshot generations

A page response's cursor is bound to the generation of the queried collection.
If a mutation changes that collection between page requests, the contract
rejects the old cursor rather than silently returning a page with gaps or
duplicates. The caller should discard the partial result and restart from the
first page without a cursor.

The cursor generation is scoped to the query and its filters. Cursor-based
pages cover KYC status indexes, business invoices, disputes and dispute
timelines, audit queries, whitelisted currencies, notifications, ranked bids,
investor investments, and payment records. Mutations that change a result set
invalidate its continuation cursor. Some immutable or append-only result sets
derive a generation from their current snapshot instead of storing a counter.

These pagination generations are distinct from the protocol version returned
by `get_version`; see [Generation Counter](GENERATION_COUNTER.md).

Legacy offset-based compatibility entrypoints remain available where existing
callers need them. New integrations should use the `*_paged` methods and pass
back `next_cursor`.

## Example

1. Request the first page with no cursor.
2. Save the returned `next_cursor`.
3. Pass that exact value to fetch the next page.
4. If the contract returns `UnstableCursor`, discard the partial list and
   restart from the first page.

When `next_cursor` is `None` or `has_more` is false, pagination is complete.
