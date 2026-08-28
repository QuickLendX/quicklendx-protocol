import express, { NextFunction, Request, RequestHandler, Response } from "express";

/**
 * Hardened ingestion limits for `POST /api/v1/events`.
 *
 * The endpoint is the only unauthenticated-ish ingress the indexer uses, so the
 * framing of the request is validated before a single byte of the payload is
 * buffered:
 *
 * 1. `eventIngestLimitsMiddleware` inspects headers only and rejects requests
 *    whose framing cannot be trusted (wrong content type, missing/ambiguous
 *    `Content-Length`, `Transfer-Encoding: chunked` from a non-allowlisted
 *    proxy).
 * 2. `eventIngestBodyParser` parses the body with a 256 KB budget that is
 *    independent from the 1 MB application-wide budget.
 *
 * No rejection ever echoes payload bytes: every message below is a constant.
 */

export const EVENT_INGEST_PATH = "/api/v1/events";
export const EVENT_INGEST_MAX_BODY_BYTES = 256 * 1024;

/** Header an allowlisted upstream proxy uses to declare itself. */
export const CHUNKED_PROXY_HEADER = "x-allow-chunked-encoding";

/** Comma-separated list of proxy identifiers permitted to forward chunked bodies. */
export const CHUNKED_PROXY_ALLOWLIST_ENV = "EVENT_INGEST_CHUNKED_PROXY_ALLOWLIST";

const JSON_MEDIA_TYPE = "application/json";
const CONTENT_LENGTH_PATTERN = /^\d+$/;

export interface EventIngestLimitsOptions {
  maxBodyBytes?: number;
  allowedChunkedProxies?: string[];
}

const readHeader = (raw: string | string[] | undefined): string | undefined => {
  if (Array.isArray(raw)) return raw.join(",");
  if (typeof raw !== "string") return undefined;
  return raw;
};

const parseAllowlist = (raw: string | undefined): string[] =>
  (raw ?? "")
    .split(",")
    .map((entry) => entry.trim())
    .filter((entry) => entry.length > 0);

const isJsonMediaType = (contentType: string | undefined): boolean => {
  if (!contentType) return false;
  const [mediaType] = contentType.split(";");
  return mediaType.trim().toLowerCase() === JSON_MEDIA_TYPE;
};

const reject = (
  res: Response,
  status: number,
  code: string,
  message: string
): void => {
  res.status(status).json({ error: { message, code } });
};

/**
 * Header-only guard. Must run before any body parser so oversized or ambiguous
 * requests are refused without buffering the payload.
 */
export const createEventIngestLimitsMiddleware = (
  options: EventIngestLimitsOptions = {}
): RequestHandler => {
  const maxBodyBytes = options.maxBodyBytes ?? EVENT_INGEST_MAX_BODY_BYTES;

  return (req: Request, res: Response, next: NextFunction): void => {
    const allowedProxies =
      options.allowedChunkedProxies ??
      parseAllowlist(process.env[CHUNKED_PROXY_ALLOWLIST_ENV]);

    if (!isJsonMediaType(readHeader(req.headers["content-type"]))) {
      reject(
        res,
        415,
        "INVALID_CONTENT_TYPE",
        "Unsupported media type. Use application/json."
      );
      return;
    }

    const contentLength = readHeader(req.headers["content-length"]);
    const transferEncoding = readHeader(req.headers["transfer-encoding"]);
    const isChunked = (transferEncoding ?? "")
      .toLowerCase()
      .split(",")
      .some((encoding) => encoding.trim() === "chunked");

    if (isChunked) {
      const proxyId = readHeader(req.headers[CHUNKED_PROXY_HEADER])?.trim();

      if (!proxyId || !allowedProxies.includes(proxyId)) {
        reject(
          res,
          400,
          "CHUNKED_ENCODING_NOT_ALLOWED",
          "Chunked transfer encoding is not accepted on this endpoint."
        );
        return;
      }

      // Both framing headers at once is the canonical request-smuggling
      // signature: intermediaries disagree on which one wins.
      if (contentLength !== undefined) {
        reject(
          res,
          400,
          "AMBIGUOUS_REQUEST_FRAMING",
          "Content-Length must not be combined with Transfer-Encoding: chunked."
        );
        return;
      }

      next();
      return;
    }

    if (contentLength === undefined) {
      reject(
        res,
        411,
        "CONTENT_LENGTH_REQUIRED",
        "Content-Length header is required."
      );
      return;
    }

    if (!CONTENT_LENGTH_PATTERN.test(contentLength.trim())) {
      reject(
        res,
        400,
        "INVALID_CONTENT_LENGTH",
        "Content-Length must be a single non-negative integer."
      );
      return;
    }

    if (Number(contentLength) > maxBodyBytes) {
      reject(
        res,
        413,
        "BODY_LIMIT_EXCEEDED",
        `Request body exceeds the ${maxBodyBytes} byte limit for this endpoint.`
      );
      return;
    }

    next();
  };
};

export const eventIngestLimitsMiddleware = createEventIngestLimitsMiddleware();

const defaultJsonParser = express.json({
  limit: EVENT_INGEST_MAX_BODY_BYTES,
  type: JSON_MEDIA_TYPE,
  verify: (req: Request, _res: Response, buf: Buffer) => {
    req.rawBody = buf;
  },
});

/**
 * Parses the ingest body against the per-route budget and normalises
 * body-parser failures into stable, payload-free error responses.
 */
export const createEventIngestBodyParser = (
  parser: RequestHandler = defaultJsonParser
): RequestHandler => {
  return (req: Request, res: Response, next: NextFunction): void => {
    parser(req, res, (err?: unknown) => {
      if (!err) {
        next();
        return;
      }

      const type = (err as { type?: string }).type;

      if (type === "entity.too.large") {
        reject(
          res,
          413,
          "BODY_LIMIT_EXCEEDED",
          `Request body exceeds the ${EVENT_INGEST_MAX_BODY_BYTES} byte limit for this endpoint.`
        );
        return;
      }

      if (type === "request.size.invalid") {
        reject(
          res,
          400,
          "CONTENT_LENGTH_MISMATCH",
          "Request body size did not match the declared Content-Length."
        );
        return;
      }

      if (type === "entity.parse.failed" || type === "encoding.unsupported") {
        // Deliberately drops the parser message, which quotes payload bytes.
        reject(res, 400, "INVALID_JSON_BODY", "Request body is not valid JSON.");
        return;
      }

      next(err);
    });
  };
};

export const eventIngestBodyParser = createEventIngestBodyParser();

/** Guard chain to mount on the ingest route, in order. */
export const eventIngestLimits: RequestHandler[] = [
  eventIngestLimitsMiddleware,
  eventIngestBodyParser,
];

/**
 * True when the request targets the ingest route, which owns its own parser and
 * must therefore be skipped by the application-wide JSON body parser.
 */
export const isEventIngestRequest = (req: Request): boolean => {
  if (req.method !== "POST") return false;
  const path = (req.path ?? "").replace(/\/+$/, "").toLowerCase();
  return path === EVENT_INGEST_PATH;
};
