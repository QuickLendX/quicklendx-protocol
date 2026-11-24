/**
 * place-bid-errors.ts: turn whatever `placeBid` threw into something the form can
 * show and act on.
 *
 * `ApiClient` already maps HTTP statuses onto typed errors (401
 * `AuthenticationError`, 403 `AuthorizationError`, 400/422 `ValidationError`,
 * 429 `NetworkError` carrying the server's code, 5xx `SystemError`). This module
 * goes one level further and answers the questions a form actually has:
 *
 *  - What do I tell the investor, in words that say what to do next?
 *  - Which field, if any, is the problem?
 *  - Did the server possibly process the request anyway? (`ambiguous`)
 *  - Is there a limit worth remembering, or a wait before trying again?
 *
 * `ambiguous` matters because a bid is a write that is sent exactly once. After a
 * timeout, a dropped connection or a 5xx the server may or may not have recorded
 * it, so the form must not offer a blind resend: it refreshes the bid list so the
 * investor can see for themselves.
 */

import { formatAtomicAmount } from "@/app/business/invoices/_lib/math";
import {
  AppError,
  AuthenticationError,
  AuthorizationError,
  NetworkError,
  ValidationError,
} from "@/app/lib/errors";
import { parseLimitFromDetails, type InvestorLimit } from "./investor-limit";
import type { FieldErrors } from "./place-bid";

export type PlaceBidFailureKind =
  | "auth"
  | "forbidden"
  | "limit"
  | "rate-limit"
  | "validation"
  | "network"
  | "server";

export interface PlaceBidFailure {
  kind: PlaceBidFailureKind;
  message: string;
  code?: string;
  /** Problems tied to a specific field. */
  fieldErrors?: FieldErrors;
  /** The server may have processed the request: refresh before anything else. */
  ambiguous: boolean;
  /** Set for `EXPOSURE_CAP_EXCEEDED`, so the next attempt can be refused locally. */
  learnedLimit?: InvestorLimit;
  /** Seconds the server asked us to wait (rate limiting). */
  retryAfterSeconds?: number;
}

/** Signature problems the API reports with 401. */
const SIGNATURE_CODES = new Set([
  "MISSING_SIGNATURE",
  "INVALID_SIGNATURE",
  "EXPIRED_SIGNATURE",
  "REPLAY_DETECTED",
  "INVALID_TIMESTAMP",
]);

/** Backend field name -> form field. */
const FIELD_MAP: Record<string, keyof FieldErrors> = {
  bid_amount: "amount",
  expected_return: "expectedReturn",
  expiration_timestamp: "expiresAt",
};

/** micro-USD as a readable USD amount, e.g. `1500000000000` -> `1,500,000`. */
export function formatUsdMicro(value: string): string {
  const formatted = formatAtomicAmount(value, 6);
  const [whole, frac = ""] = formatted.split(".");
  let wholeOut = whole;
  try {
    wholeOut = BigInt(whole).toLocaleString("en-US");
  } catch {
    // keep the raw digits
  }
  const trimmed = frac.replace(/0+$/, "");
  return trimmed ? `${wholeOut}.${trimmed}` : wholeOut;
}

function additionalData(error: AppError): Record<string, unknown> {
  return (error.context.additionalData ?? {}) as Record<string, unknown>;
}

/** `Retry-After` is either a number of seconds or an HTTP date. */
function parseRetryAfter(raw: unknown): number | undefined {
  if (typeof raw === "number" && Number.isFinite(raw) && raw >= 0) {
    return Math.ceil(raw);
  }
  if (typeof raw !== "string" || !raw.trim()) return undefined;
  const trimmed = raw.trim();
  // A number is seconds or nothing. It must not fall through to date parsing:
  // `Date.parse("-5")` is a valid (long past) date and would read as "wait 0".
  if (/^-?\d+(\.\d+)?$/.test(trimmed)) {
    const seconds = Number(trimmed);
    return seconds >= 0 ? Math.ceil(seconds) : undefined;
  }
  const date = Date.parse(trimmed);
  if (Number.isFinite(date)) {
    return Math.max(0, Math.ceil((date - Date.now()) / 1000));
  }
  return undefined;
}

function fieldErrorsFromDetails(details: unknown): FieldErrors | undefined {
  if (!Array.isArray(details)) return undefined;
  const out: FieldErrors = {};
  for (const entry of details) {
    if (!entry || typeof entry !== "object") continue;
    const { field, message } = entry as { field?: unknown; message?: unknown };
    if (typeof field !== "string" || typeof message !== "string") continue;
    const mapped = FIELD_MAP[field];
    if (mapped && out[mapped] === undefined) out[mapped] = message;
  }
  return Object.keys(out).length > 0 ? out : undefined;
}

/** The server's business-rule refusals arrive as 400 INVALID_BID with prose. */
function describeInvalidBid(message: string): {
  message: string;
  fieldErrors?: FieldErrors;
} {
  const lower = message.toLowerCase();
  if (lower.includes("already has an active bid")) {
    return { message: "You already have an active bid on this invoice." };
  }
  if (lower.includes("only verified invoices")) {
    return { message: "This invoice is not open for bids." };
  }
  if (lower.includes("invoice not found")) {
    return { message: "This invoice could not be found." };
  }
  if (lower.includes("expected return")) {
    return { message, fieldErrors: { expectedReturn: message } };
  }
  if (lower.includes("bid amount")) {
    return { message, fieldErrors: { amount: message } };
  }
  return { message };
}

export function describePlaceBidError(error: unknown): PlaceBidFailure {
  if (error instanceof AuthenticationError) {
    if (error.code && SIGNATURE_CODES.has(error.code)) {
      return {
        kind: "auth",
        code: error.code,
        ambiguous: false,
        message:
          "The server rejected the signature on this request. Bids are signed with the API key's secret, and this browser session could not provide a valid one.",
      };
    }
    return {
      kind: "auth",
      code: error.code,
      ambiguous: false,
      message: "You are not signed in for bidding. Sign in again, then retry.",
    };
  }

  if (error instanceof AuthorizationError) {
    if (error.code === "SIGNATURE_REQUIRED") {
      return {
        kind: "forbidden",
        code: error.code,
        ambiguous: false,
        message:
          "This API key cannot sign requests yet. Rotate the key to generate a signing secret, then retry.",
      };
    }
    return {
      kind: "forbidden",
      code: error.code,
      ambiguous: false,
      message: "Your account is not permitted to place bids on this invoice.",
    };
  }

  // 429 is a NetworkError carrying the server's code and `retryAfter`; a timeout
  // is also a NetworkError, so tell them apart by the field only 429 sets.
  if (error instanceof NetworkError && "retryAfter" in additionalData(error)) {
    const data = additionalData(error);

    if (error.code === "EXPOSURE_CAP_EXCEEDED") {
      const limit = parseLimitFromDetails(data.details);
      let message = "This bid would take you over your exposure limit.";
      if (limit) {
        const remaining =
          BigInt(limit.capUsd) > BigInt(limit.currentExposureUsd)
            ? (
                BigInt(limit.capUsd) - BigInt(limit.currentExposureUsd)
              ).toString()
            : "0";
        message =
          `This bid would take you over your exposure limit. ` +
          `Your limit is ${formatUsdMicro(limit.capUsd)} USD, you have ` +
          `${formatUsdMicro(limit.currentExposureUsd)} USD in use, and ` +
          `${formatUsdMicro(remaining)} USD remains. Lower the amount or wait ` +
          `for a position to settle.`;
      }
      return {
        kind: "limit",
        code: error.code,
        ambiguous: false,
        message,
        learnedLimit: limit ?? undefined,
      };
    }

    const wait = parseRetryAfter(data.retryAfter);
    return {
      kind: "rate-limit",
      code: error.code,
      ambiguous: false,
      retryAfterSeconds: wait,
      message:
        wait !== undefined && wait > 0
          ? `Too many requests. Try again in ${wait} second${wait === 1 ? "" : "s"}.`
          : "Too many requests. Wait a moment, then try again.",
    };
  }

  if (error instanceof ValidationError) {
    const data = additionalData(error);
    const fromDetails = fieldErrorsFromDetails(data.details);
    if (fromDetails) {
      return {
        kind: "validation",
        code: error.code,
        ambiguous: false,
        fieldErrors: fromDetails,
        message: "Check the highlighted fields and try again.",
      };
    }
    const invalid = describeInvalidBid(error.message);
    return {
      kind: "validation",
      code: error.code,
      ambiguous: false,
      fieldErrors: invalid.fieldErrors,
      message: invalid.message,
    };
  }

  if (error instanceof NetworkError) {
    return {
      kind: "network",
      code: error.code,
      ambiguous: true,
      message:
        "Could not confirm whether the bid went through. Check the ranked bids below before trying again.",
    };
  }

  // 5xx, an unreadable success reply, or something we did not anticipate.
  const code = error instanceof AppError ? error.code : undefined;
  return {
    kind: "server",
    code,
    ambiguous: true,
    message:
      code === "BID_RESPONSE_INVALID" && error instanceof Error
        ? error.message
        : "Something went wrong on the server. Check the ranked bids below before trying again.",
  };
}
