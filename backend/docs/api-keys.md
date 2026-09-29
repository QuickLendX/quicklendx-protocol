# API Key Storage Model

## Overview

API keys and their audit logs are persisted in a local SQLite database via the `better-sqlite3` library. This replaces the previous in-memory store and ensures credentials survive restarts.

## Database Tables

### `api_keys`

| Column | Type | Description |
|--------|------|-------------|
| `id` | TEXT (PK) | UUID v4 |
| `key_hash` | TEXT NOT NULL | SHA-256 hash of the raw key |
| `prefix` | TEXT NOT NULL | First 15 characters (`qlx_<env>_xxxxx`) |
| `name` | TEXT NOT NULL | Human-readable label |
| `scopes` | TEXT NOT NULL | JSON array of scope strings |
| `created_at` | TEXT NOT NULL | ISO 8601 timestamp |
| `last_used_at` | TEXT | Last authentication timestamp |
| `expires_at` | TEXT | Optional expiration timestamp |
| `revoked` | INTEGER NOT NULL DEFAULT 0 | 0 = active, 1 = revoked |
| `created_by` | TEXT NOT NULL | Actor who created the key |

**Indexes:**
- `idx_api_keys_prefix` — UNIQUE index on `prefix` for O(1) lookup during authentication
- `idx_api_keys_created_by` — filter by creator
- `idx_api_keys_revoked` — filter by revocation status

### `api_key_audit_log`

| Column | Type | Description |
|--------|------|-------------|
| `id` | TEXT (PK) | UUID v4 |
| `event_type` | TEXT NOT NULL | One of `created`, `used`, `rotated`, `revoked` |
| `key_id` | TEXT NOT NULL | FK → `api_keys.id` (CASCADE on delete) |
| `actor` | TEXT NOT NULL | Who performed the action |
| `timestamp` | TEXT NOT NULL | ISO 8601 timestamp |
| `ip_address` | TEXT | Client IP address |
| `endpoint` | TEXT | API endpoint (for `used` events) |
| `metadata` | TEXT | Optional JSON object |

**Indexes:**
- `idx_api_key_audit_key_id` — filter by key
- `idx_api_key_audit_event_type` — filter by event type
- `idx_api_key_audit_timestamp` — sort by time

## Security Properties

- **Raw secrets never stored**: Only SHA-256 hashes written to `api_keys.key_hash`
- **Timing-safe comparison**: Verification uses `crypto.timingSafeEqual` on the hash
- **Append-only audit**: The `api_key_audit_log` table is INSERT-only; the public interface exposes no UPDATE or DELETE for audit rows
- **Prefix-based lookup**: The UNIQUE index on `prefix` provides O(1) authentication path

## Migration

The tables are created by `src/migrations/v006_create_api_keys.ts`, which runs automatically via the migration runner (`src/lib/migrations/runner.ts`) on server startup.

## Source Code Location

- **Data access layer**: `src/db/database.ts` — `Database` class using `better-sqlite3` prepared statements
- **Service layer**: `src/services/api-key-service.ts` — business logic (creation, verification, rotation, revocation)
- **Audit service**: `src/services/audit-log.ts` — async audit event logging
- **Middleware**: `src/middleware/api-key-auth.ts` — Bearer token extraction and verification
