import { Request, Response, NextFunction } from "express";

const ADMIN_TOKEN_ENV_KEY = "ADMIN_API_TOKEN";

export function requireAdminAuth(req: Request, res: Response, next: NextFunction): void {
  const configuredToken = process.env[ADMIN_TOKEN_ENV_KEY];
  if (!configuredToken) {
    res.status(503).json({
      error: "Admin endpoints are disabled until ADMIN_API_TOKEN is configured",
      code: "ADMIN_AUTH_NOT_CONFIGURED",
    });
    return;
  }

  const authHeader = req.header("authorization");
  if (!authHeader || !authHeader.startsWith("Bearer ")) {
    res.status(401).json({ error: "Unauthorized", code: "UNAUTHORIZED" });
    return;
  }

  const presentedToken = authHeader.slice("Bearer ".length).trim();
  if (presentedToken !== configuredToken) {
    res.status(403).json({ error: "Forbidden", code: "FORBIDDEN" });
    return;
  }

  next();
}

export function getAdminActor(req: Request): string {
  return req.header("x-admin-actor") || "admin";
}
