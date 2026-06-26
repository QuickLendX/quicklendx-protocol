import { Request, Response, NextFunction } from "express";

const MAX_BODY_SIZE = 256 * 1024; // 256 KB
const ALLOWLISTED_PROXY_HEADER = "X-Allow-Chunked-Encoding"; // Example header, adjust as needed

export function eventIngestLimitsMiddleware(req: Request, res: Response, next: NextFunction) {
  // Check Content-Type
  const contentType = req.headers["content-type"];
  if (!contentType || !contentType.includes("application/json")) {
    return res.status(415).json({
      error: {
        message: "Unsupported content type, must be application/json",
        code: "INVALID_CONTENT_TYPE",
      },
    });
  }

  // Check Content-Length
  const contentLength = req.headers["content-length"];
  if (!contentLength) {
    return res.status(411).json({
      error: {
        message: "Content-Length header is required",
        code: "CONTENT_LENGTH_REQUIRED",
      },
    });
  }

  const contentLengthNum = parseInt(contentLength, 10);
  if (isNaN(contentLengthNum) || contentLengthNum > MAX_BODY_SIZE) {
    return res.status(413).json({
      error: {
        message: `Request body too large, maximum is ${MAX_BODY_SIZE} bytes`,
        code: "BODY_LIMIT_EXCEEDED",
      },
    });
  }

  // Check Transfer-Encoding
  const transferEncoding = req.headers["transfer-encoding"];
  const allowChunked = req.headers[ALLOWLISTED_PROXY_HEADER.toLowerCase()] !== undefined;
  if (transferEncoding && transferEncoding.includes("chunked") && !allowChunked) {
    return res.status(400).json({
      error: {
        message: "Chunked transfer encoding is not allowed",
        code: "CHUNKED_ENCODING_NOT_ALLOWED",
      },
    });
  }

  next();
}
