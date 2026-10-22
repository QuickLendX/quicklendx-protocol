# Data exports

The backend provides an authenticated export workflow under the v1 API:

- `POST /api/v1/exports/generate` creates a short-lived signed download link.
  The supported `format` query values are `json` and `csv`.
- `GET /api/v1/exports/download/:token` downloads the generated file using
  its signed token.

The route applies export rate limiting, optional API-key authentication,
user authentication, and request-signature verification. Generation is
concurrency-limited per API key or user. Successful requests are recorded in
the audit log; generation failures return a stable error response.

The implementation is in
[`src/routes/v1/exports.ts`](../src/routes/v1/exports.ts),
[`src/controllers/v1/exports.ts`](../src/controllers/v1/exports.ts), and
[`src/services/exportService.ts`](../src/services/exportService.ts).
Relevant coverage is in
[`src/tests/streaming-exports.test.ts`](../src/tests/streaming-exports.test.ts)
and [`src/tests/export-concurrency.test.ts`](../src/tests/export-concurrency.test.ts).
