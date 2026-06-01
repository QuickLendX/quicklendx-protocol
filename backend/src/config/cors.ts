import { CorsOptions } from "cors";
import { Request } from "express";

const parseAllowedOrigins = (raw: string | undefined): string[] => {
  if (!raw) {
    return [];
  }

  return raw
    .split(",")
    .map((origin) => origin.trim())
    .filter((origin) => origin.length > 0);
};

export const allowedBrowserOrigins = parseAllowedOrigins(
  process.env.ALLOWED_ORIGINS
);

const isAllowedBrowserOrigin = (origin?: string): boolean => {
  if (!origin) {
    return true;
  }

  return (
    allowedBrowserOrigins.includes("*") ||
    allowedBrowserOrigins.includes(origin)
  );
};

export const browserCorsOptions: CorsOptions = {
  origin: (origin, callback) => {
    if (isAllowedBrowserOrigin(origin)) {
      callback(null, true);
      return;
    }

    callback(null, false);
  },
  methods: ["GET", "POST", "PUT", "PATCH", "DELETE", "OPTIONS"],
  allowedHeaders: ["Content-Type", "Authorization", "X-CSRF-Token"],
  credentials: true,
  optionsSuccessStatus: 204,
};

export const webhookCorsOptions: CorsOptions = {
  origin: "*",
  methods: ["POST", "OPTIONS"],
  allowedHeaders: ["Content-Type", "X-Webhook-Signature", "X-Webhook-Subscriber-Id"],
  credentials: false,
  optionsSuccessStatus: 204,
};

/**
 * Dynamic CORS options delegate that chooses between webhook and browser options
 * depending on the request path.
 */
export const corsOptionsDelegate = (
  req: Request,
  callback: (err: Error | null, options?: CorsOptions) => void
): void => {
  const path = req.path || "";
  if (path.startsWith("/api/webhooks") || path.includes("/webhooks/ingest")) {
    callback(null, webhookCorsOptions);
  } else {
    callback(null, browserCorsOptions);
  }
};

