import { z } from "zod";

export const DEFAULT_API_URL = "http://localhost:3001/api/v1";
export const DEFAULT_NETWORK = "testnet";

const booleanFlagSchema = (varName: string) =>
  z
    .union([z.string(), z.boolean()])
    .optional()
    .transform((val, ctx): boolean => {
      if (val === undefined) {
        return false;
      }
      if (typeof val === "boolean") {
        return val;
      }
      const normalized = val.trim().toLowerCase();
      if (normalized === "true" || normalized === "1") {
        return true;
      }
      if (normalized === "false" || normalized === "0") {
        return false;
      }
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: `${varName} must be a boolean flag ("true", "false", "1", or "0"), received "${val}"`,
      });
      return z.NEVER;
    });

const apiUrlSchema = z
  .string()
  .optional()
  .transform((val) => (val === undefined ? DEFAULT_API_URL : val.trim()))
  .superRefine((val, ctx) => {
    let parsed: URL;
    try {
      parsed = new URL(val);
    } catch {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: `NEXT_PUBLIC_API_URL must be an absolute http(s) URL ending with /api/v1 (e.g. ${DEFAULT_API_URL}), received "${val}"`,
      });
      return;
    }

    if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: `NEXT_PUBLIC_API_URL must use http:// or https:// protocol, received "${val}"`,
      });
      return;
    }

    if (!parsed.pathname.endsWith("/api/v1")) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        message: `NEXT_PUBLIC_API_URL must end with /api/v1 (e.g. ${DEFAULT_API_URL}), received "${val}"`,
      });
    }
  });

const networkSchema = z
  .enum(["testnet", "public"], {
    errorMap: () => ({
      message: `NEXT_PUBLIC_NETWORK must be either "testnet" or "public"`,
    }),
  })
  .default(DEFAULT_NETWORK);

export const envSchema = z.object({
  NEXT_PUBLIC_API_URL: apiUrlSchema,
  NEXT_PUBLIC_NETWORK: networkSchema,
  NEXT_PUBLIC_E2E: booleanFlagSchema("NEXT_PUBLIC_E2E"),
  NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE: booleanFlagSchema(
    "NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE"
  ),
  NEXT_PUBLIC_ANALYTICS_ENABLED: booleanFlagSchema(
    "NEXT_PUBLIC_ANALYTICS_ENABLED"
  ),
});

export type Env = z.infer<typeof envSchema>;

export type RawEnvInput = {
  NEXT_PUBLIC_API_URL?: string;
  NEXT_PUBLIC_NETWORK?: string;
  NEXT_PUBLIC_E2E?: string | boolean;
  NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE?: string | boolean;
  NEXT_PUBLIC_ANALYTICS_ENABLED?: string | boolean;
};

/**
 * Reads static process.env.NEXT_PUBLIC_* references so Next.js can inline them
 * at build time, then validates and returns the typed environment config.
 */
export function parseEnv(input?: RawEnvInput): Env {
  const raw: RawEnvInput = input ?? {
    NEXT_PUBLIC_API_URL: process.env.NEXT_PUBLIC_API_URL,
    NEXT_PUBLIC_NETWORK: process.env.NEXT_PUBLIC_NETWORK,
    NEXT_PUBLIC_E2E: process.env.NEXT_PUBLIC_E2E,
    NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE:
      process.env.NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE,
    NEXT_PUBLIC_ANALYTICS_ENABLED: process.env.NEXT_PUBLIC_ANALYTICS_ENABLED,
  };

  const result = envSchema.safeParse(raw);
  if (!result.success) {
    const details = result.error.issues
      .map((issue) => {
        const key = issue.path.join(".") || "env";
        return `  - ${key}: ${issue.message}`;
      })
      .join("\n");
    throw new Error(`Invalid environment configuration:\n${details}`);
  }

  return result.data;
}

export const env: Env = parseEnv();
