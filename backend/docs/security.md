# Backend Security

## Event Ingest Endpoint Hardening

`POST /api/v1/events` is the ingress the indexer uses to submit Soroban events. A request that reaches it is buffered and parsed before any business validation runs, so its framing is validated first, by `src/middleware/event-ingest-limits.ts`. The policy applies exclusively to this route so the rest of the API keeps the default 1 MB budget.

### Framing policy

| Condition | Status | Error code |
| --- | --- | --- |
| Content type is not exactly `application/json` (parameters such as `charset` are allowed) | `415` | `INVALID_CONTENT_TYPE` |
| `Content-Length` absent on a non-chunked request | `411` | `CONTENT_LENGTH_REQUIRED` |
| `Content-Length` is not a single non-negative integer (including duplicated headers) | `400` | `INVALID_CONTENT_LENGTH` |
| Declared or actual body above 256 KB | `413` | `BODY_LIMIT_EXCEEDED` |
| `Transfer-Encoding: chunked` without an allowlisted upstream proxy | `400` | `CHUNKED_ENCODING_NOT_ALLOWED` |
| `Transfer-Encoding: chunked` combined with `Content-Length` | `400` | `AMBIGUOUS_REQUEST_FRAMING` |
| Body bytes do not match the declared `Content-Length` | `400` | `CONTENT_LENGTH_MISMATCH` |
| Body is not parseable JSON | `400` | `INVALID_JSON_BODY` |

### Why the checks are ordered this way

The header guard runs before any body parser, and the application-wide 1 MB `express.json` parser explicitly skips this route (see `isEventIngestRequest` in `src/app.ts`). An oversized or ambiguously framed request is therefore refused without buffering its payload, and the 256 KB budget is bound to the route instead of being inherited from the global parser.

Chunked encoding is evaluated before `Content-Length` because HTTP strips `Content-Length` from chunked requests: checking length first would mask a smuggled request behind a `411`. Seeing both headers at once is the canonical request-smuggling signature, since intermediaries disagree on which one delimits the message, so that combination is rejected outright even for allowlisted proxies.

### Chunked-encoding allowlist

Chunked bodies are refused by default. An upstream proxy that must forward them declares itself with the `X-Allow-Chunked-Encoding` header, and the value has to match an entry in `EVENT_INGEST_CHUNKED_PROXY_ALLOWLIST`, a comma-separated list of proxy identifiers. Presence of the header alone is not sufficient; with the variable unset (the default) every chunked request is rejected.

```bash
EVENT_INGEST_CHUNKED_PROXY_ALLOWLIST=edge-proxy-1,edge-proxy-2
```

Only terminate chunked ingest at a proxy you control, and make sure that proxy strips any client-supplied `X-Allow-Chunked-Encoding` header before forwarding.

### Rejection messages

Every rejection message is a constant. Body-parser failures are re-mapped rather than surfaced, because its native messages quote the offending payload bytes (for example `Unexpected token c ... is not valid JSON`). No response from this endpoint echoes request payload content back to the caller.

## CORS Policy

Browser-facing APIs use an explicit allowlist based on `ALLOWED_ORIGINS`.

- `ALLOWED_ORIGINS` is a comma-separated list of trusted browser origins.
- Requests from untrusted origins are rejected by CORS middleware.
- Preflight (`OPTIONS`) responses return `204` for trusted origins.
- Browser API routes run with `credentials: true` and explicit allowed headers/methods.

This prevents implicit trust of arbitrary origins and ensures only approved web clients can call browser API routes.

## CSRF Strategy

The backend API is token-oriented and does not rely on cookie sessions for browser auth. To keep state-changing endpoints CSRF-safe where applicable:

- State-changing methods (`POST`, `PUT`, `PATCH`, `DELETE`) require `application/json`.
- Requests with unsupported content types are rejected with `415 INVALID_CONTENT_TYPE`.
- If an `Origin` header is present on state-changing requests, it must be in `ALLOWED_ORIGINS`.
- Requests with untrusted origins are rejected with `403 ORIGIN_NOT_ALLOWED`.

This blocks common browser form-based CSRF paths and enforces explicit trusted origin checks.

## Browser vs Webhook Route Separation

The backend exposes webhook callbacks on a separate surface:

- Browser API surface: `/api/v1/*`
- Webhook surface: `/api/webhooks/*`

Webhook routes use dedicated CORS configuration and are not mounted under browser-facing route prefixes.

Current webhook behavior:

- `POST /api/webhooks/callbacks` accepts callbacks (`202`).
- Non-`POST` methods on callback routes return `405 METHOD_NOT_ALLOWED`.

## Security Assumptions and Follow-Ups

- Do not treat CORS as authentication; protected endpoints still require proper authN/authZ.
- Webhook routes should validate HMAC signatures (for example via `X-Webhook-Signature`) before processing payloads.
- Keep `ALLOWED_ORIGINS` minimal and environment-specific (development/staging/production).

## Dependency Policy and SBOM

The backend CI enforces dependency risk checks and software bill-of-materials generation.

- Vulnerability gate: CI runs `npm audit --json` and evaluates the report with `npm run security:scan`.
- Blocking threshold: `high` and `critical` vulnerabilities fail CI by default.
- Failure clarity: the gate prints severity totals and a direct failure reason so remediation is actionable.
- Audit artifact: `backend-audit-report` is uploaded even on failures to support debugging and review.

SBOM requirements:

- Format: CycloneDX JSON (`specVersion: 1.5`).
- Generation: `npm run sbom:generate`.
- Validation: `npm run sbom:check` ensures required SBOM fields are present before upload.
- Artifact: CI uploads `backend-sbom-<ref>` for main/release runs.

Log and secret safety assumptions:

- Security scripts only print aggregate severity counts and structural validation errors.
- Scripts do not echo environment variable values or secrets.
- Do not add tokenized registry URLs or secret-bearing command arguments to CI steps.
