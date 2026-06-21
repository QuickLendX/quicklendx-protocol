import { Request, Response, NextFunction } from "express";
import { z } from "zod";

export interface ValidationError {
  message: string;
  code: string;
  details?: unknown;
}

export function formatZodError(error: z.ZodError, includeDetails = true): ValidationError {
  return {
    message: "Validation failed",
    code: "VALIDATION_ERROR",
    details: includeDetails ? error.issues.map((issue) => ({
      field: issue.path.join("."),
      message: issue.message,
      code: issue.code,
    })) : undefined,
  };
}

export function sanitizeInput(value: string): string {
  return value
    .replace(/[<>'"]/g, "")
    .replace(/javascript:/gi, "")
    .replace(/on\w+=/gi, "")
    .trim();
}

function sanitizeParamValue(value: string | string[]): string {
  const strValue = Array.isArray(value) ? value[0] ?? "" : String(value ?? "");
  return sanitizeInput(strValue);
}

function getValidatedQuery(schema: z.ZodType, query: Record<string, unknown>): Record<string, unknown> {
  const rawQuery: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(query)) {
    if (typeof value === "string") {
      rawQuery[key] = sanitizeInput(value);
    } else if (Array.isArray(value)) {
      rawQuery[key] = value.map((v) =>
        typeof v === "string" ? sanitizeInput(v) : v
      );
    } else {
      rawQuery[key] = value;
    }
  }
  return schema.parse(rawQuery) as Record<string, unknown>;
}

function getValidatedParams(schema: z.ZodType, params: Record<string, string>): Record<string, string> {
  const rawParams: Record<string, string> = {};
  for (const [key, value] of Object.entries(params)) {
    rawParams[key] = sanitizeParamValue(value);
  }
  return schema.parse(rawParams) as Record<string, string>;
}

function sendValidationError(res: Response, error: z.ZodError) {
  const includeDetails = process.env.NODE_ENV === "development";
  res.status(400).json({
    error: formatZodError(error, includeDetails),
  });
}

export function createValidationMiddleware(
  schemas: {
    body?: z.ZodType;
    query?: z.ZodType;
    params?: z.ZodType;
  }
) {
  return (req: Request, res: Response, next: NextFunction) => {
    try {
      if (schemas.body) {
        req.body = schemas.body.parse(req.body);
      }

      if (schemas.query) {
        const validated = getValidatedQuery(schemas.query, req.query as Record<string, unknown>);
        Object.assign(req.query, validated);
      }

      if (schemas.params) {
        const validated = getValidatedParams(schemas.params, req.params as Record<string, string>);
        Object.assign(req.params, validated);
      }

      next();
    } catch (error) {
      if (error instanceof z.ZodError) {
        return sendValidationError(res, error);
      }
      next(error);
    }
  };
}

export function createQueryValidationMiddleware(schema: z.ZodType) {
  return (req: Request, res: Response, next: NextFunction) => {
    try {
      const validated = getValidatedQuery(schema, req.query as Record<string, unknown>);
      Object.assign(req.query, validated);
      next();
    } catch (error) {
      if (error instanceof z.ZodError) {
        return sendValidationError(res, error);
      }
      next(error);
    }
  };
}

export function createBodyValidationMiddleware(schema: z.ZodType) {
  return (req: Request, res: Response, next: NextFunction) => {
    try {
      req.body = schema.parse(req.body);
      next();
    } catch (error) {
      if (error instanceof z.ZodError) {
        return sendValidationError(res, error);
      }
      next(error);
    }
  };
}

export function createParamsValidationMiddleware(schema: z.ZodType) {
  return (req: Request, res: Response, next: NextFunction) => {
    try {
      const validated = getValidatedParams(schema, req.params as Record<string, string>);
      Object.assign(req.params, validated);
      next();
    } catch (error) {
      if (error instanceof z.ZodError) {
        return sendValidationError(res, error);
      }
      next(error);
    }
  };
}
