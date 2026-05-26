import { Request, Response, NextFunction } from "express";
import { RateLimiterMemory, RateLimiterRes } from "rate-limiter-flexible";

/**
 * Rate limiter configuration
 * 
 * Default: 100 requests per 60 seconds
 * Test environment: 1000 requests per 60 seconds
 */
export const rateLimiter = new RateLimiterMemory({
  points: process.env.NODE_ENV === "test" ? 1000 : 100,
  duration: 60,
  blockDuration: 60, // Block for 60 seconds if consumed more than points
});

/**
 * Set rate limit headers on response
 */
const setRateLimitHeaders = (res: Response, rateLimiterRes: RateLimiterRes) => {
  res.setHeader("X-RateLimit-Limit", rateLimiter.points);
  res.setHeader("X-RateLimit-Remaining", rateLimiterRes.remainingPoints);
  res.setHeader("X-RateLimit-Reset", new Date(Date.now() + rateLimiterRes.msBeforeNext).toISOString());
};

/**
 * Global rate limit middleware
 * 
 * Applies to all public endpoints. Returns 429 Too Many Requests if exceeded.
 * Includes Retry-After header as per security guidelines.
 */
export const rateLimitMiddleware = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  const ip = req.ip || req.headers["x-forwarded-for"] || "unknown";
  
  try {
    const rateLimiterRes = await rateLimiter.consume(String(ip));
    setRateLimitHeaders(res, rateLimiterRes);
    next();
  } catch (rejRes) {
    if (rejRes instanceof RateLimiterRes) {
      setRateLimitHeaders(res, rejRes);
      res.setHeader("Retry-After", Math.ceil(rejRes.msBeforeNext / 1000));
      res.status(429).json({
        error: {
          message: "Too many requests",
          code: "RATE_LIMIT_EXCEEDED",
          retryAfter: Math.ceil(rejRes.msBeforeNext / 1000),
        },
      });
    } else {
      // Fallback for unexpected errors
      res.status(500).json({
        error: {
          message: "Internal server error during rate limiting",
          code: "RATE_LIMIT_ERROR",
        },
      });
    }
  }
};

/**
 * Factory to create specialized rate limiters for sensitive endpoints
 * (e.g., Auth, KYC, Webhooks)
 */
export const createRateLimitMiddleware = (customLimiter: RateLimiterMemory) => {
  return async (req: Request, res: Response, next: NextFunction) => {
    const ip = req.ip || req.headers["x-forwarded-for"] || "unknown";
    try {
      const rateLimiterRes = await customLimiter.consume(String(ip));
      res.setHeader("X-RateLimit-Limit", customLimiter.points);
      res.setHeader("X-RateLimit-Remaining", rateLimiterRes.remainingPoints);
      res.setHeader("X-RateLimit-Reset", new Date(Date.now() + rateLimiterRes.msBeforeNext).toISOString());
      next();
    } catch (rejRes) {
      if (rejRes instanceof RateLimiterRes) {
        res.setHeader("Retry-After", Math.ceil(rejRes.msBeforeNext / 1000));
        res.status(429).json({
          error: {
            message: "Sensitive endpoint rate limit exceeded",
            code: "STRICT_RATE_LIMIT_EXCEEDED",
            retryAfter: Math.ceil(rejRes.msBeforeNext / 1000),
          },
        });
      } else {
        next(); // Fallback to next if something breaks
      }
    }
  };
};

/**
 * Strict rate limiter for sensitive endpoints
 * 5 requests per minute
 */
export const strictRateLimiter = new RateLimiterMemory({
  points: process.env.NODE_ENV === "test" ? 100 : 5,
  duration: 60,
  blockDuration: 300, // Block for 5 minutes
});

export const strictRateLimitMiddleware = createRateLimitMiddleware(strictRateLimiter);
