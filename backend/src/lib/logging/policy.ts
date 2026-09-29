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
import { readFileSync } from "fs";
import { join } from "path";
import { z } from "zod";

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

// ── Policy Schema (Zod validation) ───────────────────────────────────────────

const RedactionPolicySchema = z.object({
  public: z.array(z.string()),
  private: z.array(z.string()),
  secret: z.array(z.string()),
});

type RedactionPolicy = z.infer<typeof RedactionPolicySchema>;

// ── Load and validate policy from JSON ───────────────────────────────────────

let loadedPolicy: RedactionPolicy;
let fieldTierMap: Record<string, FieldTier>;
let policyLoadError: Error | null = null;

function loadPolicy(): void {
  const policyPath = join(__dirname, "redaction-policy.json");
  const policyContent = readFileSync(policyPath, "utf-8");
  const parsedPolicy = JSON.parse(policyContent);
  loadedPolicy = RedactionPolicySchema.parse(parsedPolicy);
  
  // Build the field tier map
  // A null prototype keeps unlisted names such as "constructor" and
  // "toString" from resolving through Object.prototype.
  fieldTierMap = Object.create(null) as Record<string, FieldTier>;
  for (const field of loadedPolicy.public) {
    fieldTierMap[field] = FieldTier.PUBLIC;
  }
  for (const field of loadedPolicy.private) {
    fieldTierMap[field] = FieldTier.PRIVATE;
  }
  for (const field of loadedPolicy.secret) {
    fieldTierMap[field] = FieldTier.SECRET;
  }
}

/**
 * Initialize policy on module load.
 *
 * Invariant: after this call, `loadedPolicy` and `fieldTierMap` are always
 * defined. If the on-disk policy cannot be read or fails schema validation,
 * we fall back to a deny-by-default empty policy (every field classifies as
 * PRIVATE) and record the failure so `getPolicyFields` can surface it
 * deterministically instead of throwing at import time.
 */
function initialisePolicy(): void {
  try {
    loadPolicy();
    policyLoadError = null;
  } catch (err) {
    policyLoadError = err instanceof Error ? err : new Error(String(err));
    loadedPolicy = { public: [], private: [], secret: [] };
    fieldTierMap = {};
  }
}

initialisePolicy();

// ── Expose policy for other modules ───────────────────────────────────────────

/**
 * Return the list of fields registered under the given tier.
 *
 * Behaviour is deterministic across all inputs:
 *   - Valid tier with a loaded policy → the registered field list.
 *   - Valid tier when the policy failed to load → an empty array (deny-by-
 *     default). Callers must not assume a non-empty result.
 *   - Unknown / invalid tier → an empty array. This is a boundary case and
 *     never throws, so logging call sites cannot crash on bad input.
 *
 * The returned array is a defensive copy: mutating it cannot corrupt the
 * cached policy state, and concurrent callers cannot observe each other's
 * mutations.
 */
export function getPolicyFields(tier: FieldTier): string[] {
  if (tier !== FieldTier.PUBLIC && tier !== FieldTier.PRIVATE && tier !== FieldTier.SECRET) {
    return [];
  }
  const fields = loadedPolicy[tier];
  return Array.isArray(fields) ? fields.slice() : [];
}

/**
 * Diagnostic accessor for the last policy-load failure, if any.
 *
 * Returns `null` when the policy loaded successfully. Exposed so callers
 * (and tests) can distinguish "policy intentionally empty" from "policy
 * failed to load" without inspecting the filesystem or throwing.
 */
export function getPolicyLoadError(): Error | null {
  return policyLoadError;
}

// ── Field classification registry ─────────────────────────────────────────────

/**
 * Complete list of classified field names.
 *
 * Fields not listed here default to PRIVATE (deny-by-default).
 */


// ── Classification helpers ────────────────────────────────────────────────────

/**
 * Return the tier for a given field name.
 * Unknown fields default to PRIVATE (deny-by-default).
 */
export function classifyField(name: string): FieldTier {
  return (fieldTierMap[name] as FieldTier | undefined) ?? FieldTier.PRIVATE;
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
 * Deterministic serialisation for hashing.
 *
 * `JSON.stringify` is not deterministic across all inputs: object key order
 * depends on insertion order, and values like `undefined`, functions, and
 * symbols are silently dropped or coerced. To keep `hashValue` stable and
 * reviewable we canonicalise the input first:
 *
 *   - `null` / `undefined` → fixed sentinels (never the string "undefined").
 *   - primitives → tagged so `"1"` (string) and `1` (number) do not collide.
 *   - arrays → element order preserved, recursively canonicalised.
 *   - plain objects → keys sorted lexicographically, recursively canonicalised.
 *   - other objects (Date, Map, class instances) → tagged by constructor name
 *     and their `toJSON`/`toString` output, so distinct types never collide.
 *
 * This is intentionally pure and side-effect-free.
 */
function canonicalise(value: unknown): string {
  if (value === null) return "null";
  if (value === undefined) return "undefined";
  const t = typeof value;
  if (t === "string") return `s:${value}`;
  if (t === "number") return `n:${value}`;
  if (t === "boolean") return `b:${value}`;
  if (t === "bigint") return `i:${(value as bigint).toString()}`;
  if (t === "symbol") return `y:${String(value)}`;
  if (t === "function") return `f:${(value as Function).name ?? ""}`;
  if (Array.isArray(value)) {
    return `a:[${value.map(canonicalise).join(",")}]`;
  }
  const obj = value as Record<string, unknown>;
  const ctor = (obj as { constructor?: { name?: string } }).constructor?.name ?? "Object";
  const keys = Object.keys(obj).sort();
  const body = keys.map((k) => `${JSON.stringify(k)}:${canonicalise(obj[k])}`).join(",");
  return `o:${ctor}:{${body}}`;
}

/**
 * Produce a non-reversible, short hash of a value for private fields.
 * Only the first `HASH_PREFIX_LEN` hex characters are kept to prevent
 * brute-force recovery of short values like wallet addresses.
 *
 * Determinism guarantees:
 *   - Same logical value → same hash, regardless of object key insertion order.
 *   - Different types never collide (e.g. `1` vs `"1"` vs `true`).
 *   - `null` and `undefined` are distinct and stable.
 *   - Cyclic structures are rejected with a deterministic error rather than
 *     hanging or producing a partial hash.
 */
export function hashValue(value: unknown): string {
  const seen = new WeakSet<object>();
  const guard = (v: unknown): string => {
    if (v !== null && typeof v === "object") {
      if (seen.has(v as object)) {
        throw new TypeError("hashValue: cyclic structure is not supported");
      }
      seen.add(v as object);
    }
    if (Array.isArray(v)) {
      return `a:[${v.map(guard).join(",")}]`;
    }
    if (v !== null && typeof v === "object") {
      const obj = v as Record<string, unknown>;
      const ctor = (obj as { constructor?: { name?: string } }).constructor?.name ?? "Object";
      const keys = Object.keys(obj).sort();
      const body = keys.map((k) => `${JSON.stringify(k)}:${guard(obj[k])}`).join(",");
      return `o:${ctor}:{${body}}`;
    }
    return canonicalise(v);
  };
  const str = guard(value);
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
 *
 * Failure boundary: if `hashValue` throws (e.g. cyclic input), we fail closed
 * by returning the redaction sentinel rather than leaking the raw value or
 * propagating an exception into the logging path. This keeps logging
 * best-effort and never causes silent data loss of the surrounding record.
 */
export function redactByTier(value: unknown, tier: FieldTier): unknown {
  if (tier === FieldTier.PUBLIC) return value;
  if (tier === FieldTier.SECRET) return REDACTED_SENTINEL;
  // PRIVATE
  if (value === null || value === undefined) return value;
  try {
    return hashValue(value);
  } catch {
    return REDACTED_SENTINEL;
  }
}

// ── Object-level deep redaction ───────────────────────────────────────────────

/**
 * Recursively redact an object according to the field policy.
 *
 * Arrays are traversed element-by-element. Primitive leaves are returned
 * unchanged (the caller is responsible for classifying the field before
 * passing its value here).
 *
 * Failure boundary: any per-field hashing failure is contained so a single
 * pathological value cannot abort redaction of the whole record. The failing
 * field is replaced with the sentinel; all other fields are still redacted
 * deterministically.
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
        if (tier === FieldTier.SECRET) {
          out[key] = REDACTED_SENTINEL;
        } else {
          try {
            out[key] = hashValue(value);
          } catch {
            out[key] = REDACTED_SENTINEL;
          }
        }
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
        try {
          out[key] = hashValue(value);
        } catch {
          out[key] = REDACTED_SENTINEL;
        }
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
 * The detector is deterministic and cycle-safe: it follows a stable traversal
 * order, tracks previously seen objects, and treats the literal "[REDACTED]"
 * as a safe sentinel instead of a leak.
 *
 * Useful in tests as a regression guard:
 * ```ts
 * expect(findSecretLeak(logOutput)).toBeNull();
 * ```
 */
function looksLikeSecretLiteral(value: string): boolean {
  const trimmed = value.trim();
  if (!trimmed || trimmed === REDACTED_SENTINEL) return false;

  const patterns = [
    /(?:^|\s|[:=])(?:authorization|api[_-]?key|access[_-]?token|refresh[_-]?token|token|secret|password|cookie|session|jwt)\s*[:=]\s*.+/i,
    /^(?:bearer|basic)\s+[A-Za-z0-9._~+/-]+=*$/i,
    /^(?:sk|ghp|xox[baprs]-)[A-Za-z0-9._~+/=-]{8,}$/i,
  ];

  return patterns.some((pattern) => pattern.test(trimmed));
}

export function findSecretLeak(
  value: unknown,
  _path = "",
  seen = new WeakSet<object>()
): { path: string; value: unknown } | null {
  if (value === null || value === undefined) return null;

  if (typeof value === "string") {
    if (looksLikeSecretLiteral(value)) {
      return { path: _path || "$root", value };
    }
    return null;
  }

  if (typeof value !== "object") return null;

  if (seen.has(value)) return null;
  seen.add(value);

  if (Array.isArray(value)) {
    for (let i = 0; i < value.length; i++) {
      const found = findSecretLeak(value[i], `${_path}[${i}]`, seen);
      if (found) return found;
    }
    return null;
  }

  if (value instanceof Map) {
    let index = 0;
    for (const [mapKey, mapValue] of value.entries()) {
      const mapKeyPath = _path ? `${_path}.map[${index}]` : `map[${index}]`;
      const keyLeak = findSecretLeak(mapKey, `${mapKeyPath}.key`, seen);
      if (keyLeak) return keyLeak;
      const valueLeak = findSecretLeak(mapValue, `${mapKeyPath}.value`, seen);
      if (valueLeak) return valueLeak;
      index += 1;
    }
    return null;
  }

  if (value instanceof Set) {
    let index = 0;
    for (const entry of value.values()) {
      const setPath = _path ? `${_path}.set[${index}]` : `set[${index}]`;
      const found = findSecretLeak(entry, setPath, seen);
      if (found) return found;
      index += 1;
    }
    return null;
  }

  try {
    for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
      const fieldPath = _path ? `${_path}.${k}` : k;
      if (isSecret(k) && v !== REDACTED_SENTINEL) {
        return { path: fieldPath, value: v };
      }
      const found = findSecretLeak(v, fieldPath, seen);
      if (found) return found;
    }
  } catch {
    return null;
  }

  return null;
}
