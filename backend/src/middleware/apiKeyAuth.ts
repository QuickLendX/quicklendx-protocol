import { Request, Response, NextFunction } from "express";

export interface AuthenticatedRequest extends Request {
  actor?: string;
}

interface KeyMap {
  [key: string]: string;
}

let keyMap: KeyMap = {};

export function loadApiKeys(): void {
  const envValue = process.env.ADMIN_API_KEYS || "";
  keyMap = {};
  for (const entry of envValue.split(",")) {
    const [key, actor] = entry.trim().split(":");
    if (key && actor) {
      keyMap[key] = actor;
    }
  }
}

export function apiKeyAuth(
  req: AuthenticatedRequest,
  res: Response,
  next: NextFunction
): void {
  if (process.env.SKIP_API_KEY_AUTH === "true") {
    req.actor = process.env.TEST_ACTOR || "test-actor";
    next();
    return;
  }

  if (Object.keys(keyMap).length === 0) {
    loadApiKeys();
  }

  const rawKey = req.header("X-API-Key");
  if (!rawKey) {
    res.status(401).json({
      error: {
        message: "Missing X-API-Key header",
        code: "UNAUTHORIZED",
      },
    });
    return;
  }

  const actor = keyMap[rawKey];
  if (!actor) {
    res.status(401).json({
      error: {
        message: "Invalid API key",
        code: "UNAUTHORIZED",
      },
    });
    return;
  }

  req.actor = actor;
  next();
}

export function resetApiKeys(): void {
  keyMap = {};
}