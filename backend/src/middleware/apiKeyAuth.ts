import { Request, Response, NextFunction } from "express";

export interface AuthenticatedRequest extends Request {
  actor?: string;
}

interface KeyMap {
  [key: string]: string;
}

let keyMap: KeyMap = {};
let isLoaded: boolean = false;

export function loadApiKeys(): void {
  try {
    const envValue = process.env.ADMIN_API_KEYS || "";
    keyMap = {};
    for (const entry of envValue.split(",")) {
      const trimmed = entry.trim();
      if (!trimmed) continue;
      const colonIdx = trimmed.indexOf(":");
      if (colonIdx > 0) {
        const key = trimmed.slice(0, colonIdx).trim();
        const actor = trimmed.slice(colonIdx + 1).trim();
        if (key && actor) {
          keyMap[key] = actor;
        }
      }
    }
    isLoaded = true;
  } catch (_err) {
    keyMap = {};
    isLoaded = true;
  }
}

export function apiKeyAuth(
  req: AuthenticatedRequest,
  res: Response,
  next: NextFunction
): void {
  try {
    if (process.env.SKIP_API_KEY_AUTH === "true") {
      req.actor = process.env.TEST_ACTOR || "test-actor";
      next();
      return;
    }

    if (!isLoaded) {
      loadApiKeys();
    }

    const rawKey = req.header ? req.header("X-API-Key") : undefined;
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
  } catch (_err) {
    res.status(500).json({
      error: {
        message: "Internal authentication error",
        code: "INTERNAL_ERROR",
      },
    });
  }
}

export function resetApiKeys(): void {
  keyMap = {};
  isLoaded = false;
}

export function getKeyMapSize(): number {
  return Object.keys(keyMap).length;
}