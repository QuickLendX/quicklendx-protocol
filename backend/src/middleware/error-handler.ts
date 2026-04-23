import { Request, Response, NextFunction } from "express";

export interface AppError extends Error {
  status?: number;
  code?: string;
  details?: any;
}

export const errorHandler = (
  err: AppError,
  req: Request,
  res: Response,
  next: NextFunction
) => {
  const status = err.status || 500;
  const message = err.message || "Internal Server Error";
  const code = err.code || "INTERNAL_ERROR";

  // Log error (in production you'd use a logger like winston)
  console.error(`[Error] ${code}: ${message}`, err.details || "");

  res.status(status).json({
    error: {
      message,
      code,
      details: process.env.NODE_ENV === "development" ? err.details : undefined,
    },
  });
};
