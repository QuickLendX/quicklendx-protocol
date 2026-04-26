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
});

export type Config = z.infer<typeof schema>;

function load(): Config {
  const result = schema.safeParse(process.env);
  if (!result.success) {
    // Print field names only — never print values.
    const fields = result.error.issues.map((i) => i.path.join(".")).join(", ");
    throw new Error(`Invalid configuration: ${fields}`);
  }
  return result.data;
}

export const config = load();
