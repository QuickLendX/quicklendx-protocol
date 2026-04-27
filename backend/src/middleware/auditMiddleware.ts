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
  const key = `${req.method}:${req.path}`;
  return AUDIT_ROUTES[key];
}

function getClientIp(req: Request): string {
  return (
    (req.headers["x-forwarded-for"] as string)?.split(",")[0]?.trim() ||
    (req.headers["x-real-ip"] as string) ||
    req.socket.remoteAddress ||
    "unknown"
  );
}

export function auditMiddleware(
  req: AuthenticatedRequest,
  res: Response,
  next: NextFunction
): void {
  const ctx = getAuditContext(req);
  if (!ctx) {
    next();
    return;
  }

  const originalJson = res.json.bind(res);
  const startTime = Date.now();
  let logged = false;

  res.json = function (
    body: unknown
  ): Response {
    if (logged) return originalJson(body);
    logged = true;

    const success = res.statusCode >= 200 && res.statusCode < 400;
    const effect = ctx.describeEffect(
      req.body as Record<string, unknown>,
      res
    );

    try {
let errorMsg: string | undefined = undefined;
        if (!success && typeof body === "object" && body !== null) {
          const msg = (body as { error?: { message?: string } }).error?.message;
          if (msg) errorMsg = msg;
        }

        auditService.append({
          actor: req.actor || "unknown",
          operation: ctx.operation,
          params: req.body as Record<string, unknown>,
          redactedParams: redactSensitiveFields(req.body as Record<string, unknown>),
          ip: getClientIp(req),
          userAgent: req.headers["user-agent"] || "unknown",
          effect,
          success,
          errorMessage: errorMsg,
        });
    } catch (err) {
      console.error("[Audit] Failed to write audit entry:", err);
    }

    return originalJson(body);
  };

  next();
}

export function registerAuditRoute(
  method: string,
  path: string,
  ctx: AuditContext
): void {
  AUDIT_ROUTES[`${method}:${path}`] = ctx;
}