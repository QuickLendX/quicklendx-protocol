/**
 * Scope Registry - Defines all valid API key scopes
 */

import { AdminRole } from "../types/rbac";

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
 * Error thrown when the scope registry is invalid or corrupted.
 * This is a fail-fast condition: the registry is a compile-time constant,
 * so any violation indicates a programming error that must not be silently
 * tolerated (e.g. duplicate scopes would make authorization ambiguous).
 */
export class ScopeRegistryError extends Error {
  constructor(message: string, public reason: string) {
    super(message);
    this.name = 'ScopeRegistryError';
  }
}

interface RegistryValidationResult {
  valid: boolean;
  reason?: string;
}

/**
 * Validate the static scope registry invariants.
 *
 * Invariants:
*  1. Every entry must have a non-empty scope string.
 *  2. Scope names must be unique (duplicates make authorization ambiguous).
 *  3. Every scope must be of the form `<category>:<resource>` with a non-empty
 *     resource segment.
 *  4. Wildcard scopes (`</category>:*`) are only allowed for the categories
 *     that declare them (read, write, admin).
 *  5. The category field must match the prefix of the scope string.
 */
export function validateRegistry(
  registry: Readonly<ScopeDefinition[]> = SCOPE_REGISTRY,
): RegistryValidationResult {
  if (!Array.isArray(registry)) {
    return { valid: false, reason: 'registry_not_an_array' };
  }

  const seen = new Set<string>();
  const allowedWildcardCategories = new Set(['read', 'write', 'admin']);

  for (const entry of registry) {
    if (!entry || typeof entry.scope !== 'string') {
      return { valid: false, reason: 'entry_missing_scope' };
    }

    const scope = entry.scope;
    if (scope.length === 0) {
      return { valid: false, reason: 'empty_scope_name' };
    }

    if (seen.has(scope)) {
      return { valid: false, reason: `uplease_replace_duplicate_scope:${scope}` };
    }
    seen.add(scope);

    const separatorIdx = scope.indexOf(':');
    if (separatorIdx <= 0 || separatorIdx === scope.length - 1) {
      return { valid: false, reason: `malformed_scope:${scope}` };
    }

    const category = scope.slice(0, separatorIdx);
    const resource = scope.slice(separatorIdx + 1);

    if (category !== entry.category) {
      return {
        valid: false,
        reason: `category_mismatch:${scope}:expected:${entry.category}:got:${category}`,
      };
    }

    if (resource === '*' && !allowedWildcardCategories.has(category)) {
      return { valid: false, reason: `wildcard_not_allowed:${scope}` };
    }
  }

  return { valid: true };
}

/**
 * Cached result of registry validation. Computed lazily on first access so that
 * import-side effects are avoided and tests can control when validation runs.
 */
let cachedValidation: RegistryValidationResult | undefined;

function getRegistryValidation(): RegistryValidationResult {
  if (!cachedValidation) {
    cachedValidation = validateRegistry(SCOPE_REGISTRY);
  }
  return cachedValidation;
}

/**
 * Reset the cached registry validation. Exposed for tests and for code that
 * mutates the registry at runtime (e.g. dynamic scope loading).
 */
export function resetRegistryValidationCache(): void {
  cachedValidation = undefined;
}

/**
 * Ensure the registry is valid, throwing a deterministic error otherwise.
 * This is the fail-fast boundary used by `getValidScopes` and friends.
 */
export function assertRegistryValid(): void {
  const result = getRegistryValidation();
  if (!result.valid) {
    throw new ScopeRegistryError(
      `Scope registry is invalid: ${result.reason}`,
      result.reason ?? 'unknown_registry_error',
    );
  }
}

/**
 * Get all valid scope names.
 *
 * Behavior:
 *  - Returns a defensive copy of the scope names in registry order.
 *  - The registry is validated before returning; an invalid registry throws
 *    a `ScopeRegistryError` rather than silently returning bad data.
 *  - The returned array is a fresh copy on every call, so callers cannot
 *    mutate shared state.
 */
export function getValidScopes(): string[] {
  assertRegistryValid();
  return SCOPE_REGISTRY.map(s => s.scope);
}

/**
 * Check if a scope is valid.
 *
 * This function is defensive: any non-string or empty input is rejected
 * without throwing, and the registry is validated before any lookup.
 */
export function isValidScope(scope: string): boolean {
  assertRegistryValid();
  if (typeof scope !== 'string' || scope.length === 0) {
    return false;
  }
  return SCOPE_REGISTRY.some(s => s.scope === scope);
}

/**
 * Validate an array of scopes.
 *
 * Behavior:
 *  - Non-array input is rejected deterministically with an empty invalid list.
 *  - Non-string entries are reported as invalid without throwing.
 *  - Duplicates are preserved in the input order but deduplicated in the
 *    reported invalid list so callers get a stable diagnostic surface.
 *  - The registry is validated before any lookup.
 */
export function validateScopes(scopes: string[]): { valid: boolean; invalid: string[] } {
  assertRegistryValid();

  if (!Array.isArray(scopes)) {
    return { valid: false, invalid: [] };
  }

  const invalidSet = new Set<string>();
  for (const scope of scopes) {
    if (!isValidScope(scope)) {
      invalidSet.add(typeof scope === 'string' ? scope : String(scope));
    }
  }

  const invalid = Array.from(invalidSet);
  return {
    valid: invalid.length === 0,
    invalid,
  };
}

/**
 * Check if a set of granted scopes satisfies required scopes.
 * Supports wildcard matching (e.g., read:* matches read:users).
 *
 * Behavior:
 *  - Non-array inputs are rejected deterministically (false).
 *  - Empty required list is always satisfied.
 *  - Empty granted list can only satisfy an empty required list.
 *  - Malformed required scopes (no `:`) are treated as unsatisfiable.
 *  - `admin:*` grants everything.
 */
export function hasRequiredScopes(grantedScopes: string[], requiredScopes: string[]): boolean {
  assertRegistryValid();

  if (!Array.isArray(grantedScopes) || !Array.isArray(requiredScopes)) {
    return false;
  }

  if (requiredScopes.length === 0) {
    return true;
  }

  // Check for admin:*' which grants everything
  if (grantedScopes.includes('admin:*')) {
    return true;
  }

  const grantedSet = new Set(grantedScopes);

  for (const required of requiredScopes) {
    if (typeof required !== 'string' || required.length === 0) {
      return false;
    }

    const separatorIdx = required.indexOf(':');
    if (separatorIdx <= 0 || separatorIdx === required.length - 1) {
      // Malformed required scope cannot be satisfied.
      return false;
    }

    const category = required.slice(0, separatorIdx);

    // Check for exact match
    if (grantedSet.has(required)) {
      continue;
    }

    // Check for wildcard match (e.g., read:* covers read:users)
    const wildcardScope = `${category}:*`;
    if (grantedSet.has(wildcardScope)) {
      continue;
    }

    // Required scope not found
    return false;
  }

  return true;
}

/**
 * Get scope definitions by category.
 *
 * Returns a defensive copy of the matching entries so callers cannot mutate
 * the shared registry.
 */
export function getScopesByCategory(category: ScopeDefinition['category']): ScopeDefinition[] {
  assertRegistryValid();
  if (typeof category !== 'string') {
    return [];
  }
  return SCOPE_REGISTRY.filter(s => s.category === category).map(s => ({ ...s }));
}

/**
 * Map a set of granted scopes to an administrative role.
 * Returns an `AdminRole` string when the scopes confer admin privileges,
 * or `null` when no administrative role is implied.
 */
export function roleFromScopes(grantedScopes: string[]): AdminRole | null {
  assertRegistryValid();

  if (!Array.isArray(grantedScopes)) {
    return null;
  }

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
