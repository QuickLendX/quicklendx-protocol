# Export implementation status

The canonical backend export workflow is in `backend/src/routes/v1/exports.ts`,
`backend/src/controllers/v1/exports.ts`, and
`backend/src/services/exportService.ts`. See
[backend/docs/exports.md](backend/docs/exports.md) for current endpoints and
behavior.

The duplicate package's separate metrics service was intentionally dropped;
monitoring lives in `backend/src/routes/health.ts` and the lag monitor.
The config hot-reload loader was intentionally dropped; startup configuration
is validated by `backend/src/config.ts`.
The contract-test harness was intentionally dropped; API contracts are defined
in `backend/openapi.yaml` and exercised by the canonical backend tests.
