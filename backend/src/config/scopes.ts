/**
 * Scope Registry - Defines all valid API key scopes
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
 */
export function getValidScopes(): string[] {
  return SCOPE_REGISTRY.map(s => s.scope);
}

/**
 * Check if a scope is valid
 */
export function isValidScope(scope: string): boolean {
  return SCOPE_REGISTRY.some(s => s.scope === scope);
}

/**
 * Validate an array of scopes
 */
export function validateScopes(scopes: string[]): { valid: boolean; invalid: string[] } {
  const invalid = scopes.filter(scope => !isValidScope(scope));
  return {
    valid: invalid.length === 0,
    invalid,
  };
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
 * Get scope definitions by category
 */
export function getScopesByCategory(category: ScopeDefinition['category']): ScopeDefinition[] {
  return SCOPE_REGISTRY.filter(s => s.category === category);
}

/**
 * Map a set of granted scopes to an administrative role.
 * Returns an `AdminRole` string when the scopes confer admin privileges,
 * or `null` when no administrative role is implied.
 */
import { AdminRole } from "../types/rbac";

export function roleFromScopes(grantedScopes: string[]): AdminRole | null {
  // Full admin grants highest privilege
  if (grantedScopes.includes('admin:*')) return 'super_admin';

  // Operations-level privileges: management scopes or write:*
  if (grantedScopes.includes('write:*') || grantedScopes.includes('admin:keys')) {
    return 'operations_admin';
  }

  // Support-level privileges: read access
  if (grantedScopes.includes('read:*')) return 'support';

  return null;
}
