# Secret Management & Hot Reload Protocol

## Overview

The QuickLendX backend distinguishes between two categories of configuration:

1. **Secrets** — sensitive values that are **immutable after boot** (e.g., `JWT_SECRET`, `API_KEY`, `ENCRYPTION_KEY`, `DATABASE_URL`).
2. **Runtime (hot-reloadable) values** — safe-to-change operational parameters that can be updated without restarting the process.

This document describes the SIGHUP-triggered hot reload mechanism for runtime values.

## Hot-Reloadable Keys

The following configuration keys are marked with `hotReloadable:true` in the Zod schema metadata (via `.describe()`) and can be changed at runtime:

| Key                      | Type      | Default | Description                          |
|--------------------------|-----------|---------|--------------------------------------|
| `ENABLE_RATE_LIMITING`   | boolean   | `true`  | Enable API rate limiting             |
| `MAX_REQUESTS_PER_MINUTE`| integer   | `100`   | Max requests per minute              |
| `RATE_LIMIT_POINTS`      | integer   | `1000`  | Token-bucket rate limit point budget |
| `RPC_ALLOWED_HOSTS`      | string[]  | `["*"]` | Comma-separated list of allowed RPC hostnames |
| `LAG_WARN_THRESHOLD`     | integer   | `10`    | Ledger lag that triggers a warning   |
| `LAG_CRITICAL_THRESHOLD` | integer   | `100`   | Ledger lag that triggers a critical alert |

All other keys (secrets, database URLs, Stellar endpoints, etc.) are **never** overwritten by a reload.

## Reload Protocol

### Trigger

Send `SIGHUP` to the running process:

```bash
kill -HUP <PID>
```

The process ID is written to stdout at startup and can be found with `pgrep -f quicklendx-backend`.

### What Happens

1. The `SIGHUP` handler calls `reloadConfig()`.
2. Environment files (`.env`, `.env.<profile>`, `.env.<profile>.local`) are re-read via `dotenv`.
3. The Zod schema is re-parsed against the merged environment.
   - If validation fails, an error is logged and the **current config is preserved** — the process continues running with the old values.
4. Only keys explicitly tagged `hotReloadable:true` in `src/config/schema.ts` are extracted from the fresh parse.
5. These keys are merged into the existing config singleton — **secrets and other immutable keys are left untouched**.
6. All registered subscribers (including `lagMonitor`, `rateLimitMiddleware`, and any custom `onReload()` callbacks) are notified with the merged config.
7. The new safe-to-log config (with secrets redacted) is printed to stdout.

### Idempotency

Calling `setupSignalHandlers()` multiple times is safe — only one `SIGHUP` listener is ever registered. Rapid successive `SIGHUP` signals are processed sequentially; each reload is synchronous and completes before the next begins.

### Integration Points

| Module                 | Subscribes Via            | Effect                         |
|------------------------|---------------------------|--------------------------------|
| `services/lagMonitor`  | `setupLagMonitorReload()` | Updates lag warn/critical thresholds |
| `middleware/rate-limit` | `setupRateLimitReload()` | Updates rate-limit point budget and max requests/min |

Custom modules can subscribe:

```typescript
import { onReload } from '../config';

const unsub = onReload((config) => {
  // React to new config values — secrets are guaranteed unchanged
});
// Later: unsub();
```

### Security Guarantees

- **Secrets are never written to logs** — the reload log uses `getSafeConfig()` which masks all sensitive keys.
- **Secrets are never overwritten** — `reloadConfig()` only copies keys from the `hotReloadableKeys` list.
- **Invalid input is rejected** — if the env file contains a bad value for a hot-reloadable key, the prior value is preserved.
- **Missing required secrets on reload are ignored** — validation may fail but the running config is kept.

## Testing

Run the hot-reload test suite:

```bash
npm test -- config-hot-reload
```

Coverage includes:
- Reload changes a hot-reloadable value
- Invalid values keep the prior configuration
- Invalid types fall back to defaults
- Secrets are immutable post-boot
- Multiple subscribers are all notified
- Unsubscribe removes a subscriber
- SIGHUP handler processes rapid signals correctly
- `setupSignalHandlers()` is idempotent
- Log output never contains secret values
- Subscriber count is accurately tracked
- Graceful no-op when no env changes

## Adding a New Hot-Reloadable Key

1. Add the field to `ConfigSchema` in `src/config/schema.ts`.
2. Wrap the schema with the `hotReloadable()` helper (or append `.describe('hotReloadable:true')`).
3. Wire the consumer via `onReload()` in the appropriate module.
4. Add tests in `src/tests/config-hot-reload.test.ts`.
5. Update the table in this document.
