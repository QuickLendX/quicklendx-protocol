/**
 * Scope Registry - Defines all valid API key scopes
 *
 * Invariants relied upon by `isValidScope` / `validateScopes`:
 * - `scope` values are unique across the registry (an entry is valid iff it
 *   appears here; duplicates would silently broaden matching).
 * - Matching is an exact, case-sensitive literal comparison. Scopes are
 *   never normalized: whitespace, case, and unicode variants are invalid.
 * - The registry is a static compile-time constant; runtime code must not
 *   add or remove entries (tests pin its contents to catch drift).
 */

export interface ScopeDefinition {
  scope: string;
  description: string;
  category: 'read' | 'write' | 'admin' | 'service';
}

export const SCOPE_REGISTRY: ScopeDefinition[] = [
  // Read scopes
  {
    scope: 'read:*',
    description: 'Read access to all resources',
    category: 'read',
  },
  {
    scope: 'read:users',
    description: 'Read user information',
    category: 'read',
  },
  {
    scope: 'read:jobs',
    description: 'Read job data',
    category: 'read',
  },
  {
    scope: 'read:invoices',
    description: 'Read invoice data',
    category: 'read',
  },
  {
    scope: 'read:bids',
    description: 'Read bid information',
    category: 'read',
  },
  {
    scope: 'read:settlements',
    description: 'Read settlement data',
    category: 'read',
  },

  // Write scopes
  {
    scope: 'write:*',
    description: 'Write access to all resources',
    category: 'write',
  },
  {
    scope: 'write:users',
    description: 'Create and update users',
    category: 'write',
  },
  {
    scope: 'write:jobs',
    description: 'Create and update jobs',
    category: 'write',
  },
  {
    scope: 'write:invoices',
    description: 'Create and update invoices',
    category: 'write',
  },
  {
    scope: 'write:bids',
    description: 'Create and update bids',
    category: 'write',
  },
  {
    scope: 'write:settlements',
    description: 'Create and update settlements',
    category: 'write',
  },

  // Admin scopes
  {
    scope: 'admin:keys',
    description: 'Create, rotate, and revoke API keys',
    category: 'admin',
  },
  {
    scope: 'admin:*',
    description: 'Full administrative access',
    category: 'admin',
  },

  // Service scopes
  {
    scope: 'service:ingest',
    description: 'Data ingestion service access',
    category: 'service',
  },
  {
    scope: 'service:export',
    description: 'Data export service access',
    category: 'service',
  },
  {
    scope: 'service:analytics',
    description: 'Analytics service access',
    category: 'service',
  },
  {
    scope: 'service:notifications',
    description: 'Notification service access',
    category: 'service',
  },
];

/**
 * Get all valid scope names
 *
 * Returns a fresh array on every call; callers may reorder or filter their
 * copy without affecting later validations (no shared mutable state).
 */
export function getValidScopes(): string[] {
  return SCOPE_REGISTRY.map(s => s.scope);
}

/**
 * Check if a scope is valid
 *
 * Exact, case-sensitive literal match against the registry. Non-string
 * inputs (null, undefined, numbers, objects, ...) are rejected with `false`
 * rather than throwing, so transport-layer junk cannot crash validation.
 */
export function isValidScope(scope: string): boolean {
  return SCOPE_REGISTRY.some(s => s.scope === scope);
}

/** Result of validating a scope list. Frozen on return. */
export interface ScopeValidationResult {
  /** True only when every entry is a registered scope. */
  readonly valid: boolean;
  /**
   * Rejected entries in input order, duplicates preserved, reported
   * verbatim (no trimming, case-folding, or deduplication).
   */
  readonly invalid: readonly string[];
}

function describeValueType(value: unknown): string {
  if (value === null) return 'null';
  if (Array.isArray(value)) return 'array';
  return typeof value;
}

/**
 * Validate an array of scopes against the registry.
 *
 * Deterministic failure boundary:
 * - Pure function: never mutates the input array or the registry, performs
 *   no I/O, and depends on nothing but its input, so repeated or concurrent
 *   calls with equal input always yield equal results.
 * - `invalid` preserves input order and duplicates for precise attribution.
 * - No normalization: `" READ:users "` is rejected, not silently fixed.
 * - An empty list is valid (no scopes = no grants); whether an empty grant
 *   set is acceptable is a caller policy decision.
 * - The report is frozen; callers cannot mutate their way to a different
 *   validation outcome.
 * - Non-array input is a programming error and throws a TypeError naming
 *   the received type (never a misleading "valid" result).
 */
export function validateScopes(scopes: string[]): ScopeValidationResult {
  if (!Array.isArray(scopes)) {
    throw new TypeError(
      `validateScopes expects an array of scope strings, received ${describeValueType(scopes)}`
    );
  }
  const invalid = Object.freeze(scopes.filter(scope => !isValidScope(scope)));
  return Object.freeze({ valid: invalid.length === 0, invalid });
}

/**
 * A well-formed scope has exactly one `:` separator: `<category>:<resource>`.
 * The category must be a non-empty name, and the resource must be either the
 * `*` wildcard or a non-empty name. Anything else (missing/extra colons,
 * empty segments, surrounding whitespace, non-string values) is malformed and
 * is handled by the fail-closed rules documented on `hasRequiredScopes`.
 */
const WELL_FORMED_SCOPE = /^[A-Za-z][A-Za-z0-9_-]*:(?:\*|[A-Za-z0-9][A-Za-z0-9_-]*)$/;

function isWellFormedScope(scope: unknown): scope is string {
  return typeof scope === 'string' && WELL_FORMED_SCOPE.test(scope);
}

/**
 * Normalize the *granted* scope list into a de-duplicated array of non-empty
 * strings. Non-array input and non-string entries are discarded rather than
 * thrown on, so a malformed credential record can never crash an
 * authorization check. Discarded grants can only ever *deny* access, which is
 * the safe direction for an authorization boundary.
 */
function normalizeGrantedScopes(scopes: unknown): string[] {
  if (!Array.isArray(scopes)) {
    return [];
  }

  const normalized = new Set<string>();
  for (const scope of scopes) {
    if (typeof scope === 'string' && scope.length > 0) {
      normalized.add(scope);
    }
  }

  return Array.from(normalized);
}

/**
 * A required scope list is evaluable only when it is an array in which every
 * entry is a well-formed scope. Malformed requirements are rejected instead of
 * being silently coerced, so they can never be satisfied by a wildcard.
 */
function hasOnlyWellFormedScopes(scopes: unknown): scopes is string[] {
  return Array.isArray(scopes) && scopes.every(isWellFormedScope);
}

/**
 * Check if a set of granted scopes satisfies the required scopes.
 *
 * Deterministic semantics (all outcomes are pure functions of the inputs):
 * - An empty `requiredScopes` list is always satisfied (nothing to enforce).
 * - A malformed or non-array `requiredScopes` list always fails closed.
 * - `admin:*` grants every well-formed required scope.
 * - `<category>:*` grants every well-formed scope in that category
 *   (e.g. `read:*` satisfies `read:users`).
 * - Otherwise every required scope must match exactly or via its category
 *   wildcard.
 *
 * Failure boundaries:
 * - Inputs are never mutated, so a caller may safely reuse the arrays and the
 *   function may be invoked concurrently.
 * - Duplicates and ordering do not affect the result.
 * - Malformed grants are ignored (deny-only); malformed requirements deny.
 * - The function never throws for any input, including `null`/`undefined`.
 */
export function hasRequiredScopes(grantedScopes: string[], requiredScopes: string[]): boolean {
  // Defensively ignore malformed granted scope lists (deny-only direction).
  const granted = new Set(normalizeGrantedScopes(grantedScopes));

  // An explicit, empty requirement list is always satisfied.
  if (Array.isArray(requiredScopes) && requiredScopes.length === 0) {
    return true;
  }

  // A missing or malformed requirement cannot be evaluated: fail closed.
  if (!hasOnlyWellFormedScopes(requiredScopes)) {
    return false;
  }

  // Check for admin:* which grants everything
  if (granted.has('admin:*')) {
    return true;
  }

  for (const requiredScope of requiredScopes) {
    // Check for exact match
    if (granted.has(requiredScope)) {
      continue;
    }

    // Check for wildcard match (e.g., read:* covers read:users). The category
    // is derived from the already-validated requirement, so indexOf is safe.
    const category = requiredScope.slice(0, requiredScope.indexOf(':'));
    if (granted.has(`${category}:*`)) {
      continue;
    }

    // Required scope not found
    return false;
  }

  return true;
}

/**
 * The canonical set of scope categories. This is the single source of truth
 * for what constitutes a valid category and is used at runtime to validate
 * inputs to getScopesByCategory. It is derived from the ScopeDefinition
 * type so the two cannot drift apart.
 */
export const SCOPE_CATEGORIES = ['read', 'write', 'admin', 'service'] as const satisfies ReadonlyArray<ScopeDefinition['category']>;

export type ScopeCategory = (typeof SCOPE_CATEGORIES)[number];

/**
 * Error thrown when getScopesByCategory receives an unsupported category.
 *
 * This is a deterministic, typed failure boundary: callers can catch it
 * and map it to a 400 response without exposing internal state. The error
 * message is static and never echoes the raw input, so it cannot be
 * used for log injection or to leak attacker-controlled data.
 */
export class InvalidScopeCategoryError extends Error {
  readonly code = 'INVALID_SCOPE_CATEGORY' as const;
  readonly category: unknown;

  constructor(category: unknown) {
    super(`Unknown scope category: ${SCOPE_CATEGORIES.join(', ')} are the only valid categories`);
    this.name = 'InvalidScopeCategoryError';
    this.category = category;
    // Restore prototype chain for TS/Babel downlevel targets.
    Object.setPrototypeOf(this, InvalidScopeCategoryError.prototype);
  }
}

/**
 * Runtime type guard for a valid scope category.
 */
export function isScopeCategory(value: unknown): value is ScopeCategory {
  return (typeof value === 'string') && (SCOPE_CATEGORIES as readonly string[]).includes(value);
}

/**
 * Get scope definitions by category.
 *
 * Invariants:
*  - The returned array is always a fresh array (never a reference to
 *    the internal SCOPE_REGISTRY), so callers cannot mutate registry state.
 *  - Elements are the canonical ScopeDefinition objects; they are treated
 *    as immutable by convention and the registry is not mutated at
 *    runtime.
 *  - Ordering matches SCOPE_REGISTRY declaration order, making the
 *    result deterministic and reproducible across runs and processes.
 *  - Invalid or unknown categories throw InvalidScopeCategoryError rather
 *    than silently returning an empty array, which would hide caller bugs
 *    and could lead to authorization decisions based on missing data.
 */
export function getScopesByCategory(category: ScopeDefinition['category']): ScopeDefinition[] {
  if (!isScopeCategory(category)) {
    throw new InvalidScopeCategoryError(category);
  }

  // Always return a fresh array so callers cannot corrupt the registry.
  return SCOPE_REGISTRY.filter(s => s.category === category);
}

/**
 * Map a set of granted scopes to an administrative role.
 * Returns an `AdminRole` string when the scopes confer admin privileges,
 * or `null` when no administrative role is implied.
 *
 * Invariants:
 * - Pure function: deterministic, no side-effects, no shared mutable state.
 * - Priority is fixed: super_admin > operations_admin > support > null.
 *   Adding more scopes can only maintain or increase the resolved role.
 * - Only exact string matches are used — no trimming, case-folding, or
 *   prefix matching. Near-matches ("ADMIN:*", "admin: *") are not elevated.
 * - Non-array runtime values (null, undefined, string, …) return null
 *   without throwing, since this function is called from network paths
 *   where input types cannot be fully guaranteed at runtime.
 * - `security_admin` exists in AdminRole but no scope currently maps to it.
 *   Any future mapping must be a deliberate, reviewed change.
 */
import { AdminRole } from "../types/rbac";

export function roleFromScopes(grantedScopes: string[]): AdminRole | null {
  // Guard: non-array runtime values must not throw
  if (!Array.isArray(grantedScopes)) return null;

  // Full admin grants highest privilege — checked first so it always wins
  if (grantedScopes.includes('admin:*')) return 'super_admin';

  // Operations-level privileges: management scopes or write:*
  if (grantedScopes.includes('write:*') || grantedScopes.includes('admin:keys')) {
    return 'operations_admin';
  }

  // Support-level privileges: read access
  if (grantedScopes.includes('read:*')) return 'support';

  return null;
}
