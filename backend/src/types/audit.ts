import { z } from "zod";

export const AuditOperationSchema = z.enum([
  "MAINTENANCE_MODE",
  "WEBHOOK_SECRET_ROTATE",
  "CONFIG_CHANGE",
  "BACKFILL_START",
  "BACKFILL_PROGRESS",
  "BACKFILL_COMPLETE",
  "BACKFILL_ABORT",
  "ADMIN_API_KEY_ADD",
  "ADMIN_API_KEY_REVOKE",
]);

export type AuditOperation = z.infer<typeof AuditOperationSchema>;

export const SENSITIVE_FIELDS = new Set([
  "secret",
  "token",
  "apikey",
  "api_key",
  "authorization",
  "credential",
  "privatekey",
  "private_key",
  "accesstoken",
  "access_token",
  "refreshtoken",
  "refresh_token",
  "password",
]);

export const AuditEntrySchema = z.object({
  id: z.string(),
  timestamp: z.string().datetime(),
  actor: z.string(),
  operation: AuditOperationSchema,
  params: z.record(z.string(), z.unknown()),
  redactedParams: z.record(z.string(), z.unknown()),
  ip: z.string(),
  userAgent: z.string(),
  effect: z.string(),
  success: z.boolean(),
  errorMessage: z.string().optional(),
});

export type AuditEntry = z.infer<typeof AuditEntrySchema>;

export const AuditQuerySchema = z.object({
  actor: z.string().optional(),
  operation: AuditOperationSchema.optional(),
  from: z.string().datetime().optional(),
  to: z.string().datetime().optional(),
  limit: z.coerce.number().int().min(1).max(10000).default(100),
  offset: z.coerce.number().int().min(0).default(0),
});

export type AuditQuery = z.infer<typeof AuditQuerySchema>;

export const AuditQueryResponseSchema = z.object({
  entries: AuditEntrySchema.array(),
  total: z.number(),
  limit: z.number(),
  offset: z.number(),
  hasMore: z.boolean(),
});

export type AuditQueryResponse = z.infer<typeof AuditQueryResponseSchema>;

export function redactSensitiveFields(
  params: Record<string, unknown>
): Record<string, unknown> {
  const redacted: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(params)) {
    if (SENSITIVE_FIELDS.has(key.toLowerCase())) {
      redacted[key] = "[REDACTED]";
    } else if (typeof value === "object" && value !== null) {
      redacted[key] = redactSensitiveFields(value as Record<string, unknown>);
    } else {
      redacted[key] = value;
    }
  }
  return redacted;
}