# QuickLendX Backend — Migration Workflow & Rollback Strategy

## Table of Contents
- [Overview](#overview)
- [Forward-Only Policy](#forward-only-policy)
- [Hotfix Emergency Protocol](#hotfix-emergency-protocol)
- [Directory Structure](#directory-structure)
- [Migration File Format](#migration-file-format)
- [Running Migrations](#running-migrations)
- [CI/CD Integration](#cicd-integration)
- [Rollback Procedures](#rollback-procedures)
- [Security Guardrails](#security-guardrails)
- [Incident Post-Mortem Template](#incident-post-mortem-template)

---

## Overview

The QuickLendX backend uses a **structured migration workflow** to manage database schema changes safely. All schema modifications are versioned, reviewed, and tested.

**Core Design Principles:**

1. **Forward-First**: Every migration is written with only an `up` function. Down migrations are exceptional, not routine.
2. **Single Version Sequence**: Versions are sequential integers. Gaps are forbidden.
3. **Immutable Record**: Once applied, a migration's version, name, and checksum are stored permanently in `_migrations` table.
4. **Atomic Transactions**: Each migration runs inside a transaction — failure rolls back completely.
5. **Idempotent Guard**: The runner tracks applied versions; re-running is a no-op.
6. **Hotfix-Approval Gate**: Critical changes require two-person approval before they can run in production.

---

## Forward-Only Policy

### What "Forward-Only" Means

**Standard migrations** (99% of cases):
- Contain **only** an `up` function
- Are irreversible by the system
- Cannot be "undone" — future migrations must compensate

**Example** (v001_baseline):
```typescript
export default {
  version: 1,
  name: "initial_schema",
  authoredAt: "2026-04-26",
  author: "alice",
  up: async (ctx) => {
    await ctx.db.exec("CREATE TABLE users (id INTEGER PRIMARY KEY)");
  },
  // down: undefined — by design
};
```

### Why Forward-Only?

- Prevents accidental production rollbacks that lose data
- Forces careful review and testing before merge
- Encourages additive, non-destructive schema evolution
- Matches production deployment model (immutable infrastructure)

### When Down Migrations ARE Allowed

Down migrations are **explicitly opt-in** and require:

1. `meta.hotfix = true` flag in migration definition
2. `down` function implemented
3. `--allow-down --emergency` flags on CLI
4. In production: `.hotfix-approvals/<version>_<name>.approval` file with two signatures
5. Documented rollback risk level: `low | medium | high`

---

## Hotfix Emergency Protocol

### Incident Response Flow

```
Production Incident Discovered
         ↓
   Create incident ticket
         ↓
   Form triage squad (2+ senior engineers)
         ↓
   Assess impact: data corruption? compliance breach? outage?
         ↓
   If schema change needed → create hotfix migration
         ↓
   Code review + security review (2 approvals required)
         ↓
   Create hotfix approval file in .hotfix-approvals/
         ↓
   Stage deploy → Run migration with --emergency flag
         ↓
   Monitor for 24h
         ↓
   Conduct post-mortem
```

### Hotfix Migration Requirements

Every hotfix migration `vNNN_*.ts` MUST include:

```typescript
export default {
  version: 100,
  name: "hotfix_fix_duplicate_index",
  authoredAt: "2026-04-26T12:00:00Z",
  author: "alice",
  meta: {
    hotfix: true,
    reason: "Duplicate index caused deadlock during peak load",  // Clear 1-sentence explanation
    rollback_risk: "medium",  // low | medium | high
    incident_ticket: "https://github.com/QuickLendX/quicklendx-protocol/issues/1234",
    required_approvals: 2,
  },
  up: async (ctx) => { /* fix */ },
  down: async (ctx) => { /* rollback plan */ },
};
```

### Hotfix Approval File Format

**Location**: `.hotfix-approvals/100_hotfix_fix_duplicate_index.approval`

```json
{
  "approved_by": ["alice", "bob"],           // GitHub usernames
  "approved_at": "2026-04-26T12:30:00Z",
  "incident_ticket": "https://github.com/.../issues/1234",
  "reason": "Remove duplicate index that was blocking writes",
  "rollback_plan": "Down migration drops index; no data loss since index is derived",
  "risk_accepted": "Medium: slight performance degradation on lookup queries during rollback",
  "signature_statement": "Both signers have reviewed the migration code, tested in staging, and accept responsibility"
}
```

**Approval workflow:**
1. Two senior engineers independently review the migration
2. Each adds their GitHub username to `approved_by`
3. Both must have merge access to the repository
4. CI checks verify both signatures exist before allowing hotfix to run in production

---

## Directory Structure

```
backend/
├── src/
│   ├── lib/
│   │   ├── database.ts              # DB connection manager (better-sqlite3)
│   │   └── migrations/
│   │       ├── types.ts             # TypeScript interfaces
│   │       ├── runner.ts            # Migration executor
│   │       ├── policy.ts            # Forward-only + hotfix policy
│   │       └── cli.ts               # CLI entrypoint
│   ├── migrations/                  # Source-of-truth migration files
│   │   ├── v001_initial_schema.ts
│   │   ├── v002_add_webhook_index.ts
│   │   ├── v003_hotfix_*.ts
│   │   └── ...
│   └── tests/                       # NEW test files only
│       ├── migration-runner.test.ts
│       ├── migration-policy.test.ts
│       └── migration-integration.test.ts
├── dist/                            # Compiled output
│   ├── lib/migrations/              # Compiled runner (JS)
│   └── migrations/                  # Compiled migration files (JS)
├── .hotfix-approvals/               # ⚠️  Production gate (committed)
│   ├── 003_hotfix_fix.approval
│   └── 005_emergency_fix.approval
├── .data/                           # SQLite database (gitignored)
├── docs/
│   └── migrations.md                # This file
└── package.json
```

---

## Migration File Format

Each migration file exports a default object implementing `MigrationDefinition`:

| Field | Type | Required? | Description |
|-------|------|-----------|-------------|
| `version` | `number` | ✅ | Unique, sequential integer (001, 002, 003...) |
| `name` | `string` | ✅ | snake_case short name (e.g. `add_webhook_table`) |
| `authoredAt` | `ISO date string` | ✅ | When migration was written |
| `author` | `string` | ✅ | GitHub username of author |
| `up` | `function(ctx)` | ✅ | Forward migration logic |
| `down` | `function(ctx)` | ⚠️ | Optional rollback function (rare) |
| `validate` | `function(ctx)` | optional | Pre-flight validation, returns string[] warnings |
| `meta` | `object` | optional | Metadata (hotfix flags, risk level, links) |

### Example: Standard Additive Migration

```typescript
// src/migrations/v002_add_webhook_index.ts
import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 2,
  name: "add_webhook_index",
  authoredAt: "2026-04-26",
  author: "bob@quicklendx",
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      CREATE INDEX IF NOT EXISTS idx_webhook_events
      ON webhook_subscriptions(events)
    `);
  },
  // No down — forward-only
} satisfies MigrationDefinition;
```

### Example: Hotfix Migration With Rollback

```typescript
// src/migrations/v003_hotfix_fk_constraint.ts
import type { MigrationDefinition, MigrationContext } from "../lib/migrations/types";

export default {
  version: 3,
  name: "hotfix_add_fk_to_backfill_audit",
  authoredAt: "2026-04-26T14:30:00Z",
  author: "alice",
  meta: {
    hotfix: true,
    reason: "Foreign key missing between backfill_audit and backfill_runs",
    rollback_risk: "high",  // dropping FK loses referential integrity
    incident_ticket: "https://github.com/.../issues/456",
  },
  up: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`
      ALTER TABLE backfill_audit
      ADD CONSTRAINT fk_run
      FOREIGN KEY (run_id) REFERENCES backfill_runs(id)
      ON DELETE CASCADE
    `);
  },
  down: async (ctx: MigrationContext): Promise<void> => {
    await ctx.db.exec(`ALTER TABLE backfill_audit DROP CONSTRAINT fk_run`);
  },
} satisfies MigrationDefinition;
```

---

## Running Migrations

### Standard Application (CI/CD)

```bash
# Apply all pending migrations
cd backend
npm run migrate

# Dry-run — preview changes without modifying DB
npm run migrate -- --dry-run

# CI check — fail if migrations out of sync
npm run migrate -- --check
```

### Hotfix Rollback (Emergency Only)

```bash
# Step 1: Confirm you have approval file
ls .hotfix-approvals/003_*.approval  # must exist

# Step 2: Run down migration with both flags
npm run migrate -- --allow-down --emergency

# Step 3: Monitor application health
npm run health-check
```

### Verbose Logging

```bash
npm run migrate -- --verbose
```

### Validation Only (pre-commit)

```bash
# Verify all migration files are well-formed, no version conflicts
npm run migrate -- --validate-only
```

---

## CI/CD Integration

### GitHub Actions Workflow

```yaml
# .github/workflows/backend-migrations.yml
name: Backend Migrations

on:
  push:
    branches: [main, develop]
    paths: ['backend/src/migrations/**', 'backend/src/lib/migrations/**']
  pull_request:
    branches: [main]
    paths: ['backend/src/migrations/**', 'backend/lib/migrations/**']

jobs:
  validate:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'npm'
          cache-dependency-path: backend/package-lock.json
      - name: Install dependencies
        run: cd backend && npm ci
      - name: Validate migrations
        run: cd backend && npm run migrate -- --validate-only
      - name: Check migration sync
        run: cd backend && npm run migrate -- --check

  test:
    needs: validate
    runs-on: ubuntu-latest
    services:
      sqlite:
        image: nouchka/sqlite3:latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with: { node-version: '20', cache: 'npm', cache-dependency-path: backend/package-lock.json }
      - name: Install
        run: cd backend && npm ci
      - name: Run migration unit tests
        run: cd backend && npm test -- src/tests/migration-runner.test.ts --coverage
      - name: Enforce 95% coverage threshold
        run: cd backend && npx jest --coverage --coverageThreshold='{"global":{"branches":95,"functions":95,"lines":95,"statements":95}}' src/tests/migration-runner.test.ts
```

### Add to `backend/package.json`:

```json
{
  "scripts": {
    "migrate": "ts-node src/lib/migrations/cli.ts",
    "migrate:check": "npm run migrate -- --check",
    "migrate:dry-run": "npm run migrate -- --dry-run",
    "migrate:validate": "npm run migrate -- --validate-only",
    "migrate:up": "npm run migrate",
    "migrate:down": "npm run migrate -- --allow-down --emergency",
    "build": "tsc && cp -r src/migrations dist/migrations && cp -r src/lib/migrations dist/lib/migrations"
  }
}
```

---

## Rollback Procedures

### Standard Rollback (Forward-Only world)

**There is no `--rollback` command.** Instead:

1. Write a new migration (`v00X_compensate_previous_change.ts`) that reverses the effect
2. Example: if `v005_add_column_to_users` added `legacy_id`, the compensating migration is `v006_drop_legacy_id_column` (only after data is migrated elsewhere)
3. Test thoroughly in staging
4. Apply via `npm run migrate`

### Emergency Hotfix Rollback

If a hotfix migration itself is flawed:

```bash
# 1. Verify approval exists for rolling back THIS specific migration
ls .hotfix-approvals/rollback_005_*.approval

# 2. Run with down + emergency flags
npm run migrate -- --allow-down --emergency

# 3. The `down` function from v005 executes within a transaction
#    If it fails, DB state is reverted to pre-migration (safe)
```

**After any rollback:**

- Notify the incident response channel immediately
- Freeze further deployments until RCA complete
- Create post-mortem issue (see template below)

---

## Security Guardrails

### Production Safety Blocks

1. **Down-block**: `allowDown` defaults to `false`; even setting flag requires `ALLOW_DOWN_MIGRATIONS=true` env var
2. **Hotfix-approval**: In production, `isHotfix()` forces presence of approval file
3. **Version monotonicity**: Runner refuses to apply v003 before v002 (enforced by file sorting)
4. **Checksum verification**: File content is checksummed and stored; tampering causes mismatch error
5. **Transaction boundaries**: Entire migration executes in one SQLite transaction; any error reverts fully
6. **Dry-run mode**: `--dry-run` compiles and validates but doesn't write
7. **Validate-only mode**: `--validate-only` exits non-zero on any validation error

### Secrets Handling

- Never embed credentials in migration SQL
- Use environment variables via `ctx.env` (available but avoid if possible)
- Logging: do NOT log parameter values; log structure only

### Data Loss Prevention

- Down migrations that `DROP TABLE` or `DROP COLUMN` MUST have `meta.rollback_risk = high`
- High-risk down migrations require explicit `--force` flag in addition to `--emergency`
- All destructive operations must include pre-flight "are you sure?" warnings

---

## Incident Post-Mortem Template

```markdown
# Post-Mortem: Migration v003 Hotfix Rollback

**Incident ID:** INC-2026-04-26-001
**Date:** 2026-04-26
**Affected Migration:** v003_hotfix_add_invoice_id_to_backfill_audit
**Severity:** P2 (Production Impact)

## Timeline
- 14:30 UTC: v003 applied to production
- 14:35 UTC: Alert: API errors on `/api/v1/backfill` endpoint
- 14:40 UTC: Triage squad formed; root cause identified: NULL constraint violation
- 14:45 UTC: Rollback decision made; approval file created
- 14:50 UTC: Rollback complete; service restored

## Root Cause
The migration added `invoice_id TEXT NOT NULL` without default, but existing backfill audit rows had no value — constraint violation on UPDATE backfill operation.

## Fix Applied
Rolled back v003 via `npm run migrate -- --allow-down --emergency`
- Down migration executed successfully
- Data in `invoice_id` column (written post-v003) was lost (acceptable: it was NULL anyway)

## Preventative Actions
- [ ] All ALTER TABLE ADD COLUMN must be nullable or include DEFAULT clause
- [ ] Add migration lint rule to flag NOT NULL on non-primary-key columns
- [ ] Require dry-run against production clone before hotfix merge

## Approval Records
- [Approval file](.hotfix-approvals/003_hotfix_add_invoice_id.approval) signed by alice, bob
```

---

## Appendix: Migration CLI Reference

```
Usage: npm run migrate -- [options]

Options:
  --dry-run         Preview migrations without modifying database
  --allow-down       Enable down migrations (emergency only)
  --emergency        Acknowledge this is a production-impacting change
  --validate-only    Only validate migration file syntax; don't run
  --check            CI mode: exit 0 if migrations in sync, 1 otherwise
  --verbose          Detailed logging
  -h, --help         Show help

Examples:
  npm run migrate                       # apply pending migrations
  npm run migrate -- --dry-run          # show what would run
  npm run migrate -- --check            # CI gate (exits 0/1)
  npm run migrate -- --allow-down --emergency   # emergency rollback
```

---

## Appendix: File Naming Convention

| Pattern | Example | Meaning |
|---------|---------|---------|
| `vNNN_name.ts` | `v001_initial_schema.ts` | Standard forward-only migration |
| `vNNN_hotfix_name.ts` | `v003_hotfix_fix_audit.ts` | Hotfix requiring approval |
| `NNN_name.ts` | `005_add_index.ts` | Alternative format (also accepted) |

**Rules:**
- Version must be 3 digits (001, 002, ..., 999)
- Name must be `snake_case`, no spaces
- File extension must be `.ts` (source) and compile to `.js` (dist)

---

## Appendix: Build Process

### Why We Compile Migrations

Node.js cannot `require()` TypeScript files directly. The build process:

1. `tsc` compiles `src/migrations/*.ts` → `dist/migrations/*.js`
2. `cp -r src/migrations dist/migrations` ensures JS files exist
3. Runtime loader (`runner.ts`) uses `require()` on `.js` files only

### Build Script

```json
{
  "scripts": {
    "build": "tsc && cp -r src/migrations dist/migrations && cp -r src/lib/migrations dist/lib/migrations"
  }
}
```

### Runtime Path Resolution

```typescript
// runner.ts
const MIGRATIONS_DIR = process.env.NODE_ENV === "production"
  ? path.resolve(__dirname, "../../migrations")  // dist/migrations/
  : path.resolve(__dirname, "../../src/migrations");  // src/migrations/
```

---

**Last Updated:** 2026-04-26  
**Owner:** QuickLendX Platform Team  
**Review Cadence:** Quarterly or after any hotfix incident
