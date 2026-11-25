# Cursor-Based Pagination

List endpoints in the QuickLendX backend use opaque, scope-bound cursor pagination implemented in [`backend/src/utils/pagination.ts`](../src/utils/pagination.ts) (`parsePaginationParams`, `applyPagination`, `paginationScope`, `encodeCursor`, `decodeCursor`).

## Query Parameters

| Parameter | Type | Default | Constraints | Description |
| --- | --- | --- | --- | --- |
| `limit` | integer | `20` (`DEFAULT_LIMIT`) | Positive integer (`>= 1`). Values above `100` (`MAX_LIMIT`) are clamped to `100`. | Maximum number of records returned in `data`. |
| `cursor` | string | `null` (first page) | Optional opaque `base64url` token returned by `next_cursor`. Empty string `""` is treated as omitted. | Position marker for fetching the next page. |

- `DEFAULT_LIMIT = 20`
- `MAX_LIMIT = 100`

Passing a non-integer, zero, or negative `limit` (for example `limit=0`, `limit=-5`, or `limit=abc`) is rejected with HTTP `400`.

## Response Envelope

Paginated list endpoints return a JSON object with the following fields:

```json
{
  "data": [],
  "next_cursor": "eyJpZCI6Ii4uLiIsInNvcnRfdmFsIjoxNzY3MjI1NjAwLCJzY29wZSI6IiJ9",
  "has_more": true,
  "freshness": {
    "last_indexed_at": "2026-01-01T00:00:00.000Z",
    "lag_ledgers": 0
  }
}
```

| Field | Type | Description |
| --- | --- | --- |
| `data` | `T[]` | Array of records for the current page (at most `limit` items). |
| `next_cursor` | `string \| null` | Opaque cursor to pass as `?cursor=...` to fetch the next page, or `null` when `has_more` is `false`. |
| `has_more` | `boolean` | `true` if at least one more record exists after this page; `false` on the final page. |
| `freshness` | `object` | Indexer freshness metadata attached by v1 list controllers. |

> **No `total` count:** By design, list responses do not expose a `total` count field, avoiding unbounded count scans and preventing total-count leakage.

## Ordering Guarantees

All paginated collections use a deterministic, two-column sort key:

```text
(sort_field DESC, id ASC)
```

- **Invoices** (`GET /api/v1/invoices`): ordered by `(created_at DESC, id ASC)`.
- **Settlements** (`GET /api/v1/settlements`): ordered by `(timestamp DESC, id ASC)`.

Because the cursor encodes both `sort_val` and `id`, pagination advances with:

```sql
WHERE (sort_field < cursor.sort_val)
   OR (sort_field = cursor.sort_val AND id > cursor.id)
```

This guarantees:
1. **Deterministic tie-breaking:** Records with identical timestamps are ordered stably by `id ASC`.
2. **Stability under concurrent inserts:** Newly inserted records with a newer `sort_field` do not shift or duplicate items on subsequent pages.
3. **Past-the-end safety:** If a cursor points past the last available record, the endpoint returns `{ "data": [], "next_cursor": null, "has_more": false }`.

## Cursor Opacity and Scope Binding

Cursors are `base64url`-encoded JSON payloads of the form `{ id, sort_val, scope? }`. Clients **must treat cursors as opaque strings** and must not construct, parse, or mutate them.

### Scope Mismatch Behaviour

Each paginated endpoint derives a canonical scope string from the active filter query parameters using `paginationScope(filters)`:

- Only defined filter values are included.
- Filter keys are sorted lexicographically and serialized as `encodeURIComponent(key)=encodeURIComponent(value)` joined by `&`.
- An unfiltered request has an empty scope (`""`).

Examples of computed scopes:
- `GET /api/v1/invoices` -> `""`
- `GET /api/v1/invoices?status=Verified` -> `"status=Verified"`
- `GET /api/v1/invoices?status=Verified&business=GBIZ...` -> `"business=GBIZ...&status=Verified"`
- `GET /api/v1/settlements?status=Paid` -> `"status=Paid"`

When a request includes `cursor`, the backend verifies that `cursor.scope === scope`. If a client replays a cursor obtained from one filtered view against a different filter set (or against an unfiltered view), the request fails with HTTP `400` and message `"cursor scope mismatch"`.

## Error Handling (`INVALID_PAGINATION`)

Invalid pagination inputs throw `PaginationError` and return HTTP `400 Bad Request`:

```json
{
  "error": {
    "message": "invalid cursor",
    "code": "INVALID_PAGINATION"
  }
}
```

Common error messages:
- `"limit must be a positive integer"`
- `"cursor must be a string"`
- `"invalid cursor"` (malformed `base64url`, invalid JSON, or missing/invalid `id`, `sort_val`, or `scope` types)
- `"cursor scope mismatch"` (cursor replayed with different filter parameters)

> **Controller note:** `GET /api/v1/invoices` maps all `PaginationError` instances to `INVALID_PAGINATION`. `GET /api/v1/settlements` maps invalid `limit` errors to `VALIDATION_ERROR` and all cursor errors (malformed cursor, scope mismatch) to `INVALID_PAGINATION`.

## Examples

The examples below assume the local backend is running on `http://localhost:3001` with seeded development data (`cd backend && npm run seed`).

### Invoices (`GET /api/v1/invoices`)

#### 1. Fetch the first page (custom `limit`)

```bash
curl -s "http://localhost:3001/api/v1/invoices?limit=2"
```

```json
{
  "data": [
    {
      "id": "0x0000000000000000000000000000000000000000000000000000000000000001",
      "business": "GBUSINESS00000000000000000000000000000000000000000000001",
      "amount": "15000000000",
      "currency": "USDC",
      "due_date": 1769817600,
      "status": "Verified",
      "description": "Q1 Logistics Services",
      "category": "Services",
      "tags": ["logistics"],
      "metadata": {
        "customer_name": "Acme Corp",
        "customer_address": "123 Market St",
        "tax_id": "US-12-3456789",
        "line_items": [],
        "notes": ""
      },
      "created_at": 1767229200,
      "updated_at": 1767229200,
      "contract_version": 1,
      "event_schema_version": 1,
      "indexed_at": "2026-01-01T01:00:00.000Z"
    },
    {
      "id": "0x0000000000000000000000000000000000000000000000000000000000000002",
      "business": "GBUSINESS00000000000000000000000000000000000000000000001",
      "amount": "8000000000",
      "currency": "USDC",
      "due_date": 1769817600,
      "status": "Pending",
      "description": "Cloud Infrastructure",
      "category": "Technology",
      "tags": ["cloud"],
      "metadata": {
        "customer_name": "Beta LLC",
        "customer_address": "456 Main St",
        "tax_id": "US-98-7654321",
        "line_items": [],
        "notes": ""
      },
      "created_at": 1767225600,
      "updated_at": 1767225600,
      "contract_version": 1,
      "event_schema_version": 1,
      "indexed_at": "2026-01-01T00:00:00.000Z"
    }
  ],
  "next_cursor": "eyJpZCI6IjB4MDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMiIsInNvcnRfdmFsIjoxNzY3MjI1NjAwLCJzY29wZSI6IiJ9",
  "has_more": true,
  "freshness": {
    "last_indexed_at": "2026-01-01T01:00:00.000Z",
    "lag_ledgers": 0
  }
}
```

#### 2. Fetch the next page using `next_cursor`

```bash
curl -s "http://localhost:3001/api/v1/invoices?limit=2&cursor=eyJpZCI6IjB4MDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMiIsInNvcnRfdmFsIjoxNzY3MjI1NjAwLCJzY29wZSI6IiJ9"
```

```json
{
  "data": [
    {
      "id": "0x0000000000000000000000000000000000000000000000000000000000000003",
      "business": "GBUSINESS00000000000000000000000000000000000000000000002",
      "amount": "5000000000",
      "currency": "USDC",
      "due_date": 1769817600,
      "status": "Paid",
      "description": "Advisory Retainer",
      "category": "Consulting",
      "tags": ["advisory"],
      "metadata": {
        "customer_name": "Gamma Inc",
        "customer_address": "789 Oak Ave",
        "tax_id": "US-55-1122334",
        "line_items": [],
        "notes": ""
      },
      "created_at": 1767222000,
      "updated_at": 1767222000,
      "contract_version": 1,
      "event_schema_version": 1,
      "indexed_at": "2025-12-31T23:00:00.000Z"
    }
  ],
  "next_cursor": null,
  "has_more": false,
  "freshness": {
    "last_indexed_at": "2026-01-01T01:00:00.000Z",
    "lag_ledgers": 0
  }
}
```

#### 3. Scope mismatch error (replaying an unfiltered cursor with a filter)

```bash
curl -s "http://localhost:3001/api/v1/invoices?status=Verified&cursor=eyJpZCI6IjB4MDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMDAwMiIsInNvcnRfdmFsIjoxNzY3MjI1NjAwLCJzY29wZSI6IiJ9"
```

```json
{
  "error": {
    "message": "cursor scope mismatch",
    "code": "INVALID_PAGINATION"
  }
}
```

### Settlements (`GET /api/v1/settlements`)

#### 1. Fetch a filtered page of settlements

```bash
curl -s "http://localhost:3001/api/v1/settlements?status=Paid&limit=1"
```

```json
{
  "data": [
    {
      "id": "0xsettle001",
      "invoice_id": "0x0000000000000000000000000000000000000000000000000000000000000003",
      "amount": "5000000000",
      "payer": "GPAYER000000000000000000000000000000000000000000000000",
      "recipient": "GRECIP000000000000000000000000000000000000000000000000",
      "timestamp": 1767229200,
      "status": "Paid",
      "contract_version": 1,
      "event_schema_version": 1,
      "indexed_at": "2026-01-01T01:00:00.000Z"
    }
  ],
  "next_cursor": null,
  "has_more": false,
  "freshness": {
    "last_indexed_at": "2026-01-01T01:00:00.000Z",
    "lag_ledgers": 0
  }
}
```

#### 2. Malformed cursor error

```bash
curl -s "http://localhost:3001/api/v1/settlements?cursor=not-valid-base64"
```

```json
{
  "error": {
    "message": "invalid cursor",
    "code": "INVALID_PAGINATION"
  }
}
```

## Source References

- Pagination utility: [`backend/src/utils/pagination.ts`](../src/utils/pagination.ts)
- Invoices controller: [`backend/src/controllers/v1/invoices.ts`](../src/controllers/v1/invoices.ts)
- Settlements controller: [`backend/src/controllers/v1/settlements.ts`](../src/controllers/v1/settlements.ts)
