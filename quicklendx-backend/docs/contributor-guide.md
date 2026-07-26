# QuickLendX Backend — Contributor Guide

**Audience:** contributors making changes to `quicklendx-backend/`.  
**Goal:** capture the intent, architecture, and conventions that live in
engineers' heads so you can make a correct change without reading every
commit.

---

## Table of contents

1. [Module layout](#1-module-layout)
2. [Running the service locally](#2-running-the-service-locally)
3. [Configuration and environment variables](#3-configuration-and-environment-variables)
4. [Request path: route → controller → service](#4-request-path-route--controller--service)
5. [Export pipeline end-to-end](#5-export-pipeline-end-to-end)
6. [Audit trail](#6-audit-trail)
7. [Observability: metrics and health](#7-observability-metrics-and-health)
8. [Contract testing](#8-contract-testing)
9. [Adding a new endpoint](#9-adding-a-new-endpoint)
10. [Secret redaction rules](#10-secret-redaction-rules)
11. [Checklist before opening a PR](#11-checklist-before-opening-a-pr)

---

## 1. Module layout

```
quicklendx-backend/
├── src/
│   ├── config/
│   │   ├── schema.ts          ← Zod schema for all env vars
│   │   ├── loader.ts          ← loadConfig() — validates at startup, fail-fast
│   │   ├── masking.ts         ← getSafeConfig(), sensitive-key detection
│   │   └── index.ts           ← re-exports getConfig(), resetConfig()
│   │
│   ├── routes/v1/
│   │   ├── admin.ts           ← AdminRouter — /admin/exports/*
│   │   └── monitoring.ts      ← MonitoringRouter — /metrics, /health
│   │
│   ├── controllers/v1/
│   │   └── exports.ts         ← ExportController — validates + streams export data
│   │
│   ├── services/
│   │   ├── exportService.ts   ← ExportService — formats, streams, checks size
│   │   ├── auditService.ts    ← AuditService — in-memory export audit trail
│   │   └── metricsService.ts  ← MetricsService — Prometheus text format
│   │
│   ├── types/
│   │   └── exports.ts         ← ExportFormat, ExportDataType, ExportRequest, etc.
│   │
│   └── testing/
│       ├── contract-harness.ts   ← createContractHarness() — OpenAPI test helper
│       ├── contract-validator.ts ← ContractValidator — schema validation
│       └── fixtures/             ← per-domain sample payloads for contract tests
│
├── docs/                      ← operator/contributor documentation
├── openapi.yaml               ← API contract (single source of truth)
└── package.json
```

**Three-layer convention:**  
`routes` handle HTTP plumbing (auth, routing, error wrapping) → `controllers`
parse and validate request data → `services` contain the business logic. Tests
target the service and controller layers directly; routes are covered by
contract tests against `openapi.yaml`.

---

## 2. Running the service locally

```bash
cd quicklendx-backend

# Install exact dependency versions
npm ci

# Copy the environment template and fill in values
cp .env.example .env
# Edit .env — see §3 for what each variable does

# Run tests (no database required)
npm test

# Watch mode during development
npm run test:watch

# Coverage report
npm run test:coverage

# Start the dev server
npm run dev

# Type-check without compiling
npx tsc --noEmit

# Lint
npm run lint
```

---

## 3. Configuration and environment variables

All environment variables are declared in `src/config/schema.ts` as a Zod
schema. The application **cannot start** if any required variable is missing or
has the wrong type — validation runs before any other code.

| Variable | Required | Dev default | Production rules |
|----------|----------|-------------|-----------------|
| `NODE_ENV` | No | `development` | — |
| `PORT` | No | `3000` | — |
| `DATABASE_URL` | Yes | — | Must be PostgreSQL (`postgresql://`) |
| `JWT_SECRET` | Yes | — | Min 64 chars (32 in dev) |
| `API_KEY` | Yes | — | Min 32 chars (16 in dev) |
| `ENCRYPTION_KEY` | Yes | — | Min 64 chars (32 in dev) |
| `STELLAR_NETWORK_URL` | Yes | — | Horizon endpoint |
| `STELLAR_NETWORK_PASSPHRASE` | Yes | — | Network passphrase |
| `ENABLE_RATE_LIMITING` | No | `true` | — |
| `MAX_REQUESTS_PER_MINUTE` | No | `100` | — |
| `SENTRY_DSN` | No | — | Error tracking DSN |

**Loading config in code:**

```typescript
import { getConfig } from './config';

const config = getConfig(); // Throws on first call if validation fails
console.log(`Listening on port ${config.PORT}`);
```

**Logging config safely (no secret leakage):**

```typescript
import { getSafeConfig } from './config';

const safeConfig = getSafeConfig(getConfig());
console.log('Startup config:', safeConfig);
// { PORT: 3000, JWT_SECRET: '[REDACTED]', API_KEY: '[REDACTED]', ... }
```

Never log `getConfig()` directly. Keys containing `secret`, `password`, `token`,
`key`, `auth`, `credential`, or `private` are automatically redacted by
`getSafeConfig()`. See §10 for the full redaction rules.

---

## 4. Request path: route → controller → service

### Admin export routes (`src/routes/v1/admin.ts`)

```
GET /admin/exports/audit   → ExportController.exportAuditLog()
GET /admin/exports/stats   → ExportController.getExportStats()
```

`AdminRouter` registers routes on construction and dispatches them via
`handleRequest(method, path, req)`. In the current implementation the router
is a standalone registry (not mounted to an Express app); integration with the
HTTP framework is done by the caller.

### Monitoring routes (`src/routes/v1/monitoring.ts`)

```
GET /metrics    → MetricsService.serializePrometheus()   (auth required)
GET /health     → { status: 'ok', timestamp: '...' }     (no auth)
```

Authentication on `/metrics` is bearer-token based. The route checks for
`Authorization: Bearer <api-key>` before calling the metrics aggregator. If
auth is disabled (test mode), `MonitoringRouter(false)` skips the check.

### Data flow

```
HTTP request
    │
    ▼
Route (auth check, path match)
    │
    ▼
Controller (parse query params, call validateExportRequest)
    │  ← 400 if validation fails
    ▼
Service (format data, stream chunks, update audit)
    │
    ▼
AsyncGenerator<chunk> → HTTP response stream
    │
    └── AuditService.recordExportAudit / updateExportAuditStatus (side-effect)
```

---

## 5. Export pipeline end-to-end

**Supported data types:** `invoices`, `bids`, `settlements`, `disputes`, `audit`  
**Supported formats:** `ndjson` (default), `json`, `csv`

### Default limits

| Limit | Value | Enforced by |
|-------|-------|-------------|
| Max rows per request | 10 000 | `ExportService.validateExportRequest` |
| Max bytes per request | 50 MB | `ExportService.streamExport` |
| Max date range | 90 days | `ExportService.validateExportRequest` |
| Chunk size (rows buffered) | 1 000 | `ExportService.config.chunkSize` |

These defaults live in `DEFAULT_CONFIG` in `src/services/exportService.ts` and
can be overridden by passing a `Partial<ExportConfig>` to the `ExportService`
constructor.

### Streaming with back-pressure

`ExportService.streamExport()` is an `AsyncGenerator`. It yields one chunk per
`chunkSize` rows, with a `setImmediate` yield between chunks to allow the event
loop to drain the socket buffer (back-pressure). The caller (`ExportController`)
wraps the generator and appends a `# Content-Digest: sha256=<hash>` trailer
after the last chunk for integrity verification.

**Concrete example:**

```typescript
// Controller side (simplified)
const generator = exportService.streamExport({
  id: crypto.randomUUID(),
  userId: 'user-123',
  dataType: 'invoices',
  format: 'ndjson',
  startDate: new Date('2025-01-01'),
  endDate: new Date('2025-03-31'),
  limit: 5000,
  timestamp: new Date(),
});

// The response body is the async generator itself;
// the HTTP framework streams chunks to the client as they are yielded.
for await (const chunk of generator) {
  if (chunk.data) {
    socket.write(chunk.data);
  }
  // chunk.checksum is updated on every yield — final value is the full-export hash
}
```

### Integrity verification

Each chunk carries a cumulative `checksum` field — a rolling SHA-256 hash over
all data yielded so far. The final chunk's `checksum` is the integrity digest
for the entire export. Clients can verify the download by computing
`sha256(concatenated_chunks)` and comparing it to the `Content-Digest` trailer.

---

## 6. Audit trail

Every export operation is recorded in `AuditService` (currently an in-memory
`Map<string, ExportAuditEntry>`; in production this would be a database table).

### Audit entry lifecycle

```
ExportController.handleExportStream()
    │
    ├─ AuditService.recordExportAudit(..., status='in-progress')
    │   → returns ExportAuditEntry with a new UUID
    │
    ├─ [streaming chunks]
    │
    └─ AuditService.updateExportAuditStatus(id, 'completed', rows, bytes)
       OR
       AuditService.updateExportAuditStatus(id, 'failed', rows, bytes, errorMsg)
```

### Querying the audit trail

```typescript
import AuditService from './services/auditService';

// All exports by a user (newest first, capped at 100)
const history = AuditService.getUserExportHistory('user-123');

// Exports in a date range
const recent = AuditService.getAuditsByDateRange(
  new Date('2025-07-01'),
  new Date('2025-07-23')
);

// Platform-wide stats
const stats = AuditService.getExportStatistics();
// { totalExports, totalBytesExported, totalRowsExported, successfulExports, failedExports, byDataType }

// Per-user stats
const userStats = AuditService.getExportStatistics('user-123');
```

### Admin endpoint

`GET /admin/exports/audit` calls `ExportController.exportAuditLog()`, which
streams the `audit` data type through the same pipeline as any other export.

`GET /admin/exports/stats` is a placeholder that returns a 200 with a note to
integrate `AuditService.getExportStatistics()`. Implement this when wiring a
real user context.

---

## 7. Observability: metrics and health

### Endpoints

| Endpoint | Auth | Content-Type | Purpose |
|----------|------|-------------|---------|
| `GET /v1/metrics` | Bearer token | `text/plain; version=0.0.4` | Prometheus scrape |
| `GET /v1/health` | None | `application/json` | Load-balancer probe |

### Available metrics

All metrics use the `qlx_` prefix.

| Metric name | Type | What it means | Action threshold |
|-------------|------|---------------|-----------------|
| `qlx_ingest_lag_ledgers` | Gauge | Ledgers behind Stellar tip | Alert if > 100 for 5 min |
| `qlx_webhook_queue_depth` | Gauge | Webhooks waiting for delivery | Alert if > 1 000 for 2 min |
| `qlx_webhook_overflow_total` | Counter | Queue overflows since startup | Alert if `increase[5m] > 0` |
| `qlx_rpc_circuit_state` | Gauge | 0=closed, 1=open, 2=half-open | Alert immediately if = 1 |
| `qlx_invariant_violations_total` | Counter | Contract invariant breaches | Alert if `increase[5m] > 0` |

### Updating metrics from services

`MetricsService.aggregateMetrics(services)` accepts an object whose optional
fields each implement a small interface. Pass only the services that are running:

```typescript
import { metricsService } from './services/metricsService';

await metricsService.aggregateMetrics({
  lagMonitor: {
    getLagLedgers: async () => stellarIndexer.currentLag(),
  },
  webhookQueue: {
    getDepth:         async () => queue.size(),
    getOverflowCount: async () => queue.overflowsSinceStart(),
  },
  invariantService: {
    getViolationCount: async () => invariantChecker.totalViolations(),
  },
  rpcClient: {
    getCircuitState: async () => rpc.circuitState(), // 0 | 1 | 2
  },
});
```

If any individual service call throws, `aggregateMetrics` logs a warning and
continues. The `/metrics` endpoint will serve the last known value for that
metric rather than returning 500.

### Adding a new metric

1. Call `metricsService.updateMetric(name, value)` with a name that has not
   been registered yet — this logs a warning and does nothing. You must first
   register the metric in `MetricsService.initializeDefaultMetrics()`:

```typescript
// src/services/metricsService.ts — initializeDefaultMetrics()
this.metrics.set('qlx_settlement_queue_depth', {
  name: 'qlx_settlement_queue_depth',
  type: 'gauge',
  value: 0,
  help: 'Pending settlements awaiting on-chain confirmation',
});
```

2. Add a corresponding service interface to `aggregateMetrics` parameters.
3. Add a PromQL example and an alert rule to `docs/observability.md`.

---

## 8. Contract testing

Every API response is validated against `openapi.yaml`. The contract harness
wraps an OpenAPI validator and is used in both unit tests and integration tests.

### How it works

```typescript
import { createContractHarness } from './testing';

const harness = createContractHarness({
  specPath: './openapi.yaml',
  failFast: true,
});

const result = harness.testResponse(
  'GET',
  '/admin/exports/audit',
  200,
  actualResponseBody
);

if (!result.passed) {
  console.error('Contract violation:', result.validation.errors);
}
```

### Fixtures

Sample request/response payloads live in `src/testing/fixtures/`. Each domain
has its own fixture file:

| File | Domain |
|------|--------|
| `auth.fixtures.ts` | Login, token refresh |
| `invoice.fixtures.ts` | Invoice CRUD |
| `bid.fixtures.ts` | Bid placement, status |
| `user.fixtures.ts` | User profile |
| `system.fixtures.ts` | Health, metrics |

When you add a new endpoint, add a fixture file (or extend the relevant
existing one) with a `valid*` export for success and an `*Error` export for
each distinct error response.

### Running contract tests

```bash
npm test src/testing/__tests__/contract-validator.test.ts
npm test src/testing/__tests__/contract-harness.test.ts

# Or all tests
npm test
```

---

## 9. Adding a new endpoint

Follow this checklist in order to keep the three-layer invariant intact.

### Step 1 — Define the type

Add request and response shapes to `src/types/` (or reuse an existing type
from `src/types/exports.ts`).

### Step 2 — Add the service method

Write business logic in `src/services/`. Services must not import from
`routes` or `controllers`. Keep them framework-agnostic so they can be
tested without an HTTP context.

```typescript
// src/services/settlementService.ts (example)
export class SettlementService {
  async getPendingSettlements(userId: string): Promise<SettlementRecord[]> {
    // query logic here
  }
}
```

### Step 3 — Add the controller method

```typescript
// src/controllers/v1/settlements.ts (example)
export class SettlementController {
  constructor(private service: SettlementService = new SettlementService()) {}

  async listPending(req: HttpRequest): Promise<HttpResponse> {
    const records = await this.service.getPendingSettlements(req.userId);
    return {
      statusCode: 200,
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ data: records }),
    };
  }
}
```

### Step 4 — Register the route

```typescript
// src/routes/v1/settlements.ts (example)
export class SettlementRouter {
  private routes: Route[] = [];
  private controller: SettlementController;

  constructor(service?: SettlementService) {
    this.controller = new SettlementController(service);
    this.routes.push({
      method: 'GET',
      path: '/settlements/pending',
      handler: (req) => this.controller.listPending(req),
    });
  }
}
```

### Step 5 — Update `openapi.yaml`

Add the new path, request schema, and all response schemas. This is the
contract that the frontend team consumes; treat it as the source of truth.

### Step 6 — Add fixtures

Add a `validSettlements` fixture and an error fixture to
`src/testing/fixtures/`.

### Step 7 — Write tests

- **Service tests**: test business logic in isolation.
- **Controller tests**: test parameter parsing and error wrapping.
- **Contract tests**: use the harness to validate response shapes.

```bash
npm test
npm run test:coverage  # must stay above 95%
```

### Step 8 — Update docs

Add the new endpoint to the table in this file (§4) and add any new metrics
or audit entries to §6 / §7.

---

## 10. Secret redaction rules

`getSafeConfig()` in `src/config/masking.ts` scans every key in a config
object and replaces the value with `'[REDACTED]'` if the **key name** (
case-insensitive) contains any of:

`password`, `secret`, `token`, `key`, `auth`, `credential`, `private`,
`api_key`, `api-key`

This applies recursively to nested objects. Numbers, booleans, and arrays are
redacted only if their parent key matches.

**Rule:** never log a raw config object. Always call `getSafeConfig(config)`
first. If a new sensitive variable is added that doesn't match the patterns
above, add the pattern to `masking.ts` and update the pattern list here.

---

## 11. Checklist before opening a PR

```
□  npm test                             # all tests pass
□  npm run test:coverage                # coverage stays ≥ 95%
□  npx tsc --noEmit                     # no TypeScript errors
□  npm run lint                         # no ESLint violations
□  openapi.yaml updated (if new routes) # contract is the source of truth
□  Fixtures added or updated            # for every new response shape
□  getSafeConfig() used for all logging # no secret leakage
□  Service is framework-agnostic        # no HTTP imports in services
□  PR description contains: Closes #<issue-number>
```

---

## Related documents

- [`docs/configuration.md`](configuration.md) — all environment variables with validation rules
- [`docs/exports.md`](exports.md) — export limits, formats, and integrity digest
- [`docs/observability.md`](observability.md) — Prometheus metrics, Grafana queries, alert rules
- [`docs/testing.md`](testing.md) — contract testing, fixtures, and coverage requirements
- [`openapi.yaml`](../openapi.yaml) — authoritative API schema
- [`../../README.md`](../../README.md) — monorepo overview
