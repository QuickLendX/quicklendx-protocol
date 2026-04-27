# Backend Browser Security

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
