import { Router, Request, Response, NextFunction } from "express";
import { webhookSecretService } from "../../services/webhookSecretService";

const router = Router();

/**
 * GET /api/v1/webhooks/jwks
 *
 * Public endpoint exposing the active set of Ed25519 public keys
 * in JWKS (JSON Web Key Set) format.
 */
router.get("/", (req: Request, res: Response, next: NextFunction): void => {
  try {
    const keys = webhookSecretService.getActiveJWKs();
    res.json({ keys });
  } catch (err) {
    next(err);
  }
});

export default router;
