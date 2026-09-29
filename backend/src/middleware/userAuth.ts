import { Request, Response, NextFunction } from "express";
import { RequestWithUser } from "../types/auth";

/**
 * Middleware to require user authentication.
 * In this implementation, we expect a Bearer token which is simply the User ID (Stellar address).
 * In a production environment, this should be a JWT or a verified Soroban signature.
 */
export function requireUserAuth(req: Request, res: Response, next: NextFunction): void {
  const authHeader = req.header("authorization");

  if (!authHeader || !authHeader.startsWith("Bearer ")) {
    res.status(401).json({
      error: {
        message: "Authentication required. Provide 'Authorization: Bearer <user_id>' header.",
        code: "UNAUTHORIZED",
      },
    });
    return;
  }

  const userId = authHeader.slice("Bearer ".length).trim();

  if (!userId) {
    res.status(401).json({
      error: {
        message: "Invalid authentication token.",
        code: "UNAUTHORIZED",
      },
    });
    return;
  }

  (req as RequestWithUser).user = { userId };
  next();
}

export function getUser(req: Request): string {
  const user = (req as RequestWithUser).user;
  if (!user) {
    throw new Error("User context not available. Ensure requireUserAuth middleware is used.");
  }
  return user.userId;
}
