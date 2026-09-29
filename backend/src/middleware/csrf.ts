import { NextFunction, Request, Response } from "express";
import { allowedBrowserOrigins } from "../config/cors";

const STATE_CHANGING_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);
const JSON_CONTENT_TYPE = "application/json";

const isAllowedOrigin = (originHeader: string | undefined): boolean => {
  if (!originHeader) {
    return true;
  }

  return (
    allowedBrowserOrigins.includes("*") ||
    allowedBrowserOrigins.includes(originHeader)
  );
};

const isJsonContentType = (contentType: string | undefined): boolean => {
  if (!contentType) {
    return false;
  }

  return contentType.toLowerCase().startsWith(JSON_CONTENT_TYPE);
};

export const csrfMiddleware = (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  if (!STATE_CHANGING_METHODS.has(req.method)) {
    next();
    return;
  }

  // Exempt machine-to-machine webhook ingress paths
  const path = req.path || "";
  if (
    path.startsWith("/api/webhooks") ||
    path.includes("/webhooks/ingest")
  ) {
    next();
    return;
  }

  // Exempt API-key authenticated requests (admin/keys or general api-keys)
  const authHeader = req.headers["authorization"];
  const hasApiKeyHeader = req.headers["x-api-key"] !== undefined;
  const hasBearerApiKey = typeof authHeader === "string" && authHeader.startsWith("Bearer qlx_");

  if (hasApiKeyHeader || hasBearerApiKey) {
    next();
    return;
  }

  // Browser-driven write requests validation:

  // 1. Verify Origin header if present
  const originHeader =
    typeof req.headers.origin === "string" ? req.headers.origin : undefined;
  if (!isAllowedOrigin(originHeader)) {
    res.status(403).json({
      error: {
        message: "Request origin is not allowed",
        code: "ORIGIN_NOT_ALLOWED",
      },
    });
    return;
  }

  // 2. Verify custom CSRF token header
  const csrfToken = req.headers["x-csrf-token"];
  if (!csrfToken) {
    res.status(403).json({
      error: {
        message: "Missing CSRF token",
        code: "MISSING_CSRF_TOKEN",
      },
    });
    return;
  }

  // 3. Verify Content-Type is application/json
  const contentTypeHeader =
    typeof req.headers["content-type"] === "string"
      ? req.headers["content-type"]
      : undefined;

  if (!isJsonContentType(contentTypeHeader)) {
    res.status(415).json({
      error: {
        message: "Unsupported Media Type. Use application/json.",
        code: "INVALID_CONTENT_TYPE",
      },
    });
    return;
  }

  next();
};

