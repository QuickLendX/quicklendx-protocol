# Backend Features: Webhook Versioning, Chain Cursor & Cache

This document covers three backend features implemented in
`quicklendx-frontend/app/lib/`:

---

## #851 – Webhook Payload Versioning & Compatibility Window

### Overview

Every outbound webhook payload is wrapped in a **versioned envelope**.
The current schema version is `v2`. A **compatibility window** (90 days by
default) lets integrators stay on `v1` before being force-upgraded.

### File map

| File | Purpose |
|------|---------|
| `app/lib/webhook/types.ts` | Envelope types, version constants, `ChainCursor`, `SubscriberConfig` |
| `app/lib/webhook/versioning.ts` | `transformEnvelope`, `buildEnvelopeV2`, downgrade chain |
| `app/lib/webhook/dispatcher.ts` | `dispatchEvent`, `registerSubscriber`, HMAC signing |
| `app/lib/webhook/__tests__/versioning.test.ts` | Unit tests |

### Envelope shapes

```typescript
// v2 (canonical)
{
  version: 2,
  delivery_id: "uuid",
  created_at: "2026-04-23T18:00:00.000Z",
  cursor: { ledger_seq: 1234, tx_hash: "0xabc...", event_index: 0 },
  event_type: "invoice.settled",
  payload: { /* contract event fields */ }
}

// v1 (downgraded – cursor hoisted to top level, event_index dropped)
{
  version: 1,
  delivery_id: "uuid",
  created_at: "...",
  event_type: "invoice.settled",
  ledger_seq: 1234,
  tx_hash: "0xabc...",
  payload: { /* ... */ }
}
```

### Per-subscriber version pin

```typescript
registerSubscriber({
  subscriber_id: "acme-corp",
  endpoint_url: "https://acme.example.com/webhooks",
  pinned_version: 1,          // subscriber still on v1
  pin_expires_at: 1785000000, // Unix timestamp; null = never expires
  secret: "s3cr3t",
  event_types: ["invoice.settled", "bid.accepted"],
});
```

Once `pin_expires_at` is in the past, the dispatcher auto-upgrades the
subscriber to `CURRENT_WEBHOOK_VERSION` (when `force_upgrade_expired_pins`
is `true`, which is the default).

### Adding a new schema version

1. Bump `CURRENT_WEBHOOK_VERSION` in `types.ts`.
2. Add a new `WebhookEnvelopeVN` interface.
3. Prepend a downgrader function to `DOWNGRADE_CHAIN` in `versioning.ts`.
4. Extend `WebhookVersion` union type.

---

## #876 – Chain Cursor Model & Monotonic Ingestion

### Overview

The **chain cursor** `{ ledger_seq, tx_hash, event_index }` uniquely
identifies any on-chain event. The `MonotonicIngester` enforces:

- **Strict ordering** – rejects events that would arrive out of order.
- **Gap detection** – halts ingestion and fires `onGap` when events are
  missing.
- **Duplicate skipping** – idempotent; same cursor is silently dropped.
- **Safe resume** – persists the last committed cursor to a `CursorStore`
  and reloads it on restart.

### File map

| File | Purpose |
|------|---------|
| `app/lib/indexer/cursor.ts` | `ChainCursor`, `compareCursors`, `MonotonicIngester`, `CursorStore` |
| `app/lib/indexer/__tests__/cursor.test.ts` | Unit tests |

### Gap comparison rules

| Scenario | Result |
|----------|--------|
| Same ledger, `event_index + 1` | `after` ✅ |
| Next ledger, `event_index = 0` | `after` ✅ |
| Same cursor | `equal` (duplicate) |
| Ledger or index goes backwards | `before` (halt) |
| Index skips within same ledger | `gap` (halt) |
| Ledger jumps > 1 | `gap` (halt) |
| Next ledger, `event_index ≠ 0` | `gap` (halt) |

### Usage

```typescript
const store = new InMemoryCursorStore(); // swap for Redis/DB
const ingester = new MonotonicIngester(store, {
  onAccept: async (cursor) => {
    // process event at `cursor`
  },
  onGap: (current, incoming) => {
    console.error("Gap detected!", { current, incoming });
    // alert ops; do NOT resume until gap is resolved
  },
});

await ingester.resume(); // loads last committed cursor from store

for await (const event of stellarEventStream) {
  const result = await ingester.ingest(event.cursor, event);
  if (result.status === "gap") break; // ingester is now halted
}
```

---

## #877 – Read-Through Cache with Event-Driven Invalidation

### Overview

A typed read-through cache wrapping any `CacheStore` (in-memory by default;
swap for Redis). Key design decisions:

- **Explicit `is_stale` flag** – financial data is NEVER served silently stale.
  Callers must check `is_stale` and display a freshness warning when `true`.
- **`serve_stale = false` by default** – conservative default; stale = miss.
- **Stale-while-revalidate** – opt-in with `serve_stale: true`; triggers a
  background refresh while returning the stale value.
- **Event-driven invalidation** – `invalidateOnEvent` maps contract event types
  to the correct cache eviction strategy.

### File map

| File | Purpose |
|------|---------|
| `app/lib/cache/read-through-cache.ts` | `ReadThroughCache`, `CacheStore`, `invalidateOnEvent` |
| `app/lib/cache/__tests__/read-through-cache.test.ts` | Unit tests |

### Cache key namespacing

| Key helper | Pattern | Evicted by |
|-----------|---------|-----------|
| `invoiceDetailKey(id)` | `invoice:<id>` | Any invoice event |
| `bestBidKey(id)` | `best_bid:<id>` | Any bid or invoice event |

### Usage

```typescript
const cache = new ReadThroughCache(new InMemoryCacheStore(), {
  default_ttl_ms: 30_000, // 30 s
  serve_stale: false,
});

// Read-through
const result = await cache.get(
  invoiceDetailKey(invoiceId),
  () => fetchInvoiceFromChain(invoiceId)
);

if (result.is_stale) {
  // Must show freshness warning on UI for financial data
}

// Invalidation (called by the indexer)
await invalidateOnEvent(cache, "invoice.settled", invoiceId);
```

### Invalidation matrix

| Event type | `invoice:<id>` evicted | `best_bid:<id>` evicted |
|-----------|----------------------|------------------------|
| `invoice.*` | ✅ | ✅ |
| `payment.*` | ✅ | ✅ |
| `dispute.*` | ✅ | ✅ |
| `bid.*` | ❌ | ✅ |
| `escrow.*` | ✅ | ✅ |

---

## Running the tests

```bash
cd quicklendx-frontend
npm ci
npm test
```

Tests are located in `__tests__/` subdirectories within each module folder.
