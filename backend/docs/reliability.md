# Reliability & Degraded Mode

This document explains how the QuickLendX backend detects indexer lag, enters
degraded mode, gates write operations, and signals health state to clients.

---

## Overview

The backend indexes on-chain data from the Soroban/Stellar ledger.  When the
indexer falls behind the chain tip, stale data can cause incorrect bid
calculations, double-spend risks, and settlement errors.

The **Degraded Mode** system provides three layers of protection:

1. **Lag Monitor** — continuously computes the gap between the current chain
   tip and the last indexed ledger.
2. **Degraded Guard** — middleware that blocks write/sensitive endpoints when
   lag exceeds configured thresholds.
3. **Status Injector** — response interceptor that appends a `_system` metadata
   block to every JSON response so clients always know the current health state.

---

## Lag Thresholds

Lag is measured in **ledgers**.  Each Stellar ledger closes approximately every
5 seconds.

| Level        | Default threshold | Meaning                                                  |
|--------------|-------------------|----------------------------------------------------------|
| `none`       | lag < 10          | System is healthy; all operations permitted.             |
| `warn`       | 10 ≤ lag < 50     | Indexer is behind; write operations are blocked.         |
| `critical`   | lag ≥ 50          | Indexer is severely behind; all mutating ops blocked.    |

### Threshold calculation

```
lag = current_chain_ledger − last_indexed_ledger
```

- `current_chain_ledger` — fetched from the Soroban RPC (mocked in tests).
- `last_indexed_ledger` — updated by the indexer process after each batch.

At **10 ledgers** (~50 seconds) the system enters `warn` level.  This is
conservative enough to catch transient RPC hiccups before they cause data
integrity issues.

At **50 ledgers** (~4 minutes) the system enters `critical` level.  At this
point the indexed state is too stale to safely process any writes.

### Overriding thresholds

Set environment variables before starting the server:

```bash
LAG_WARN_THRESHOLD=15      # ledgers before warn
LAG_CRITICAL_THRESHOLD=75  # ledgers before critical
```

---

## API Endpoints

### `GET /api/v1/status`

Returns the current lag status.  Never blocked by the degraded guard.

**Response `200`**

```json
{
  "lag": 5,
  "warnThreshold": 10,
  "criticalThreshold": 50,
  "level": "none",
  "isDegraded": false,
  "isCritical": false,
  "checkedAt": "2026-04-23T12:00:00.000Z",
  "_system": { "status": "operational", "degraded": false, "lag": 5, "level": "none" }
}
```

`level` values: `"none"` | `"warn"` | `"critical"`

---

## `_system` Metadata (Response Injection)

Every JSON **object** response from the API includes a `_system` field:

```json
{
  "...": "original response fields unchanged",
  "_system": {
    "status": "operational",
    "degraded": false,
    "lag": 5,
    "level": "none"
  }
}
```

| Field      | Type    | Description                                              |
|------------|---------|----------------------------------------------------------|
| `status`   | string  | `"operational"` \| `"degraded"` \| `"maintenance"`      |
| `degraded` | boolean | `true` when level is `warn` or `critical`                |
| `lag`      | number  | Current indexer lag in ledgers                           |
| `level`    | string  | `"none"` \| `"warn"` \| `"critical"`                    |

**Schema stability guarantee:** `_system` is purely additive.  Existing fields
are never modified or removed.  Clients that do not read `_system` are
completely unaffected.

Array responses (e.g. `GET /invoices`) do **not** have `_system` injected,
preserving their array type.

---

## Feature Gating (Degraded Guard)

Write and sensitive endpoints are protected by `degradedGuard()` middleware.

### Behaviour

| Lag level  | `degradedGuard()` | `degradedGuard({ criticalOnly: true })` |
|------------|-------------------|-----------------------------------------|
| `none`     | ✅ Pass through   | ✅ Pass through                         |
| `warn`     | ❌ 503            | ✅ Pass through                         |
| `critical` | ❌ 503            | ❌ 503                                  |

### Error response (503)

```json
{
  "error": {
    "message": "Service is degraded due to indexer lag. Write operations are temporarily unavailable.",
    "code": "DEGRADED_MODE",
    "details": {
      "lag": 20,
      "warn_threshold": 10,
      "critical_threshold": 50,
      "level": "warn"
    }
  }
}
```

At critical level, `code` is `"DEGRADED_MODE_CRITICAL"`.

### Applying the guard to a route

```typescript
import { degradedGuard } from "../middleware/degraded-guard";

// Block at warn AND critical:
router.post("/bids", authMiddleware, degradedGuard(), bidController.placeBid);

// Block only at critical:
router.post("/settlements", authMiddleware, degradedGuard({ criticalOnly: true }), ...);
```

### Security contract

`degradedGuard` **must** be placed **after** any authentication/authorisation
middleware.  It never bypasses auth — it only adds an availability gate on top
of existing security layers.  The guard does not modify request headers, auth
tokens, or any security-sensitive fields.

---

## Frontend Integration Guide

### Polling `/api/v1/status`

Poll every 30 seconds to get the current lag level:

```typescript
const { level, isDegraded } = await fetch("/api/v1/status").then(r => r.json());

if (isDegraded) {
  showBanner("System is experiencing delays. Some actions may be unavailable.");
}
```

### Reading `_system` from any response

Every API response object includes `_system`.  Use it to update UI state
without a separate status poll:

```typescript
const data = await fetch("/api/v1/invoices/abc").then(r => r.json());

if (data._system?.degraded) {
  disableWriteButtons();
}
```

### Handling 503 responses

When a write endpoint returns 503, display a user-friendly message and offer
a retry:

```typescript
if (response.status === 503) {
  const { error } = await response.json();
  if (error.code === "DEGRADED_MODE" || error.code === "DEGRADED_MODE_CRITICAL") {
    showRetryDialog(
      "The system is temporarily unavailable due to high indexer lag. " +
      "Please try again in a few minutes."
    );
  }
}
```

### Recommended UI states

| `_system.level` | Recommended UI behaviour                                      |
|-----------------|---------------------------------------------------------------|
| `none`          | Normal operation; no banner needed.                           |
| `warn`          | Show a yellow warning banner; disable write buttons.          |
| `critical`      | Show a red error banner; disable all mutating actions.        |
| `maintenance`   | Show a maintenance page; disable all interactions.            |

---

## Architecture Diagram

```
Request
  │
  ▼
rateLimitMiddleware
  │
  ▼
statusInjector          ← wraps res.json() to append _system
  │
  ▼
Router
  │
  ├── GET  /status      ← always accessible, returns LagStatus
  ├── GET  /invoices    ← read-only, no guard
  │
  └── POST /write-action
        │
        ▼
      degradedGuard()   ← checks lagMonitor.getLagStatus()
        │                  503 if warn or critical
        ▼
      controller        ← only reached when healthy
```

---

## Testing

Run the full test suite:

```bash
npm test
```

Key test files:

| File | What it tests |
|------|---------------|
| `src/tests/lagMonitor.test.ts` | Unit tests for lag calculation, threshold validation, env var config |
| `src/tests/degradedGuard.test.ts` | Unit + integration tests for endpoint gating (503/201) |
| `src/tests/statusInjector.test.ts` | Schema stability, `_system` injection, array passthrough |
| `src/tests/ingestion-fault.test.ts` | Ingestion unit-of-work tests with transient/partial commit database failures |
| `src/tests/webhook-delivery-fault.test.ts` | Webhook egress security and stream body byte limit tests with DNS mock |
| `src/tests/rpc-circuit-fault.test.ts` | Stellar RPC client circuit transitions (CLOSED -> OPEN -> HALF_OPEN) and retries |

---

## Fault Injection Harness

To ensure the backend can recover gracefully from transient infra outages (e.g. database errors, partial writes, network resets, and DNS resolution failures), a fault injection harness is available under [faultInjector.ts](file:///c:/Users/USER/OneDrive/Documents/GitHub/quicklendx-protocol/backend/src/tests/helpers/faultInjector.ts).

### Harness Component Wrappers

1. **`FaultyIngestionStore`**: Wraps any `IngestionStore` implementation.
   - `setShouldFailCommit(fail, error)`: Forces `commitBatch` to fail.
   - `setPartialCommitCount(count)`: Simulates "mid-batch" failures by only persisting `count` events while the transaction fails and the cursor remains unadvanced.
   - `setShouldFailGetCursor(fail, error)`: Simulates database read failures.
   - `setShouldFailRollback(fail, error)`: Simulates database rollback failures during reorgs.

2. **`FaultyFetch`**: Temporarily replaces the global `fetch` with a queue of simulated HTTP response statuses or network errors.
   - `queueFailure({ error, status, body })`: Queues a specific failure (e.g. status code 502/503/429 or connection error) for the next call.
   - `restore()`: Reinstates the original global fetch.

3. **`FaultyDns`**: Temporarily replaces `dns.lookup` to return private/blocked addresses or DNS errors.
   - `setup(resolveToIps, shouldFail)`: Customizes what IPs the lookup returns or flags failure.
   - `restore()`: Reinstates the original `dns.lookup` function.

### Example Harness Usage

```typescript
import { FaultyDns } from "./helpers/faultInjector";

const faultyDns = new FaultyDns();
beforeEach(() => {
  // Overrides dns.lookup globally
  faultyDns.setup(["127.0.0.1"]); 
});

afterEach(() => {
  // Essential: cleans up to avoid leaking mock to other tests
  faultyDns.restore(); 
});
```

