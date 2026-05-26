import { CorsOptions } from "cors";

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

  return allowedBrowserOrigins.includes(origin);
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
  allowedHeaders: ["Content-Type", "Authorization"],
  credentials: true,
  optionsSuccessStatus: 204,
};

export const webhookCorsOptions: CorsOptions = {
  origin: "*",
  methods: ["POST", "OPTIONS"],
  allowedHeaders: ["Content-Type", "X-Webhook-Signature"],
  credentials: false,
  optionsSuccessStatus: 204,
};
