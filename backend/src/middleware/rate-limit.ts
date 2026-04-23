import { Request, Response, NextFunction } from "express";
import { RateLimiterMemory } from "rate-limiter-flexible";

export const rateLimiter = new RateLimiterMemory({
  points: process.env.NODE_ENV === "test" ? 1000 : 100, 
  duration: 60,
});

export const rateLimitMiddleware = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    await rateLimiter.consume(req.ip || "unknown");
    next();
  } catch (rejRes) {
    res.status(429).json({
      error: {
        message: "Too many requests",
        code: "RATE_LIMIT_EXCEEDED",
      },
    });
  }
};
