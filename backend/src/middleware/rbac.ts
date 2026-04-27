import { NextFunction, Request, Response } from "express";
import { auditLogService } from "../services/auditLogService";
import { AdminRole } from "../types/rbac";

export interface AdminContext {
  role: AdminRole;
  envName: string;
}

export type RequestWithAdminContext = Request & {
  adminContext?: AdminContext;
};

interface TokenConfig {
  role: AdminRole;
  envName: string;
  token: string;
}

interface RoleResolution {
  configured: boolean;
  misconfigured: boolean;
  reason?: string;
  tokens: Map<string, AdminContext>;
}

const TOKEN_ENV_CONFIG: ReadonlyArray<{ role: AdminRole; envName: string }> = [
  { role: "support", envName: "QLX_SUPPORT_TOKEN" },
  { role: "operations_admin", envName: "QLX_OPERATIONS_TOKEN" },
  { role: "super_admin", envName: "QLX_SUPER_ADMIN_TOKEN" },
];

function getClientIp(req: Request): string {
  return req.ip || "unknown";
}

function buildRoleResolution(): RoleResolution {
  const configuredTokens: TokenConfig[] = TOKEN_ENV_CONFIG.map(
    ({ role, envName }) => ({
      role,
      envName,
      token: process.env[envName]?.trim() || "",
    }),
  ).filter((entry) => entry.token.length > 0);

  if (configuredTokens.length === 0) {
    return {
      configured: false,
      misconfigured: false,
      tokens: new Map<string, AdminContext>(),
    };
  }

  const duplicateToken = configuredTokens.find((candidate, index) => {
    return (
      configuredTokens.findIndex((entry) => entry.token === candidate.token) !==
      index
    );
  });

  if (duplicateToken) {
    return {
      configured: true,
      misconfigured: true,
      reason: `Duplicate admin token configured for ${duplicateToken.envName}`,
      tokens: new Map<string, AdminContext>(),
    };
  }

  const tokens = new Map<string, AdminContext>();
  for (const entry of configuredTokens) {
    tokens.set(entry.token, {
      role: entry.role,
      envName: entry.envName,
    });
  }

  return {
    configured: true,
    misconfigured: false,
    tokens,
  };
}

function extractBearerToken(req: Request): string | null {
  const authorizationHeader = req.headers.authorization;
  if (!authorizationHeader) {
    return null;
  }

  const [scheme, token] = authorizationHeader.split(" ");
  if (scheme !== "Bearer" || !token?.trim()) {
    return null;
  }

  return token.trim();
}

function sendError(
  res: Response,
  status: number,
  message: string,
  code: string,
): Response {
  return res.status(status).json({
    error: {
      message,
      code,
    },
  });
}

export function requireAdminRoles(
  allowedRoles: readonly AdminRole[],
  action: string,
) {
  return (req: Request, res: Response, next: NextFunction) => {
    const resolution = buildRoleResolution();

    if (!resolution.configured) {
      auditLogService.recordAuthorization({
        action,
        outcome: "denied",
        role: "anonymous",
        method: req.method,
        path: req.path,
        ip: getClientIp(req),
        reason: "rbac_not_configured",
      });
      return sendError(
        res,
        503,
        "Admin access is disabled until RBAC tokens are configured.",
        "RBAC_NOT_CONFIGURED",
      );
    }

    if (resolution.misconfigured) {
      auditLogService.recordAuthorization({
        action,
        outcome: "denied",
        role: "anonymous",
        method: req.method,
        path: req.path,
        ip: getClientIp(req),
        reason: resolution.reason,
      });
      return sendError(
        res,
        500,
        "RBAC configuration is invalid. Duplicate or conflicting credentials detected.",
        "RBAC_MISCONFIGURED",
      );
    }

    const token = extractBearerToken(req);
    if (!token) {
      auditLogService.recordAuthorization({
        action,
        outcome: "denied",
        role: "anonymous",
        method: req.method,
        path: req.path,
        ip: getClientIp(req),
        reason: "missing_bearer_token",
      });
      return sendError(
        res,
        401,
        "Bearer authentication is required for admin endpoints.",
        "AUTH_REQUIRED",
      );
    }

    const adminContext = resolution.tokens.get(token);
    if (!adminContext) {
      auditLogService.recordAuthorization({
        action,
        outcome: "denied",
        role: "anonymous",
        method: req.method,
        path: req.path,
        ip: getClientIp(req),
        reason: "invalid_admin_token",
      });
      return sendError(
        res,
        403,
        "The provided admin credential is not valid for this environment.",
        "FORBIDDEN",
      );
    }

    if (!allowedRoles.includes(adminContext.role)) {
      auditLogService.recordAuthorization({
        action,
        outcome: "denied",
        role: adminContext.role,
        method: req.method,
        path: req.path,
        ip: getClientIp(req),
        reason: "insufficient_role",
      });
      return sendError(
        res,
        403,
        "The authenticated admin role does not have access to this action.",
        "INSUFFICIENT_ROLE",
      );
    }

    (req as RequestWithAdminContext).adminContext = adminContext;
    auditLogService.recordAuthorization({
      action,
      outcome: "allowed",
      role: adminContext.role,
      method: req.method,
      path: req.path,
      ip: getClientIp(req),
    });
    next();
  };
}

export function getAdminContext(req: Request): AdminContext {
  const adminContext = (req as RequestWithAdminContext).adminContext;
  if (!adminContext) {
    throw new Error("Admin context is not available on this request.");
  }
  return adminContext;
}
