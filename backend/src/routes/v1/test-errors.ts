import { Router } from "express";
import { createErrorHandler } from "../../middleware/error-handler";

const router = Router();

router.get("/500", (req, res, next) => {
  const err: any = new Error("Test Server Error");
  err.status = 500;
  err.code = "TEST_ERROR";
  next(err);
});

router.get("/no-message", (req, res, next) => {
  const err: any = new Error();
  delete err.message;
  next(err);
});

/**
 * Previously mutated process.env.NODE_ENV = 'development' to force detail
 * exposure — a global race condition. Now uses the injectable exposeDetails
 * option on createErrorHandler so no env mutation is needed.
 */
router.get("/development", (req, res, next) => {
  const err: any = new Error("Dev Error");
  err.details = { foo: "bar" };
  next(err);
});

router.get("/default-error", (req, res, next) => {
  next(new Error("Default Error"));
});

// All routes in this router expose details — this router is only mounted in
// non-production environments (gated in routes/v1/index.ts).
router.use(createErrorHandler({ exposeDetails: true }));

export default router;
