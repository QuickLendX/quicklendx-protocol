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
 *
 * The fallback must reproduce the *deny-by-default* half of the policy, and it
 * may only do that if the replacement map has a null prototype for exactly the
 * reason `loadPolicy` does: a name that collides with an `Object.prototype`
 * member would otherwise resolve through the prototype chain, and
 * `isPrivate("constructor")` would answer false for a field that has to be
 * masked. The failure path is the one place where that was previously wrong,
 * which made the most sensitive boundary the least protected.
 */
function initialisePolicy(): void {
  try {
    loadPolicy();
    policyLoadError = null;
  } catch (err) {
    policyLoadError = err instanceof Error ? err : new Error(String(err));
    loadedPolicy = { public: [], private: [], secret: [] };
    fieldTierMap = Object.create(null) as Record<string, FieldTier>;
  }
}

initialisePolicy();

// ── Expose policy for other modules ───────────────────────────────────────────

export interface PolicyFieldEntry {
  field: string;
  tier: FieldTier;
}

/**
 * Return policy fields.
 *
 * Overloads:
 * 1. `getPolicyFields(tier: FieldTier): string[]`
 *    Returns the list of field names configured under the given tier.
 * 2. `getPolicyFields(fields: string[]): PolicyFieldEntry[]`
 *    Classifies each field in `fields` returning `{ field, tier }` objects.
 */
export function getPolicyFields(tier: FieldTier): string[];
export function getPolicyFields(fields: string[]): PolicyFieldEntry[];
export function getPolicyFields(
  arg: FieldTier | string[]
): string[] | PolicyFieldEntry[] {
  if (arg === null || arg === undefined) {
    throw new TypeError("getPolicyFields: argument cannot be null or undefined");
  }

  if (Array.isArray(arg)) {
    for (const elem of arg) {
      if (typeof elem !== "string") {
        throw new TypeError("getPolicyFields: all array elements must be strings");
      }
    }
    return arg.map((field) => ({
      field,
      tier: classifyField(field),
    }));
  }

  if (typeof arg !== "string") {
    throw new TypeError("getPolicyFields: expected a string[] or FieldTier");
  }

  if (
    arg !== FieldTier.PUBLIC &&
    arg !== FieldTier.PRIVATE &&
    arg !== FieldTier.SECRET
  ) {
    throw new TypeError("getPolicyFields: expected an array of field names");
  }

  const fields = loadedPolicy[arg];
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
 *
 * Invariant: for every input exactly one of `isPublic` / `isPrivate` /
 * `isSecret` is true, and a name that is not an own key of `fieldTierMap` is
 * always PRIVATE. Callers use those three predicates to decide whether a value
 * may be logged verbatim, so an unclassified name that answers "none of the
 * three" is a leak.
 *
 * The own-property guard is what makes that hold independently of how
 * `fieldTierMap` was built. Relying on a null prototype alone left the invariant
 * one `fieldTierMap = {}` away from being false again — and that is exactly what
 * the policy-load failure path used to do, so `isPrivate("constructor")` and
 * `isPrivate("toString")` answered false whenever the policy file was
 * unreadable. A field explicitly registered under such a name is still honoured;
 * only prototype lookups are rejected.
 *
 * A non-string input is rejected before the lookup. Property access would coerce
 * it to a key, so `classifyField(undefined)` would resolve to the tier of a field
 * literally named `"undefined"` — letting a caller that passes the wrong type
 * classify a value as PUBLIC, and log it verbatim. The `string` type is only a
 * compile-time promise; this makes it hold at runtime too.
 */
export function classifyField(name: string): FieldTier {
  if (typeof name !== "string") return FieldTier.PRIVATE;
  const tier = Object.prototype.hasOwnProperty.call(fieldTierMap, name)
    ? (fieldTierMap[name] as FieldTier | undefined)
    : undefined;
  return tier ?? FieldTier.PRIVATE;
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
  if (t === "string") return value as string;
  if (t === "number") return String(value);
  if (t === "boolean") return String(value);
  if (t === "bigint") return (value as bigint).toString();
  if (t === "symbol") return String(value);
  if (t === "function") return (value as Function).name ?? "";
  if (Array.isArray(value)) {
    return `[${value.map(canonicalise).join(",")}]`;
  }
  const obj = value as Record<string, unknown>;
  const keys = Object.keys(obj).sort();
  const body = keys.map((k) => `${JSON.stringify(k)}:${canonicalise(obj[k])}`).join(",");
  return `{${body}}`;
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
  obj: Record<string, unknown>,
  seen: WeakSet<object> = new WeakSet()
): Record<string, unknown> {
  if (obj === null || typeof obj !== "object") {
    return {};
  }
  if (seen.has(obj)) {
    throw new Error("redactObject: cyclic structure is not supported");
  }
  seen.add(obj);

  const out: Record<string, unknown> = {};

  // We use Object.keys instead of Object.entries to safely catch throwing getters.
  for (const key of Object.keys(obj)) {
    let value: unknown;
    try {
      value = obj[key];
    } catch {
      out[key] = REDACTED_SENTINEL;
      continue;
    }

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
        out[key] = value.map((item) => {
          if (item !== null && typeof item === "object") {
            try {
              return redactObject(item as Record<string, unknown>, seen);
            } catch {
              return REDACTED_SENTINEL;
            }
          }
          return item;
        });
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
        try {
          out[key] = redactObject(value as Record<string, unknown>, seen);
        } catch {
          out[key] = REDACTED_SENTINEL;
        }
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
 * Coerce an arbitrary value into a plain record for redaction.
 *
 * Returns an empty record for `null`, `undefined`, primitives, and arrays so
 * that a malformed request container degrades to "nothing to redact" instead
 * of throwing. Without this guard, `redactObject` calls `Object.entries` on the
 * value and a `null` query or header bag raises a `TypeError` inside the
 * logging path, which would abort the surrounding record.
 */
function asRedactableRecord(value: unknown): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }
  return value as Record<string, unknown>;
}

/**
 * Coerce an arbitrary value into a string for the snapshot's routing fields.
 * `null` / `undefined` become the empty string so a partial request object can
 * never put an `undefined` hole into the serialised log line.
 */
function asSnapshotString(value: unknown): string {
  return typeof value === "string" ? value : value === undefined || value === null ? "" : String(value);
}

/**
 * Canonical string for a header value, used only to order colliding values.
 * Uses the same tagging scheme as `canonicalise` so ordering never depends on
 * whether a value arrived as `"1"` or `1`.
 */
function headerValueSortKey(value: unknown): string {
  try {
    return canonicalise(value);
  } catch {
    // A cyclic header value cannot be canonicalised; fall back to a constant
    // key so the comparator stays total and the sort cannot throw.
    return " ";
  }
}

/**
 * Lower-case header names and merge values that collide under that
 * normalisation.
 *
 * Why merge instead of first-wins / last-wins: a plain `Object.fromEntries`
 * over `[k.toLowerCase(), v]` keeps only the *last* spelling, so
 * `{"X-Trace": "a", "x-trace": "b"}` and `{"x-trace": "b", "X-Trace": "a"}`
 * would produce two different snapshots for the same logical request. That is
 * an ordering-dependent log record — precisely the non-determinism a redaction
 * boundary must not have, since it makes two identical requests look like
 * different traffic.
 *
 * Colliding values are kept as a sorted array so both orderings converge, and
 * the result stays JSON-serialisable. A single-value header (the overwhelming
 * majority) keeps its original scalar shape, so existing snapshots and
 * consumers are unchanged.
 */
function normaliseHeaderBag(
  headers: Record<string, unknown>
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [name, value] of Object.entries(headers)) {
    const key = name.toLowerCase();
    const existing = Object.prototype.hasOwnProperty.call(out, key)
      ? out[key]
      : undefined;
    if (!Object.prototype.hasOwnProperty.call(out, key)) {
      out[key] = value;
      continue;
    }
    const merged = (Array.isArray(existing) ? existing : [existing]).concat([
      value,
    ]);
    merged.sort((a, b) => {
      const ka = headerValueSortKey(a);
      const kb = headerValueSortKey(b);
      return ka < kb ? -1 : ka > kb ? 1 : 0;
    });
    out[key] = merged;
  }
  return out;
}

/**
 * Produce a log-safe snapshot of an incoming HTTP request.
 * All query params, headers, and body fields are classified and redacted.
 *
 * Invariants — this function is total (it never throws) and pure:
 *   1. **Total.** A `null` request, a missing query/header bag, or a malformed
 *      container yields an empty record rather than an exception. Logging is a
 *      best-effort side channel: it must never be the reason a request fails,
 *      so a defect here degrades the record instead of dropping it.
 *   2. **No mutation.** The input request is only read; a fresh object is
 *      returned for every section, so a caller reusing the same Express
 *      `req` across retries cannot observe partially-redacted state.
 *   3. **Deterministic.** Identical input yields byte-identical output: header
 *      names are lower-cased before classification, and `hashValue` is
 *      key-order independent. Two concurrent calls share no state, so a retry
 *      after a failure produces exactly the snapshot the first call would have.
 *   4. **Fails closed.** Anything that cannot be classified becomes PRIVATE
 *      (hashed) or `[REDACTED]`; no raw value ever reaches the returned object,
 *      so `findSecretLeak(snapshot)` is always `null`.
 */
export function sanitiseRequest(req: {
  method: string;
  path: string;
  query: Record<string, unknown>;
  headers: Record<string, unknown>;
  body?: unknown;
}): SafeRequestSnapshot {
  // Guard the whole body: a getter that throws on `req.headers` or a proxy
  // that rejects `ownKeys` must not escape into the request pipeline.
  let source: Record<string, unknown> = {};
  try {
    source =
      req !== null && typeof req === "object"
        ? (req as unknown as Record<string, unknown>)
        : {};
  } catch {
    source = {};
  }

  return {
    method: asSnapshotString(source.method),
    path: asSnapshotString(source.path),
    query: redactObject(asRedactableRecord(source.query)),
    headers: redactObject(
      // Normalise header names to lower case before classification so
      // `Content-Type` and `content-type` cannot resolve to different tiers.
      // RFC 9110 §5.2 makes header names case-insensitive, so two spellings of
      // the same name are one field, not two. Colliding values are sorted by
      // their canonical form rather than taken in arrival order, so the
      // snapshot does not depend on the order the transport happened to
      // deliver them in — the same headers always hash to the same value.
      // See `normaliseHeaderBag`.
      normaliseHeaderBag(asRedactableRecord(source.headers))
    ),
    body:
      source.body && typeof source.body === "object"
        ? redactObject(source.body as Record<string, unknown>)
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
