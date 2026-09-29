import { Request, Response, NextFunction } from "express";
import { auditService } from "../services/auditService";
import { redactSensitiveFields, AuditOperation } from "../types/audit";
import { AuthenticatedRequest } from "./apiKeyAuth";

export interface AuditContext {
  operation: AuditOperation;
  describeEffect: (params: Record<string, unknown>, res: Response) => string;
}

const AUDIT_ROUTES: { [route: string]: AuditContext } = {
  "POST:/maintenance": {
    operation: "MAINTENANCE_MODE",
    describeEffect: (params) =>
      `Maintenance mode set to ${params["enabled"] ?? "unknown"}`,
  },
  "POST:/webhook/rotate": {
    operation: "WEBHOOK_SECRET_ROTATE",
    describeEffect: (params) =>
      `Webhook secret rotated for keyId: ${params["keyId"] ?? "unknown"}`,
  },
  "POST:/config": {
    operation: "CONFIG_CHANGE",
    describeEffect: (params) =>
      `Config updated: ${JSON.stringify(params["key"] ?? "")} = ${JSON.stringify(params["value"] ?? "")}`,
  },
  "POST:/backfill": {
    operation: "BACKFILL_START",
    describeEffect: (params) =>
      `Backfill started: entity=${params["entity"] ?? "unknown"}, fromLedger=${params["fromLedger"] ?? "unknown"}`,
  },
  "POST:/backfill/abort": {
    operation: "BACKFILL_ABORT",
    describeEffect: (params) =>
      `Backfill aborted: jobId=${params["jobId"] ?? "unknown"}`,
  },
  "POST:/keys": {
    operation: "ADMIN_API_KEY_ADD",
    describeEffect: (params) =>
      `API key added for actor: ${params["actor"] ?? "unknown"}`,
  },
  "DELETE:/keys": {
    operation: "ADMIN_API_KEY_REVOKE",
    describeEffect: (params) =>
      `API key revoked for actor: ${params["actor"] ?? "unknown"}`,
  },
};

function getAuditContext(req: Request): AuditContext | undefined {
  if (!req) return undefined;
  const method = (req.method || "").toUpperCase();
  const path = req.path || "";
  const origPath = req.originalUrl ? req.originalUrl.split("?")[0] : "";

  return (
    AUDIT_ROUTES[`${method}:${path}`] ||
    AUDIT_ROUTES[`${method}:${origPath}`]
  );
}

function getClientIp(req: Request): string {
  if (!req || !req.headers) return "unknown";
  const xff = req.headers["x-forwarded-for"];
  const xffStr = Array.isArray(xff) ? xff[0] : xff;
  if (xffStr && typeof xffStr === "string") {
    const ip = xffStr.split(",")[0]?.trim();
    if (ip) return ip;
  }
  const xRealIp = req.headers["x-real-ip"];
  if (xRealIp && typeof xRealIp === "string") {
    return xRealIp.trim();
  }
  return req.socket?.remoteAddress || "unknown";
}

export function auditMiddleware(
  req: AuthenticatedRequest,
  res: Response,
  next: NextFunction
): void {
  try {
    const ctx = getAuditContext(req);
    if (!ctx) {
      next();
      return;
    }

    let logged = false;

    const recordAuditEntry = (body?: unknown) => {
      if (logged) return;
      logged = true;

      try {
        const rawParams =
          req?.body && typeof req.body === "object" && !Array.isArray(req.body)
            ? (req.body as Record<string, unknown>)
            : {};

        let effect = "unknown";
        try {
          effect = ctx.describeEffect(rawParams, res);
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          effect = `[Effect description failed: ${msg}]`;
        }

        const statusCode = res.statusCode || 200;
        const success = statusCode >= 200 && statusCode < 400;

        let errorMessage: string | undefined = undefined;
        if (!success) {
          if (typeof body === "object" && body !== null) {
            const b = body as Record<string, unknown>;
            if (b.error && typeof b.error === "object" && (b.error as Record<string, unknown>).message) {
              errorMessage = String((b.error as Record<string, unknown>).message);
            } else if (typeof b.error === "string") {
              errorMessage = b.error;
            } else if (typeof b.message === "string") {
              errorMessage = b.message;
            }
          } else if (typeof body === "string" && body.trim().length > 0) {
            errorMessage = body;
          }
        }

        const actor = (req && req.actor) || "unknown";
        const userAgent = (req && req.headers && req.headers["user-agent"]) || "unknown";

        auditService.append({
          actor,
          operation: ctx.operation,
          params: rawParams,
          redactedParams: redactSensitiveFields(rawParams),
          ip: getClientIp(req),
          userAgent: typeof userAgent === "string" ? userAgent : "unknown",
          effect,
          success,
          errorMessage,
        });
      } catch (err) {
        console.error("[Audit] Failed to write audit entry:", err);
      }
    };

    const originalJson = res.json.bind(res);
    res.json = function (body: unknown): Response {
      recordAuditEntry(body);
      return originalJson(body);
    };

    const originalSend = res.send.bind(res);
    res.send = function (body?: unknown): Response {
      recordAuditEntry(body);
      return originalSend(body);
    };

    res.once("finish", () => {
      recordAuditEntry();
    });

    next();
  } catch (err) {
    console.error("[Audit] Error in auditMiddleware setup:", err);
    next();
  }
}

export function registerAuditRoute(
  method: string,
  path: string,
  ctx: AuditContext
): void {
  AUDIT_ROUTES[`${method.toUpperCase()}:${path}`] = ctx;
}