import { z } from 'zod';

/**
 * Environment profile types
 */
export const ProfileSchema = z.enum(['development', 'test', 'production']);
export type Profile = z.infer<typeof ProfileSchema>;

/**
 * Log level types
 */
export const LogLevelSchema = z.enum(['debug', 'info', 'warn', 'error']);
export type LogLevel = z.infer<typeof LogLevelSchema>;

/**
 * Base configuration schema with strict validation
 */
export const ConfigSchema = z.object({
  // Application
  NODE_ENV: ProfileSchema.default('development'),
  PORT: z.coerce.number().int().min(1).max(65535).default(3000),
  LOG_LEVEL: LogLevelSchema.default('info'),

  // Database
  DATABASE_URL: z.string().url().min(1),
  DATABASE_POOL_SIZE: z.coerce.number().int().min(1).max(100).default(10),

  // Authentication & Security
  JWT_SECRET: z.string().min(32),
  API_KEY: z.string().min(16),
  ENCRYPTION_KEY: z.string().min(32),

  // External Services
  STELLAR_NETWORK_URL: z.string().url(),
  STELLAR_NETWORK_PASSPHRASE: z.string().min(1),

  // Feature Flags
  ENABLE_RATE_LIMITING: z.coerce.boolean().default(true),
  MAX_REQUESTS_PER_MINUTE: z.coerce.number().int().min(1).max(10000).default(100),

  // Monitoring (optional)
  SENTRY_DSN: z.string().url().optional(),
});

/**
 * Production-specific schema with stricter validation
 */
export const ProductionConfigSchema = ConfigSchema.extend({
  JWT_SECRET: z.string().min(64),
  API_KEY: z.string().min(32),
  ENCRYPTION_KEY: z.string().min(64),
  DATABASE_URL: z.string().url().refine(
    (url) => url.startsWith('postgresql://') || url.startsWith('postgres://'),
    { message: 'Production database must use PostgreSQL' }
  ),
});

export type Config = z.infer<typeof ConfigSchema>;
export type ProductionConfig = z.infer<typeof ProductionConfigSchema>;
