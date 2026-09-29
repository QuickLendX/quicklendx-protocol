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
 * Check if a set of granted scopes satisfies required scopes
 * Supports wildcard matching (e.g., read:* matches read:users)
 */
export function hasRequiredScopes(grantedScopes: string[], requiredScopes: string[]): boolean {
  // Check for admin:* which grants everything
  if (grantedScopes.includes('admin:*')) {
    return true;
  }

  for (const required of requiredScopes) {
    const [category, resource] = required.split(':');
    
    // Check for exact match
    if (grantedScopes.includes(required)) {
      continue;
    }

    // Check for wildcard match (e.g., read:* covers read:users)
    const wildcardScope = `${category}:*`;
    if (grantedScopes.includes(wildcardScope)) {
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
