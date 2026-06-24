/**
 * Central config loader. Validates env vars at startup so the app fails fast
 * with a clear message rather than silently using undefined values.
 *
 * Rules:
 *  - Never log secret values (API keys, tokens).
 *  - All secrets are optional in dev; required in production.
 *  - Consumers import `config`, not `process.env` directly.
 */
import { z } from "zod";

const isProduction = process.env.NODE_ENV === "production";

const schema = z.object({
  NODE_ENV: z.enum(["development", "test", "production"]).default("development"),
  PORT: z.coerce.number().int().min(1).max(65535).default(3001),
  STELLAR_RPC_URL: z.url().default("https://soroban-testnet.stellar.org"),
  RATE_LIMIT_POINTS: z.coerce.number().int().min(1).default(100),
  RATE_LIMIT_PER_KEY_POINTS: z.coerce.number().int().min(1).default(60),
  RATE_LIMIT_RECONCILIATION_POINTS: z.coerce.number().int().min(1).default(10),
  RATE_LIMIT_EXPORT_POINTS: z.coerce.number().int().min(1).default(5),

  // Database configuration
  DATABASE_PATH: z.string().default(function () {
    return process.env.NODE_ENV === "production"
      ? "/var/lib/quicklendx/backend.db"
      : ".data/dev.db";
  }),

  // Secrets — required in production, optional elsewhere.
  ADMIN_API_KEY: isProduction
    ? z.string().min(32)
    : z.string().min(32).optional(),
  WEBHOOK_SECRET: isProduction
    ? z.string().min(16)
    : z.string().min(16).optional(),
  EXPORT_SECRET: isProduction
    ? z.string().min(32)
    : z.string().min(32).default("development-only-export-secret-32-chars"),
  RPC_ALLOWED_HOSTS: z.string().default("soroban-testnet.stellar.org,soroban-mainnet.stellar.org,localhost"),
  RETENTION_RAW_EVENTS_DAYS: z.coerce.number().int().min(1).default(30),
  RETENTION_AUDIT_LOG_DAYS: z.coerce.number().int().min(1).default(90),
  RETENTION_SNAPSHOTS_DAYS: z.coerce.number().int().min(1).default(14),
  RETENTION_BATCH_SIZE: z.coerce.number().int().min(1).max(10000).default(500),
  RETENTION_INTERVAL_MS: z.coerce.number().int().min(60000).default(24 * 60 * 60 * 1000),
  RETENTION_ARCHIVE_DIR: z.string().default(".data/retention-archives"),
  RETENTION_AUDIT_ACTOR: z.string().min(1).default("system:retention-worker"),

  // Alert routing configuration
  ALERT_ROUTES_JSON: z.string().optional(),
  ALERT_DEDUPE_WINDOW_MS: z.coerce.number().int().min(0).default(15 * 60 * 1000), // 15 minutes
  PAGERDUTY_INTEGRATION_KEY: isProduction
    ? z.string().min(1).optional()
    : z.string().min(1).optional(),
  SLACK_WEBHOOK_URL: isProduction
    ? z.string().url().optional()
    : z.string().url().optional(),
  ALERT_EMAIL_RECIPIENTS: z.string().optional(), // comma-separated emails
});

export type Config = z.infer<typeof schema>;

function load(): Config {
  const result = schema.safeParse(process.env);
  if (!result.success) {
    const fields = result.error.issues.map((i) => i.path.join(".")).join(", ");
    throw new Error(`Invalid configuration: ${fields}`);
  }
  return result.data;
}

export const config = load();

export interface AlertRoute {
  severity: "LOW" | "MEDIUM" | "HIGH";
  channels: ("email" | "slack" | "pagerduty")[];
}

export interface AlertRoutes {
  routes?: AlertRoute[];
}

function parseAlertRoutes(): AlertRoutes {
  if (!config.ALERT_ROUTES_JSON) {
    return { routes: undefined };
  }
  try {
    return JSON.parse(config.ALERT_ROUTES_JSON);
  } catch (err) {
    console.error("Failed to parse ALERT_ROUTES_JSON:", err);
    throw new Error("Invalid ALERT_ROUTES_JSON configuration");
  }
}

export const alertConfig = {
  dedupeWindowMs: config.ALERT_DEDUPE_WINDOW_MS,
  pagerdutyIntegrationKey: config.PAGERDUTY_INTEGRATION_KEY,
  slackWebhookUrl: config.SLACK_WEBHOOK_URL,
  emailRecipients: config.ALERT_EMAIL_RECIPIENTS?.split(",").map((e) => e.trim()) || [],
  routes: parseAlertRoutes().routes || [
    { severity: "HIGH", channels: ["pagerduty", "slack"] },
    { severity: "MEDIUM", channels: ["slack", "email"] },
    { severity: "LOW", channels: ["email"] },
  ],
} as const;

export const retentionConfig = {
  rawEventsMs: config.RETENTION_RAW_EVENTS_DAYS * 24 * 60 * 60 * 1000,
  auditLogsMs: config.RETENTION_AUDIT_LOG_DAYS * 24 * 60 * 60 * 1000,
  snapshotsMs: config.RETENTION_SNAPSHOTS_DAYS * 24 * 60 * 60 * 1000,
  batchSize: config.RETENTION_BATCH_SIZE,
  intervalMs: config.RETENTION_INTERVAL_MS,
  archiveDir: config.ARCHIVE_DIR,
  actor: config.RETENTION_AUDIT_ACTOR,
  archiveEnabled: config.ARCHIVE_ENABLED,
} as const;

