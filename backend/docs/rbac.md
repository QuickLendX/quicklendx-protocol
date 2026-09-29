# Backend RBAC

This document describes the backend role-based access control model for operational and support endpoints.

## Goals

- Separate troubleshooting, routine operations, and high-risk configuration changes.
- Fail closed when credentials are missing or ambiguous.
- Keep the implementation small, explicit, and easy to review.
- Preserve least privilege by default.

## Roles

| Role               | Purpose                   | Allowed actions                                                         |
| ------------------ | ------------------------- | ----------------------------------------------------------------------- |
| `support`          | Read-only troubleshooting | Read admin status, read audit logs                                      |
| `operations_admin` | Operational changes       | Everything `support` can do, plus maintenance toggles and backfill jobs |
| `super_admin`      | Dangerous configuration   | Everything `operations_admin` can do, plus dangerous config changes     |

## Credential Model

Administrative credentials are persisted API keys stored in the backend database. Keys are created, rotated, and revoked via the API key management endpoints. Only SHA-256 hashes of keys are stored — plaintext keys are returned to callers only at creation time.

Incoming admin requests present a bearer token in `Authorization: Bearer <key>`. The middleware verifies the key against the persisted store and maps the key's granted scopes to an administrative role. Plaintext key values are never logged or stored.

## Protected Endpoints

| Endpoint                      | Method | Allowed roles                                | Notes                         |
| ----------------------------- | ------ | -------------------------------------------- | ----------------------------- |
| `/api/admin/status`           | `GET`  | `support`, `operations_admin`, `super_admin` | Internal troubleshooting view |
| `/api/admin/audit-logs`       | `GET`  | `support`, `operations_admin`, `super_admin` | Read-only audit trail         |
| `/api/admin/maintenance`      | `POST` | `operations_admin`, `super_admin`            | Toggle maintenance mode       |
| `/api/admin/backfill`         | `POST` | `operations_admin`, `super_admin`            | Queue backfill jobs           |
| `/api/admin/config/dangerous` | `POST` | `super_admin`                                | High-risk config updates      |

## Fail-Closed Behavior

- Missing bearer credentials return `401 AUTH_REQUIRED`.
- Invalid, revoked, or expired credentials return `403 FORBIDDEN`.
- Authenticated but unauthorized roles return `403 INSUFFICIENT_ROLE`.

The middleware resolves roles from the persisted API key store at request time; this enables key rotation without process restarts and provides per-key auditability. If a key does not map to any administrative role (i.e., lacks admin scopes), the request is rejected with `INSUFFICIENT_ROLE`.

## Audit Logging

The backend records two classes of audit events in memory:

- Authorization decisions: allowed or denied attempts against protected admin endpoints.
- Privileged actions: successful maintenance toggles, backfill requests, and dangerous config updates.

Each entry captures timestamp, role, request method, path, client IP, action name, and optional reason/metadata.

## Security Notes

- Least privilege is enforced with route-level allowlists instead of implicit hierarchy checks.
- `support` is strictly read-only.
- `operations_admin` cannot change dangerous configuration.
- `super_admin` is the only role permitted to perform dangerous config writes.
- Duplicate tokens are rejected to avoid accidental privilege overlap.
- Audit access is read-only and intentionally exposed to `support` for troubleshooting without write authority.

## Testing

Authorization behavior is covered with matrix tests in [src/tests/rbac.test.ts](../src/tests/rbac.test.ts) and status/admin behavior checks in [src/tests/status.test.ts](../src/tests/status.test.ts).
