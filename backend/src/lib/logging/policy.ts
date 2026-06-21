/**
 * Logging Policy — Field-Level Redaction and PII Classification
 *
 * Defines three sensitivity tiers:
 *   PUBLIC  — safe to appear verbatim in any log sink.
 *   PRIVATE — business-sensitive; must be masked before logging.
 *   SECRET  — must NEVER appear in logs; always replaced with [REDACTED].
 *
 * Design principles
 * ─────────────────
 * 1. Deny-by-default: unknown fields are treated as PRIVATE and masked.
 * 2. The redaction functions are pure and side-effect-free — they always
 *    return a new object, never mutating the input.
 * 3. Masking is deterministic per field tier so snapshots are stable in tests.
 * 4. No crypto operations are performed on SECRET fields; they are simply
 *    replaced with the literal "[REDACTED]" so no information leaks via
 *    timing, encoding, or key material.
 */

import { createHash } from "crypto";

// ── Tier definitions ──────────────────────────────────────────────────────────

export const FieldTier = {
  /** Appears verbatim in logs. */
  PUBLIC: "public",
  /** Business-sensitive; logged as a SHA-256 prefix hash. */
  PRIVATE: "private",
  /** Never logged; replaced by the literal string [REDACTED]. */
  SECRET: "secret",
} as const;

export type FieldTier = (typeof FieldTier)[keyof typeof FieldTier];

// ── Field classification registry ─────────────────────────────────────────────

/**
 * Complete list of classified field names.
 *
 * Fields not listed here default to PRIVATE (deny-by-default).
 */
const FIELD_POLICY: Record<string, FieldTier> = {
  // ── Public fields (safe to log verbatim) ────────────────────────────────
  id:                      FieldTier.PUBLIC,
  invoice_id:              FieldTier.PUBLIC,
  bid_id:                  FieldTier.PUBLIC,
  settlement_id:           FieldTier.PUBLIC,
  dispute_id:              FieldTier.PUBLIC,
  status:                  FieldTier.PUBLIC,
  timestamp:               FieldTier.PUBLIC,
  created_at:              FieldTier.PUBLIC,
  updated_at:              FieldTier.PUBLIC,
  method:                  FieldTier.PUBLIC,
  path:                    FieldTier.PUBLIC,
  url:                     FieldTier.PUBLIC,
  statusCode:              FieldTier.PUBLIC,
  duration:                FieldTier.PUBLIC,
  requestId:               FieldTier.PUBLIC,
  version:                 FieldTier.PUBLIC,
  category:                FieldTier.PUBLIC,
  currency:                FieldTier.PUBLIC,
  due_date:                FieldTier.PUBLIC,

  // ── Private fields (hashed before logging) ──────────────────────────────
  business:                FieldTier.PRIVATE,
  investor:                FieldTier.PRIVATE,
  payer:                   FieldTier.PRIVATE,
  recipient:               FieldTier.PRIVATE,
  actor:                   FieldTier.PRIVATE,
  user_id:                 FieldTier.PRIVATE,
  userId:                  FieldTier.PRIVATE,
  initiator:               FieldTier.PRIVATE,
  amount:                  FieldTier.PRIVATE,
  bid_amount:              FieldTier.PRIVATE,
  expected_return:         FieldTier.PRIVATE,
  ipAddress:               FieldTier.PRIVATE,
  ip:                      FieldTier.PRIVATE,
  userAgent:               FieldTier.PRIVATE,
  user_agent:              FieldTier.PRIVATE,
  description:             FieldTier.PRIVATE,
  reason:                  FieldTier.PRIVATE,
  tags:                    FieldTier.PRIVATE,
  notes:                   FieldTier.PRIVATE,

  // ── Secret fields (must NEVER appear in logs) ───────────────────────────
  // Wallet / authentication
  signature:               FieldTier.SECRET,
  wallet_signature:        FieldTier.SECRET,
  private_key:             FieldTier.SECRET,
  secret:                  FieldTier.SECRET,
  token:                   FieldTier.SECRET,
  access_token:            FieldTier.SECRET,
  refresh_token:           FieldTier.SECRET,
  api_key:                 FieldTier.SECRET,
  authorization:           FieldTier.SECRET,
  password:                FieldTier.SECRET,
  // KYC / PII
  tax_id:                  FieldTier.SECRET,
  ssn:                     FieldTier.SECRET,
  national_id:             FieldTier.SECRET,
  passport_number:         FieldTier.SECRET,
  date_of_birth:           FieldTier.SECRET,
  bank_account:            FieldTier.SECRET,
  kyc_document:            FieldTier.SECRET,
  kyc_data:                FieldTier.SECRET,
  customer_name:           FieldTier.SECRET,
  customer_address:        FieldTier.SECRET,
  phone_number:            FieldTier.SECRET,
  email:                   FieldTier.SECRET,
  // Crypto secrets
  mnemonic:                FieldTier.SECRET,
  seed_phrase:             FieldTier.SECRET,
  // Webhook
  webhook_secret:          FieldTier.SECRET,
  signing_secret:          FieldTier.SECRET,
} as const;

// ── Classification helpers ────────────────────────────────────────────────────

/**
 * Return the tier for a given field name.
 * Unknown fields default to PRIVATE (deny-by-default).
 */
export function classifyField(name: string): FieldTier {
  return (FIELD_POLICY[name] as FieldTier | undefined) ?? FieldTier.PRIVATE;
}

/** True when a field must never appear in any log output. */
export function isSecret(name: string): boolean {
  return classifyField(name) === FieldTier.SECRET;
}

/** True when a field is safe to log verbatim. */
export function isPublic(name: string): boolean {
  return classifyField(name) === FieldTier.PUBLIC;
}

/** True when a field should be hashed before logging. */
export function isPrivate(name: string): boolean {
  return classifyField(name) === FieldTier.PRIVATE;
}

// ── Value-level redaction ─────────────────────────────────────────────────────

const REDACTED_SENTINEL = "[REDACTED]";
const HASH_PREFIX_LEN = 8; // characters of SHA-256 hex to keep

/**
 * Produce a non-reversible, short hash of a value for private fields.
 * Only the first `HASH_PREFIX_LEN` hex characters are kept to prevent
 * brute-force recovery of short values like wallet addresses.
 */
export function hashValue(value: unknown): string {
  const str = typeof value === "string" ? value : JSON.stringify(value);
  return (
    "sha256:" +
    createHash("sha256").update(str).digest("hex").slice(0, HASH_PREFIX_LEN)
  );
}

/**
 * Redact a single leaf value according to the given tier.
 *
 * - PUBLIC  → value unchanged
 * - PRIVATE → `hashValue(value)`
 * - SECRET  → `"[REDACTED]"`
 */
export function redactByTier(value: unknown, tier: FieldTier): unknown {
  if (tier === FieldTier.PUBLIC) return value;
  if (tier === FieldTier.SECRET) return REDACTED_SENTINEL;
  // PRIVATE
  if (value === null || value === undefined) return value;
  return hashValue(value);
}

// ── Object-level deep redaction ───────────────────────────────────────────────

/**
 * Recursively redact an object according to the field policy.
 *
 * Arrays are traversed element-by-element. Primitive leaves are returned
 * unchanged (the caller is responsible for classifying the field before
 * passing its value here).
 */
export function redactObject(
  obj: Record<string, unknown>
): Record<string, unknown> {
  const out: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(obj)) {
    const tier = classifyField(key);
    if (Array.isArray(value)) {
      // Redact each element if they are objects, otherwise apply tier to array
      if (tier !== FieldTier.PUBLIC) {
        out[key] = tier === FieldTier.SECRET ? REDACTED_SENTINEL : hashValue(value);
      } else {
        out[key] = value.map((item) =>
          item !== null && typeof item === "object"
            ? redactObject(item as Record<string, unknown>)
            : item
        );
      }
    } else if (value !== null && typeof value === "object") {
      if (tier === FieldTier.SECRET) {
        out[key] = REDACTED_SENTINEL;
      } else if (tier === FieldTier.PRIVATE) {
        out[key] = hashValue(value);
      } else {
        // PUBLIC: recurse into nested objects
        out[key] = redactObject(value as Record<string, unknown>);
      }
    } else {
      out[key] = redactByTier(value, tier);
    }
  }

  return out;
}

// ── Request / Response safe serialisers ──────────────────────────────────────

export interface SafeRequestSnapshot {
  method: string;
  path: string;
  query: Record<string, unknown>;
  headers: Record<string, unknown>;
  body: Record<string, unknown> | null;
}

/**
 * Produce a log-safe snapshot of an incoming HTTP request.
 * All query params, headers, and body fields are classified and redacted.
 */
export function sanitiseRequest(req: {
  method: string;
  path: string;
  query: Record<string, unknown>;
  headers: Record<string, unknown>;
  body?: unknown;
}): SafeRequestSnapshot {
  return {
    method: req.method,
    path: req.path,
    query: redactObject(req.query as Record<string, unknown>),
    headers: redactObject(
      // Drop raw Authorization / Cookie values before object redaction
      Object.fromEntries(
        Object.entries(req.headers).map(([k, v]) => [k.toLowerCase(), v])
      )
    ),
    body:
      req.body && typeof req.body === "object"
        ? redactObject(req.body as Record<string, unknown>)
        : null,
  };
}

export interface SafeResponseSnapshot {
  statusCode: number;
  body: Record<string, unknown> | null;
}

/**
 * Produce a log-safe snapshot of an outgoing HTTP response body.
 */
export function sanitiseResponse(
  statusCode: number,
  body: unknown
): SafeResponseSnapshot {
  return {
    statusCode,
    body:
      body && typeof body === "object"
        ? redactObject(body as Record<string, unknown>)
        : null,
  };
}

// ── "No secrets in output" assertion helper ───────────────────────────────────

/**
 * Walk any serialisable value and return the first SECRET value found,
 * or `null` if the object is clean.
 *
 * Useful in tests as a regression guard:
 * ```ts
 * expect(findSecretLeak(logOutput)).toBeNull();
 * ```
 */
export function findSecretLeak(
  value: unknown,
  _path = ""
): { path: string; value: unknown } | null {
  if (value === null || value === undefined) return null;

  if (typeof value === "string") {
    // Treat the literal "[REDACTED]" as clean; anything else is suspicious
    // only if it matches a known secret pattern — let callers do that check.
    return null;
  }

  if (Array.isArray(value)) {
    for (let i = 0; i < value.length; i++) {
      const found = findSecretLeak(value[i], `${_path}[${i}]`);
      if (found) return found;
    }
    return null;
  }

  if (typeof value === "object") {
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      const fieldPath = _path ? `${_path}.${k}` : k;
      if (isSecret(k) && v !== REDACTED_SENTINEL) {
        return { path: fieldPath, value: v };
      }
      const found = findSecretLeak(v, fieldPath);
      if (found) return found;
    }
  }

  return null;
}
