# Backend configuration

The backend reads and validates environment variables in
[`src/config.ts`](../src/config.ts) at startup. Invalid values stop startup;
secret values are not included in validation errors. The defaults below reflect
the schema in that file.

| Variable | Default | Purpose |
|---|---|---|
| `NODE_ENV` | `development` | `development`, `test`, or `production` |
| `PORT` | `3001` | HTTP listener port |
| `STELLAR_RPC_URL` | Soroban testnet URL | RPC endpoint |
| `DATABASE_PATH` | `.data/dev.db` (or production path) | SQLite database file |
| `RATE_LIMIT_POINTS` | `100` | General rate-limit points |
| `RATE_LIMIT_PER_KEY_POINTS` | `60` | Per-key rate-limit points |
| `RATE_LIMIT_RECONCILIATION_POINTS` | `10` | Reconciliation rate-limit points |
| `RATE_LIMIT_EXPORT_POINTS` | `5` | Export rate-limit points |
| `EXPORT_MAX_CONCURRENT_PER_KEY` | `2` | Concurrent exports per key |
| `EXPORT_DIR` | `.data/exports` | Generated export directory |
| `EXPORT_TTL_MS` | `3600000` | Export link lifetime |
| `ARCHIVE_DIR` | `.data/archives` | Archive output directory |
| `ARCHIVE_ENABLED` | `false` | Archive feature toggle |
| `EXPOSURE_CAP_PER_INVESTOR_USD` | `10000000000` | Investor exposure ceiling |
| `ADMIN_API_KEY` | unset | Required in production; at least 32 characters |
| `WEBHOOK_SECRET` | unset | Required in production; at least 16 characters |
| `EXPORT_SECRET` | Development-only default | Required in production; at least 32 characters |
| `RPC_ALLOWED_HOSTS` | Testnet, mainnet, localhost | Comma-separated allowed RPC hosts |
| `RETENTION_RAW_EVENTS_DAYS` | `30` | Raw event retention |
| `RETENTION_AUDIT_LOG_DAYS` | `90` | Audit log retention |
| `RETENTION_SNAPSHOTS_DAYS` | `14` | Snapshot retention |
| `RETENTION_BATCH_SIZE` | `500` | Retention processing batch |
| `RETENTION_INTERVAL_MS` | `86400000` | Retention interval |
| `RETENTION_ARCHIVE_DIR` | `.data/retention-archives` | Retention archive directory |
| `RETENTION_AUDIT_ACTOR` | `system:retention-worker` | Audit actor for retention |
| `ALERT_ROUTES_JSON` | unset | Optional alert routing configuration |
| `ALERT_DEDUPE_WINDOW_MS` | `900000` | Alert deduplication window |
| `MAX_NOTIFICATION_DEDUP_ENTRIES` | `10000` | Notification deduplication cap |
| `NOTIFICATION_DEDUP_TTL_MS` | `86400000` | Notification deduplication lifetime |
| `PAGERDUTY_INTEGRATION_KEY` | unset | Optional PagerDuty integration |
| `SLACK_WEBHOOK_URL` | unset | Optional Slack alert destination |
| `ALERT_EMAIL_RECIPIENTS` | unset | Optional comma-separated email recipients |

Set environment variables through the deployment environment. Do not commit
credentials or production secrets. See [`secrets.md`](./secrets.md) for local
development and committed-secret scanning.
