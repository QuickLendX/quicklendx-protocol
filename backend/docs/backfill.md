# Admin Backfill Tooling

This document describes the admin-only ledger-range backfill tooling for issue `#875`.

## Security model

- All admin backfill endpoints require `Authorization: Bearer <token>`.
- Token is read from `ADMIN_API_TOKEN`.
- If `ADMIN_API_TOKEN` is not set, admin endpoints fail closed with `503`.
- Optional `x-admin-actor` header is captured for audit attribution.

## Endpoints

- `POST /api/admin/backfill`
  - Starts a run or executes dry-run preview.
  - Request fields:
    - `startLedger` (integer, required)
    - `endLedger` (integer, required)
    - `dryRun` (boolean, default `false`)
    - `concurrency` (integer, default `1`)
    - `idempotencyKey` (string, optional)
- `GET /api/admin/backfill/runs`
  - Lists tracked runs.
- `GET /api/admin/backfill/:runId`
  - Returns run details.
- `POST /api/admin/backfill/pause`
  - Pauses a running run.
- `POST /api/admin/backfill/resume`
  - Resumes paused or failed run from last cursor.

## Guardrails

- Max range guardrail via `BACKFILL_MAX_LEDGER_RANGE` (default `5000`).
- Max concurrency guardrail via `BACKFILL_MAX_CONCURRENCY` (default `4`).
- Invalid range (`endLedger < startLedger`) is rejected.
- Dry-run computes affected scope preview without mutating run state.

## Audit log

- Every preview/start/pause/resume/complete/failure/idempotent-reuse action is appended to a persistent JSONL file.
- Path is configurable with `BACKFILL_AUDIT_LOG_PATH`.
- Default path: `backend/.data/backfill-audit-log.jsonl` (resolved from process cwd).

## Pause/resume behavior

- Runner processes work in bounded chunks to keep control points frequent.
- Pause stops future ticks while preserving cursor and processed count.
- Resume restarts from cursor.
- Failed runs can be resumed after operator remediation.
