import { Router } from "express";

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

router.get("/development", (req, res, next) => {
  const oldEnv = process.env.NODE_ENV;
  process.env.NODE_ENV = "development";
  const err: any = new Error("Dev Error");
  err.details = { foo: "bar" };
  next(err);
  process.env.NODE_ENV = oldEnv;
});

router.get("/default-error", (req, res, next) => {
  next(new Error("Default Error"));
});

export default router;
