import { NextFunction, Request, Response } from "express";
import { allowedBrowserOrigins } from "../config/cors";

const STATE_CHANGING_METHODS = new Set(["POST", "PUT", "PATCH", "DELETE"]);
const JSON_CONTENT_TYPE = "application/json";

const isAllowedOrigin = (originHeader: string | undefined): boolean => {
  if (!originHeader) {
    return true;
  }

  return allowedBrowserOrigins.includes(originHeader);
};

const isJsonContentType = (contentType: string | undefined): boolean => {
  if (!contentType) {
    return false;
  }

  return contentType.toLowerCase().startsWith(JSON_CONTENT_TYPE);
};

export const csrfMiddleware = (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  if (!STATE_CHANGING_METHODS.has(req.method)) {
    next();
    return;
  }

  const originHeader =
    typeof req.headers.origin === "string" ? req.headers.origin : undefined;
  if (!isAllowedOrigin(originHeader)) {
    res.status(403).json({
      error: {
        message: "Request origin is not allowed",
        code: "ORIGIN_NOT_ALLOWED",
      },
    });
    return;
  }

  const contentTypeHeader =
    typeof req.headers["content-type"] === "string"
      ? req.headers["content-type"]
      : undefined;

  if (!isJsonContentType(contentTypeHeader)) {
    res.status(415).json({
      error: {
        message: "Unsupported Media Type. Use application/json.",
        code: "INVALID_CONTENT_TYPE",
      },
    });
    return;
  }

  next();
};
