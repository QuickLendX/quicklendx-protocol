import { Request, Response, NextFunction } from "express";

export interface RequestLimitsConfig {
  bodyLimit: string;
  queryParamLimit: number;
  totalQueryLimit: number;
  headerPerKeyLimit: number;
  totalHeadersLimit: number;
}

const DEFAULT_CONFIG: RequestLimitsConfig = {
  bodyLimit: "1mb",
  queryParamLimit: 2 * 1024,
  totalQueryLimit: 8 * 1024,
  headerPerKeyLimit: 16 * 1024,
  totalHeadersLimit: 64 * 1024,
};

export function createRequestLimitsMiddleware(
  config: Partial<RequestLimitsConfig> = {}
) {
  const limits = { ...DEFAULT_CONFIG, ...config };

  return (req: Request, res: Response, next: NextFunction) => {
    if (req.body && typeof req.body === "object") {
      const bodySize = Buffer.byteLength(JSON.stringify(req.body), "utf8");
      const limitBytes = parseLimitToBytes(limits.bodyLimit);
      if (bodySize > limitBytes) {
        return res.status(413).json({
          error: {
            message: "Request body too large",
            code: "BODY_LIMIT_EXCEEDED",
          },
        });
      }
    }

    if (req.query && Object.keys(req.query).length > 0) {
      let totalQuerySize = 0;
      for (const [key, value] of Object.entries(req.query)) {
        const keySize = Buffer.byteLength(key, "utf8");
        const valueSize = Buffer.byteLength(
          Array.isArray(value) ? value.join(",") : String(value ?? ""),
          "utf8"
        );
        totalQuerySize += keySize + valueSize;

        if (keySize + valueSize > limits.queryParamLimit) {
          return res.status(400).json({
            error: {
              message: `Query parameter '${key}' exceeds size limit`,
              code: "QUERY_PARAM_LIMIT_EXCEEDED",
            },
          });
        }
      }

      if (totalQuerySize > limits.totalQueryLimit) {
        return res.status(400).json({
          error: {
            message: "Total query string size exceeds limit",
            code: "QUERY_TOTAL_LIMIT_EXCEEDED",
          },
        });
      }
    }

    if (req.headers && Object.keys(req.headers).length > 0) {
      let totalHeadersSize = 0;

      for (const [key, value] of Object.entries(req.headers)) {
        const valueStr = Array.isArray(value) ? value.join(",") : String(value ?? "");
        const headerSize = Buffer.byteLength(valueStr, "utf8");

        if (headerSize > limits.headerPerKeyLimit) {
          return res.status(431).json({
            error: {
              message: `Header '${key}' exceeds size limit`,
              code: "HEADER_LIMIT_EXCEEDED",
            },
          });
        }

        totalHeadersSize += headerSize;
      }

      if (totalHeadersSize > limits.totalHeadersLimit) {
        return res.status(431).json({
          error: {
            message: "Total headers size exceeds limit",
            code: "HEADERS_TOTAL_LIMIT_EXCEEDED",
          },
        });
      }
    }

    next();
  };
}

function parseLimitToBytes(limit: string): number {
  const match = limit.match(/^(\d+)(mb|kb|b)?$/i);
  if (!match) {
    return 1024 * 1024;
  }
  const value = parseInt(match[1], 10);
  const unit = (match[2] || "b").toLowerCase();
  switch (unit) {
    case "mb":
      return value * 1024 * 1024;
    case "kb":
      return value * 1024;
    default:
      return value;
  }
}

export const requestLimitsMiddleware = createRequestLimitsMiddleware();
