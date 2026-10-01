import * as fc from 'fast-check';
import {
  SCOPE_REGISTRY,
  getValidScopes,
  isValidScope,
  validateScopes,
  hasRequiredScopes,
  getScopesByCategory,
  roleFromScopes,
} from '../config/scopes';

/**
 * Deterministic failure-boundary coverage for `hasRequiredScopes`.
 *
 * The suite locks in four classes of guarantees:
 *   1. success   - well-formed granted scopes satisfy well-formed requirements
 *   2. rejection - missing/insufficient grants deny, unknown scopes deny
 *   3. boundary  - empty/duplicate/malformed/non-array inputs are deterministic
 *   4. regression - malformed requirements can never be satisfied by a wildcard
 *
 * Everything asserted here is a pure function of its inputs, so the suite is
 * safe to run repeatedly and in parallel.
 */

const ALL_CATEGORIES = ['read', 'write', 'admin', 'service'] as const;

describe('scope registry', () => {
  it('exposes a non-empty registry with unique scope names', () => {
    expect(SCOPE_REGISTRY.length).toBeGreaterThan(0);
    const names = getValidScopes();
    expect(new Set(names).size).toBe(names.length);
    expect(names).toEqual(SCOPE_REGISTRY.map(s => s.scope));
  });

  it('only contains well-formed `<category>:<resource>` scopes', () => {
    // Mirrors the structural invariant enforced inside `hasRequiredScopes`.
    const wellFormed = /^[A-Za-z][A-Za-z0-9_-]*:(?:\*|[A-Za-z0-9][A-Za-z0-9_-]*)$/;
    for (const scope of getValidScopes()) {
      expect(scope).toMatch(wellFormed);
    }
  });

  it('groups registry entries by category', () => {
    for (const category of ALL_CATEGORIES) {
      const scoped = getScopesByCategory(category);
      expect(scoped.length).toBeGreaterThan(0);
      expect(scoped.every(s => s.category === category)).toBe(true);
    }
    expect(
      ALL_CATEGORIES.reduce((sum, c) => sum + getScopesByCategory(c).length, 0)
    ).toBe(SCOPE_REGISTRY.length);
  });
});

describe('isValidScope / validateScopes', () => {
  it('accepts every registered scope', () => {
    for (const scope of getValidScopes()) {
      expect(isValidScope(scope)).toBe(true);
    }
  });

  it('rejects unknown, malformed, and empty scopes', () => {
    for (const scope of ['', 'read', ':users', 'read:', 'read:users:extra', ' READ:users']) {
      expect(isValidScope(scope)).toBe(false);
    }
  });

  it('reports an empty list as valid', () => {
    expect(validateScopes([])).toEqual({ valid: true, invalid: [] });
  });

  it('reports all invalid entries without mutating the input', () => {
    const input = ['read:users', 'nope', 'read:*', 'also:nope'];
    const snapshot = [...input];

    expect(validateScopes(input)).toEqual({
      valid: false,
      invalid: ['nope', 'also:nope'],
    });
    expect(input).toEqual(snapshot);
  });
});

describe('hasRequiredScopes — success paths', () => {
  it('matches an exact granted scope', () => {
    expect(hasRequiredScopes(['read:users'], ['read:users'])).toBe(true);
  });

  it('matches every required scope regardless of order', () => {
    expect(
      hasRequiredScopes(['write:invoices', 'read:users'], ['read:users', 'write:invoices'])
    ).toBe(true);
  });

  it('treats `admin:*` as a superset of every well-formed requirement', () => {
    expect(
      hasRequiredScopes(['admin:*'], ['read:users', 'write:jobs', 'admin:keys', 'service:export'])
    ).toBe(true);
  });

  it('lets a category wildcard satisfy that category only', () => {
    expect(hasRequiredScopes(['read:*'], ['read:users', 'read:invoices'])).toBe(true);
    expect(hasRequiredScopes(['write:*'], ['write:bids'])).toBe(true);
    expect(hasRequiredScopes(['service:*'], ['service:ingest', 'service:analytics'])).toBe(true);
  });

  it('allows a requirement list that is explicitly empty', () => {
    expect(hasRequiredScopes([], [])).toBe(true);
    expect(hasRequiredScopes(['read:users'], [])).toBe(true);
  });
});

describe('hasRequiredScopes — rejection paths', () => {
  it('denies when the granted list is empty', () => {
    expect(hasRequiredScopes([], ['read:users'])).toBe(false);
  });

  it('denies an unknown requirement even when other scopes are granted', () => {
    expect(hasRequiredScopes(['read:users'], ['read:users', 'write:invoices'])).toBe(false);
    expect(hasRequiredScopes(['read:*'], ['write:invoices'])).toBe(false);
  });

  it('does not let a wildcard cross categories', () => {
    expect(hasRequiredScopes(['read:*'], ['write:users'])).toBe(false);
    expect(hasRequiredScopes(['write:*'], ['read:users'])).toBe(false);
    expect(hasRequiredScopes(['admin:keys'], ['admin:*'])).toBe(false);
  });

  it('does not let a concrete grant satisfy a wildcard requirement', () => {
    expect(hasRequiredScopes(['read:users'], ['read:*'])).toBe(false);
    expect(hasRequiredScopes(['read:users', 'read:jobs'], ['read:*'])).toBe(false);
  });

  it('denies when only some of the required scopes are granted', () => {
    expect(hasRequiredScopes(['read:users'], ['read:users', 'admin:keys'])).toBe(false);
  });
});

describe('hasRequiredScopes — boundary & robustness', () => {
  it('is order-independent for granted and required lists', () => {
    const granted = ['write:invoices', 'read:*', 'service:ingest'];
    expect(hasRequiredScopes(granted, ['read:users', 'write:invoices'])).toBe(
      hasRequiredScopes([...granted].reverse(), ['write:invoices', 'read:users'])
    );
  });

  it('is unaffected by duplicate grants or duplicate requirements', () => {
    expect(hasRequiredScopes(['read:users'], ['read:users', 'read:users'])).toBe(true);
    expect(hasRequiredScopes(['read:users', 'read:users'], ['read:users'])).toBe(true);
    expect(hasRequiredScopes(['admin:*', 'admin:*'], ['read:users'])).toBe(true);
    expect(hasRequiredScopes(['read:users'], ['read:users', 'write:bids', 'write:bids'])).toBe(
      false
    );
  });

  it('ignores malformed, empty, and non-string grants (deny-only direction)', () => {
    const malformedGrants = [
      'read:',
      ':users',
      'read:users:extra',
      ' read:users',
      '',
      'READ:USERS',
    ];
    for (const bad of malformedGrants) {
      expect(hasRequiredScopes([bad], ['read:users'])).toBe(false);
    }
    // Non-string entries are discarded rather than trusted.
    expect(hasRequiredScopes([42, null, undefined, {}] as unknown as string[], ['read:users'])).toBe(
      false
    );
    // ...but they cannot poison a list that also contains a valid grant.
    expect(hasRequiredScopes(['read:users', 42] as unknown as string[], ['read:users'])).toBe(true);
  });

  it('fails closed for malformed requirement entries', () => {
    for (const bad of ['read', 'read:', ':users', 'read:users:extra', ' read:users', '']) {
      expect(hasRequiredScopes(['read:*'], [bad])).toBe(false);
    }
    expect(hasRequiredScopes([], [''])).toBe(false);
  });

  it('fails closed when the requirement list is malformed even if admin:* is granted', () => {
    // Regression: `admin:*` must not paper over a requirement that cannot be
    // evaluated, otherwise the outcome would depend on malformed input.
    expect(hasRequiredScopes(['admin:*'], ['read:users:extra'])).toBe(false);
    expect(hasRequiredScopes(['admin:*'], [''])).toBe(false);
  });

  it('never throws for arbitrary non-array inputs and fails closed', () => {
    const unusable = [undefined, null, 0, '', 'read:users', {}, () => {}, Symbol('x')];
    for (const value of unusable) {
      expect(() => hasRequiredScopes(value as unknown as string[], value as unknown as string[])).not.toThrow();
    }
    // A non-array requirement list must deny, not silently allow.
    expect(hasRequiredScopes(['admin:*'], undefined as unknown as string[])).toBe(false);
    expect(hasRequiredScopes(undefined as unknown as string[], ['read:users'])).toBe(false);
    // Non-array grants are treated as "no grants".
    expect(hasRequiredScopes('read:users' as unknown as string[], ['read:users'])).toBe(false);
  });

  it('does not mutate its inputs', () => {
    const granted = ['read:users', 'read:users'];
    const required = ['read:users', 'write:bids'];
    const grantedSnapshot = [...granted];
    const requiredSnapshot = [...required];

    hasRequiredScopes(granted, required);

    expect(granted).toEqual(grantedSnapshot);
    expect(required).toEqual(requiredSnapshot);
  });

  it('is deterministic across repeated and concurrent invocation', async () => {
    const granted = ['read:*', 'write:invoices'];
    const required = ['read:users', 'write:invoices'];

    const repeats = Array.from({ length: 50 }, () => hasRequiredScopes(granted, required));
    expect(new Set(repeats)).toEqual(new Set([true]));

    const concurrent = await Promise.all(
      Array.from({ length: 100 }, () =>
        Promise.resolve().then(() => hasRequiredScopes(granted, required))
      )
    );
    expect(concurrent.every(result => result === true)).toBe(true);
  });
});

describe('hasRequiredScopes — property-based invariants', () => {
  it('never throws and always returns a boolean for arbitrary input', () => {
    fc.assert(
      fc.property(fc.anything(), fc.anything(), (granted, required) => {
        let result: unknown;
        expect(() => {
          result = hasRequiredScopes(granted as string[], required as string[]);
        }).not.toThrow();
        expect(typeof result).toBe('boolean');
      }),
      { numRuns: 500 }
    );
  });

  it('is order-independent for arbitrary string arrays', () => {
    fc.assert(
      fc.property(
        fc.array(fc.string(), { maxLength: 8 }),
        fc.array(fc.string(), { maxLength: 8 }),
        (granted, required) => {
          expect(hasRequiredScopes(granted, required)).toBe(
            hasRequiredScopes([...granted].reverse(), [...required].reverse())
          );
        }
      ),
      { numRuns: 500 }
    );
  });

  it('is idempotent under duplication of either list', () => {
    fc.assert(
      fc.property(
        fc.array(fc.string(), { maxLength: 8 }),
        fc.array(fc.string(), { maxLength: 8 }),
        (granted, required) => {
          expect(hasRequiredScopes([...granted, ...granted], [...required, ...required])).toBe(
            hasRequiredScopes(granted, required)
          );
        }
      ),
      { numRuns: 500 }
    );
  });

  it('is monotonic: adding a grant never revokes an existing decision', () => {
    fc.assert(
      fc.property(
        fc.array(fc.string(), { maxLength: 8 }),
        fc.array(fc.string(), { maxLength: 8 }),
        fc.string(),
        (granted, required, extra) => {
          const before = hasRequiredScopes(granted, required);
          const after = hasRequiredScopes([...granted, extra], required);
          if (before) {
            expect(after).toBe(true);
          }
        }
      ),
      { numRuns: 500 }
    );
  });
});

describe('roleFromScopes', () => {
  it('maps admin:* to super_admin', () => {
    expect(roleFromScopes(['admin:*'])).toBe('super_admin');
    expect(roleFromScopes(['read:*', 'write:*', 'admin:*'])).toBe('super_admin');
  });

  it('maps write:* and admin:keys to operations_admin', () => {
    expect(roleFromScopes(['write:*'])).toBe('operations_admin');
    expect(roleFromScopes(['admin:keys'])).toBe('operations_admin');
    expect(roleFromScopes(['write:invoices', 'admin:keys'])).toBe('operations_admin');
  });

  it('maps read:* to support', () => {
    expect(roleFromScopes(['read:*'])).toBe('support');
    // Only the read wildcard confers support; a concrete read scope does not.
    expect(roleFromScopes(['read:users'])).toBeNull();
  });

  it('returns null when no administrative privilege is implied', () => {
    expect(roleFromScopes([])).toBeNull();
    expect(roleFromScopes(['read:users', 'service:ingest'])).toBeNull();
  });
});
