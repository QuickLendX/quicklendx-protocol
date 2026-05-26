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
 * Get scope definitions by category
 */
export function getScopesByCategory(category: ScopeDefinition['category']): ScopeDefinition[] {
  return SCOPE_REGISTRY.filter(s => s.category === category);
}
