import {
  isValidScope,
  getValidScopes,
  validateScopes,
  roleFromScopes,
  SCOPE_REGISTRY,
} from "../src/config/scopes";
import { ADMIN_ROLES, AdminRole } from "../src/types/rbac";

describe("isValidScope", () => {
  describe("valid scopes (success path)", () => {
    it("should accept every scope registered in SCOPE_REGISTRY", () => {
      for (const definition of SCOPE_REGISTRY) {
        expect(isValidScope(definition.scope)).toBe(true);
      }
    });

    it("should accept every scope returned by getValidScopes()", () => {
      const validScopes = getValidScopes();
      expect(validScopes.length).toBeGreaterThan(0);
      for (const scope of validScopes) {
        expect(isValidScope(scope)).toBe(true);
      }
    });

    it("should accept all read scopes", () => {
      expect(isValidScope("read:*")).toBe(true);
      expect(isValidScope("read:users")).toBe(true);
      expect(isValidScope("read:jobs")).toBe(true);
      expect(isValidScope("read:invoices")).toBe(true);
      expect(isValidScope("read:bids")).toBe(true);
      expect(isValidScope("read:settlements")).toBe(true);
    });

    it("should accept all write scopes", () => {
      expect(isValidScope("write:*")).toBe(true);
      expect(isValidScope("write:users")).toBe(true);
      expect(isValidScope("write:jobs")).toBe(true);
      expect(isValidScope("write:invoices")).toBe(true);
      expect(isValidScope("write:bids")).toBe(true);
      expect(isValidScope("write:settlements")).toBe(true);
    });

    it("should accept all admin scopes", () => {
      expect(isValidScope("admin:keys")).toBe(true);
      expect(isValidScope("admin:*")).toBe(true);
    });

    it("should accept all service scopes", () => {
      expect(isValidScope("service:ingest")).toBe(true);
      expect(isValidScope("service:export")).toBe(true);
      expect(isValidScope("service:analytics")).toBe(true);
      expect(isValidScope("service:notifications")).toBe(true);
    });
  });

  describe("rejection of unknown scopes", () => {
    it("should reject unknown resources under a valid category", () => {
      expect(isValidScope("read:unknown")).toBe(false);
      expect(isValidScope("write:unknown")).toBe(false);
      expect(isValidScope("admin:unknown")).toBe(false);
      expect(isValidScope("service:unknown")).toBe(false);
    });

    it("should reject unknown categories with a valid resource", () => {
      expect(isValidScope("delete:users")).toBe(false);
      expect(isValidScope("execute:jobs")).toBe(false);
      expect(isValidScope("list:invoices")).toBe(false);
      expect(isValidScope("get:bids")).toBe(false);
    });

    it("should reject fully unrelated strings", () => {
      expect(isValidScope("foo")).toBe(false);
      expect(isValidScope("scope")).toBe(false);
      expect(isValidScope("admin")).toBe(false);
      expect(isValidScope("read")).toBe(false);
      expect(isValidScope("users")).toBe(false);
      expect(isValidScope("superuser")).toBe(false);
    });
  });

  describe("boundary conditions", () => {
    it("should reject the empty string", () => {
      expect(isValidScope("")).toBe(false);
    });

    it("should reject whitespace-only strings", () => {
      expect(isValidScope(" ")).toBe(false);
      expect(isValidScope("  ")).toBe(false);
      expect(isValidScope("\t")).toBe(false);
      expect(isValidScope("\n")).toBe(false);
      expect(isValidScope("\r\n")).toBe(false);
    });

    it("should reject scopes with leading or trailing whitespace", () => {
      expect(isValidScope(" read:users")).toBe(false);
      expect(isValidScope("read:users ")).toBe(false);
      expect(isValidScope(" read:users ")).toBe(false);
      expect(isValidScope("read:users\n")).toBe(false);
      expect(isValidScope("read:users\t")).toBe(false);
    });

    it("should reject case variations (matching is case-sensitive)", () => {
      expect(isValidScope("READ:USERS")).toBe(false);
      expect(isValidScope("Read:Users")).toBe(false);
      expect(isValidScope("read:USERS")).toBe(false);
      expect(isValidScope("read:Users")).toBe(false);
      expect(isValidScope("ADMIN:*")).toBe(false);
      expect(isValidScope("Service:Ingest")).toBe(false);
    });

    it("should reject scopes missing the colon separator", () => {
      expect(isValidScope("readusers")).toBe(false);
      expect(isValidScope("adminkeys")).toBe(false);
      expect(isValidScope("serviceingest")).toBe(false);
      expect(isValidScope("read*")).toBe(false);
    });

    it("should reject scopes with empty segments", () => {
      expect(isValidScope("read:")).toBe(false);
      expect(isValidScope(":users")).toBe(false);
      expect(isValidScope(":")).toBe(false);
      expect(isValidScope("write:")).toBe(false);
      expect(isValidScope("admin:")).toBe(false);
    });

    it("should reject scopes with extra colons", () => {
      expect(isValidScope("read:users:extra")).toBe(false);
      expect(isValidScope("read::users")).toBe(false);
      expect(isValidScope("read:users:")).toBe(false);
      expect(isValidScope(":::")).toBe(false);
    });

    it("should reject wildcard misuse", () => {
      expect(isValidScope("*:users")).toBe(false);
      expect(isValidScope("*:jobs")).toBe(false);
      expect(isValidScope("read:**")).toBe(false);
      expect(isValidScope("*:*")).toBe(false);
      expect(isValidScope("*")).toBe(false);
      expect(isValidScope("read:*:extra")).toBe(false);
    });

    it("should reject near-miss strings differing by a single character", () => {
      expect(isValidScope("read:user")).toBe(false);
      expect(isValidScope("read:userss")).toBe(false);
      expect(isValidScope("ead:users")).toBe(false);
      expect(isValidScope("rread:users")).toBe(false);
      expect(isValidScope("read:users\x00")).toBe(false);
      expect(isValidScope("read:users\u200b")).toBe(false);
    });

    it("should reject unicode and special-character lookalikes", () => {
      expect(isValidScope("read:usérs")).toBe(false);
      expect(isValidScope("read:users🔒")).toBe(false);
      expect(isValidScope("read:users/")).toBe(false);
      expect(isValidScope("read:users\\")).toBe(false);
      expect(isValidScope("read:users;drop")).toBe(false);
    });

    it("should reject very long strings", () => {
      expect(isValidScope("a".repeat(10000))).toBe(false);
      expect(isValidScope(`read:${"a".repeat(10000)}`)).toBe(false);
    });
  });

  describe("non-string inputs (failure boundary)", () => {
    it("should reject null without throwing", () => {
      expect(() => isValidScope(null as unknown as string)).not.toThrow();
      expect(isValidScope(null as unknown as string)).toBe(false);
    });

    it("should reject undefined without throwing", () => {
      expect(() => isValidScope(undefined as unknown as string)).not.toThrow();
      expect(isValidScope(undefined as unknown as string)).toBe(false);
    });

    it("should reject numbers without throwing", () => {
      expect(isValidScope(0 as unknown as string)).toBe(false);
      expect(isValidScope(1 as unknown as string)).toBe(false);
      expect(isValidScope(-1 as unknown as string)).toBe(false);
      expect(isValidScope(NaN as unknown as string)).toBe(false);
      expect(isValidScope(Infinity as unknown as string)).toBe(false);
    });

    it("should reject booleans without throwing", () => {
      expect(isValidScope(true as unknown as string)).toBe(false);
      expect(isValidScope(false as unknown as string)).toBe(false);
    });

    it("should reject objects without throwing", () => {
      expect(isValidScope({} as unknown as string)).toBe(false);
      expect(
        isValidScope({ scope: "read:users" } as unknown as string)
      ).toBe(false);
    });

    it("should reject arrays without throwing", () => {
      expect(isValidScope([] as unknown as string)).toBe(false);
      expect(
        isValidScope(["read:users"] as unknown as string)
      ).toBe(false);
    });

    it("should reject functions without throwing", () => {
      expect(
        isValidScope((() => "read:users") as unknown as string)
      ).toBe(false);
    });
  });

  describe("determinism", () => {
    it("should return identical results across repeated calls", () => {
      const inputs = [
        "read:users",
        "admin:*",
        "bogus",
        "",
        "READ:USERS",
        " read:users",
      ];
      for (const input of inputs) {
        const first = isValidScope(input);
        for (let i = 0; i < 10; i++) {
          expect(isValidScope(input)).toBe(first);
        }
      }
    });

    it("should not mutate the scope registry", () => {
      const before = getValidScopes();
      isValidScope("read:users");
      isValidScope("bogus");
      isValidScope("");
      isValidScope(null as unknown as string);
      expect(getValidScopes()).toEqual(before);
    });

    it("should produce results independent of call order", () => {
      const forward = [
        "read:users",
        "bogus",
        "admin:*",
        "",
        "write:bids",
      ].map(isValidScope);
      const backward = [
        "write:bids",
        "",
        "admin:*",
        "bogus",
        "read:users",
      ].map(isValidScope);
      expect(forward).toEqual(backward.reverse());
    });
  });
});

/**
 * Deterministic failure-boundary coverage for validateScopes.
 *
 * Contract under test (see backend/src/config/scopes.ts):
 * - Pure: equal inputs yield equal results; input array and registry are
 *   never mutated; repeated/concurrent calls are safe.
 * - `invalid` preserves input order, duplicates, and verbatim values.
 * - Empty list is valid; no normalization or deduplication is applied.
 * - Non-array input throws a TypeError naming the received type.
 * - The returned report is frozen and cannot be mutated after the fact.
 */
describe("validateScopes (deterministic failure boundaries)", () => {
  describe("success paths", () => {
    it("should mark an all-scope set as valid", () => {
      const result = validateScopes(getValidScopes());
      expect(result.valid).toBe(true);
      expect(result.invalid).toEqual([]);
    });

    it("should mark every single registered scope as valid on its own", () => {
      for (const scope of getValidScopes()) {
        const result = validateScopes([scope]);
        expect(result.valid).toBe(true);
        expect(result.invalid).toEqual([]);
      }
    });

    it("should accept duplicates of valid scopes (deduplication is caller policy)", () => {
      const result = validateScopes(["read:users", "read:users", "read:users"]);
      expect(result.valid).toBe(true);
      expect(result.invalid).toEqual([]);
    });

    it("should mark an empty scope list as valid", () => {
      const result = validateScopes([]);
      expect(result.valid).toBe(true);
      expect(result.invalid).toEqual([]);
    });
  });

  describe("rejection paths", () => {
    it("should report only the invalid scopes in a mixed list", () => {
      const result = validateScopes([
        "read:users",
        "bogus",
        "admin:*",
        "READ:USERS",
        "",
      ]);
      expect(result.valid).toBe(false);
      expect(result.invalid).toEqual(["bogus", "READ:USERS", ""]);
    });

    it("should mark an all-invalid list as valid: false", () => {
      const result = validateScopes(["bogus", "nope", "read:unknown"]);
      expect(result.valid).toBe(false);
      expect(result.invalid).toEqual(["bogus", "nope", "read:unknown"]);
    });

    it("should preserve duplicate invalid entries in the report", () => {
      const result = validateScopes(["bogus", "bogus", "read:users"]);
      expect(result.valid).toBe(false);
      expect(result.invalid).toEqual(["bogus", "bogus"]);
    });

    it("should preserve duplicates across mixed valid and invalid entries", () => {
      const result = validateScopes([
        "bogus",
        "read:users",
        "bogus",
        "write:bids",
        "bogus",
      ]);
      expect(result.valid).toBe(false);
      expect(result.invalid).toEqual(["bogus", "bogus", "bogus"]);
    });

    it("should flag a single invalid entry among thousands of valid ones", () => {
      const scopes = [...getValidScopes(), "sneaky:scope", ...getValidScopes()];
      const result = validateScopes(scopes);
      expect(result.valid).toBe(false);
      expect(result.invalid).toEqual(["sneaky:scope"]);
    });

    it("should report near-miss invalid scopes verbatim, never corrected", () => {
      const nearMisses = [
        " read:users",           // leading space
        "read:users ",           // trailing space
        "READ:USERS",            // case variation
        "read:user",             // singular near-miss
        "read:users\n",          // trailing newline
        "read:unknown",          // valid category, unknown resource
        "delete:users",          // invalid category
        "read:",                 // empty resource segment
        "read:*:extra",          // wildcard misuse
      ];
      const result = validateScopes(nearMisses);
      expect(result.valid).toBe(false);
      expect(result.invalid).toEqual(nearMisses);
    });

    it("should preserve input order of invalid entries regardless of position", () => {
      const result = validateScopes([
        "bad1",
        "read:users",
        "bad2",
        "admin:*",
        "bad3",
      ]);
      expect(result.invalid).toEqual(["bad1", "bad2", "bad3"]);
    });

    it("should treat boundary strings as invalid entries", () => {
      const result = validateScopes(["", " ", "\t", "\n"]);
      expect(result.valid).toBe(false);
      expect(result.invalid).toEqual(["", " ", "\t", "\n"]);
    });
  });

  describe("non-array input (hard failure boundary)", () => {
    it("should throw a TypeError for null", () => {
      expect(() =>
        validateScopes(null as unknown as string[])
      ).toThrow(TypeError);
    });

    it("should throw a TypeError for undefined", () => {
      expect(() =>
        validateScopes(undefined as unknown as string[])
      ).toThrow(TypeError);
    });

    it("should throw a TypeError for a bare scope string", () => {
      expect(() =>
        validateScopes("read:users" as unknown as string[])
      ).toThrow(TypeError);
    });

    it("should throw a TypeError for numbers, booleans, and objects", () => {
      expect(() => validateScopes(42 as unknown as string[])).toThrow(TypeError);
      expect(() => validateScopes(true as unknown as string[])).toThrow(TypeError);
      expect(() =>
        validateScopes({ 0: "read:users", length: 1 } as unknown as string[])
      ).toThrow(TypeError);
    });

    it("should name the received type in the error message without echoing values", () => {
      for (const bad of [null, undefined, 42, true, "read:users", {}, () => {}]) {
        let message = "";
        try {
          validateScopes(bad as unknown as string[]);
        } catch (err) {
          message = (err as Error).message;
        }
        expect(message).toMatch(/validateScopes expects an array/i);
      }
      // Secrets/junk are never echoed verbatim into the message.
      try {
        validateScopes("qlx_live_supersecret" as unknown as string[]);
      } catch (err) {
        expect((err as Error).message).not.toContain("qlx_live_supersecret");
      }
    });
  });

  describe("determinism and purity", () => {
    it("should return identical results across repeated calls", () => {
      const inputs = [
        [],
        getValidScopes(),
        ["read:users", "bogus"],
        ["bogus", "bogus"],
        ["", " ", "READ:USERS"],
      ];
      for (const input of inputs) {
        const first = validateScopes(input);
        for (let i = 0; i < 10; i++) {
          const again = validateScopes(input);
          expect(again.valid).toBe(first.valid);
          expect(again.invalid).toEqual(first.invalid);
        }
      }
    });

    it("should produce results independent of call order", () => {
      const forward = [
        ["read:users"],
        ["bogus"],
        [],
        ["admin:*", "nope"],
        ["", "write:bids"],
      ].map(validateScopes);
      const backward = [
        ["", "write:bids"],
        ["admin:*", "nope"],
        [],
        ["bogus"],
        ["read:users"],
      ].map(validateScopes);
      expect(backward.reverse()).toEqual(forward);
    });

    it("should not mutate the input array", () => {
      const input = ["read:users", "bogus", "admin:*", ""];
      const snapshot = [...input];
      validateScopes(input);
      expect(input).toEqual(snapshot);
      expect(input).toHaveLength(4);
    });

    it("should not mutate the scope registry", () => {
      const before = getValidScopes();
      validateScopes(["read:users", "bogus"]);
      validateScopes([]);
      validateScopes(getValidScopes());
      expect(getValidScopes()).toEqual(before);
      expect(SCOPE_REGISTRY).toHaveLength(before.length);
    });

    it("should freeze the returned report against mutation", () => {
      const result = validateScopes(["bogus", "read:users"]);
      expect(Object.isFrozen(result)).toBe(true);
      expect(Object.isFrozen(result.invalid)).toBe(true);
      expect(() => {
        (result as { valid: boolean }).valid = true;
      }).toThrow();
      expect(() => {
        (result.invalid as unknown as string[]).push("read:users");
      }).toThrow();
      // Frozen report still reports the truth.
      expect(result.valid).toBe(false);
      expect(result.invalid).toEqual(["bogus"]);
    });

    it("should allow concurrent interleaved calls without cross-talk", () => {
      const validSet = getValidScopes();
      const invalidSet = ["bogus", "nope", ""];
      const iterations = 200;
      const outcomes: boolean[] = [];
      for (let i = 0; i < iterations; i++) {
        outcomes.push(
          validateScopes(validSet).valid,
          validateScopes(invalidSet).valid
        );
      }
      // Every even outcome is the valid set; every odd one the invalid set.
      outcomes.forEach((valid, i) => {
        expect(valid).toBe(i % 2 === 0);
      });
    });

    it("should yield identical reports for structurally equal inputs created independently", () => {
      const a = validateScopes(["read:users", "bogus", ""]);
      const b = validateScopes(["read:users", "bogus", ""]);
      expect(a.valid).toBe(b.valid);
      expect(a.invalid).toEqual(b.invalid);
      expect(a).not.toBe(b); // independent objects, equal content
    });
  });

  describe("compatibility with production caller (api-key-service)", () => {
    it("should keep the report shape consumed by ApiKeyService.createApiKey", () => {
      // api-key-service.ts relies on: scopeValidation.valid as a boolean
      // gate and scopeValidation.invalid.join(', ') in the error message.
      const good = validateScopes(["read:users", "write:invoices"]);
      expect(typeof good.valid).toBe("boolean");
      expect(good.valid).toBe(true);
      expect(Array.isArray(good.invalid)).toBe(true);
      expect(good.invalid.join(", ")).toBe("");

      const bad = validateScopes(["read:users", "bogus"]);
      expect(typeof bad.valid).toBe("boolean");
      expect(bad.valid).toBe(false);
      expect(bad.invalid.join(", ")).toBe("bogus");
    });
  });
});

// ── roleFromScopes — deterministic failure-boundary coverage (issue #2642) ───

describe("roleFromScopes", () => {
  // ── Valid privilege levels ──────────────────────────────────────────────────

  describe("valid privilege levels", () => {
    it("returns super_admin for ['admin:*']", () => {
      expect(roleFromScopes(["admin:*"])).toBe("super_admin");
    });

    it("returns operations_admin for ['write:*']", () => {
      expect(roleFromScopes(["write:*"])).toBe("operations_admin");
    });

    it("returns operations_admin for ['admin:keys']", () => {
      expect(roleFromScopes(["admin:keys"])).toBe("operations_admin");
    });

    it("returns support for ['read:*']", () => {
      expect(roleFromScopes(["read:*"])).toBe("support");
    });

    it("returns null for empty array", () => {
      expect(roleFromScopes([])).toBeNull();
    });
  });

  // ── Priority ordering ───────────────────────────────────────────────────────

  describe("priority ordering", () => {
    it("admin:* wins over write:* when both present", () => {
      expect(roleFromScopes(["write:*", "admin:*"])).toBe("super_admin");
    });

    it("admin:* wins over admin:keys when both present", () => {
      expect(roleFromScopes(["admin:keys", "admin:*"])).toBe("super_admin");
    });

    it("admin:* wins over read:* when both present", () => {
      expect(roleFromScopes(["read:*", "admin:*"])).toBe("super_admin");
    });

    it("admin:* wins over all three lower-privilege scopes combined", () => {
      expect(roleFromScopes(["read:*", "write:*", "admin:keys", "admin:*"])).toBe("super_admin");
    });

    it("write:* wins over read:* when admin:* absent", () => {
      expect(roleFromScopes(["read:*", "write:*"])).toBe("operations_admin");
    });

    it("write:* + admin:keys together yield operations_admin (no escalation)", () => {
      expect(roleFromScopes(["write:*", "admin:keys"])).toBe("operations_admin");
    });
  });

  // ── Duplicate scopes ────────────────────────────────────────────────────────

  describe("duplicate scopes", () => {
    it("duplicate admin:* entries still return super_admin", () => {
      expect(roleFromScopes(["admin:*", "admin:*", "admin:*"])).toBe("super_admin");
    });

    it("duplicate write:* entries still return operations_admin", () => {
      expect(roleFromScopes(["write:*", "write:*"])).toBe("operations_admin");
    });

    it("result is stable when a scope appears 100 times", () => {
      expect(roleFromScopes(Array(100).fill("read:*"))).toBe("support");
    });
  });

  // ── Order independence ──────────────────────────────────────────────────────

  describe("order independence", () => {
    it("super_admin regardless of admin:* position", () => {
      expect(roleFromScopes(["admin:*", "read:*", "write:*"])).toBe("super_admin");
      expect(roleFromScopes(["read:*", "write:*", "admin:*"])).toBe("super_admin");
    });

    it("operations_admin regardless of write:* position", () => {
      expect(roleFromScopes(["read:*", "write:*"])).toBe("operations_admin");
      expect(roleFromScopes(["write:*", "read:*"])).toBe("operations_admin");
    });
  });

  // ── Null / no-role boundary ─────────────────────────────────────────────────

  describe("null / no-role boundary", () => {
    it("returns null for granular read scopes without read:*", () => {
      expect(roleFromScopes(["read:users", "read:invoices"])).toBeNull();
    });

    it("returns null for granular write scopes without write:*", () => {
      expect(roleFromScopes(["write:users", "write:invoices"])).toBeNull();
    });

    it("returns null for service scopes only", () => {
      expect(roleFromScopes(["service:ingest", "service:export"])).toBeNull();
    });

    it("returns null for all non-wildcard registry scopes", () => {
      const roleScopes = new Set(["admin:*", "write:*", "admin:keys", "read:*"]);
      const nonRole = SCOPE_REGISTRY.map((s) => s.scope).filter((s) => !roleScopes.has(s));
      expect(roleFromScopes(nonRole)).toBeNull();
    });
  });

  // ── Adversarial / near-match inputs ────────────────────────────────────────

  describe("adversarial near-match inputs", () => {
    it("returns null for 'ADMIN:*' (wrong case)", () => {
      expect(roleFromScopes(["ADMIN:*"])).toBeNull();
    });

    it("returns null for 'admin: *' (space before asterisk)", () => {
      expect(roleFromScopes(["admin: *"])).toBeNull();
    });

    it("returns null for ' write:*' (leading space)", () => {
      expect(roleFromScopes([" write:*"])).toBeNull();
    });

    it("returns null for 'write:* ' (trailing space)", () => {
      expect(roleFromScopes(["write:* "])).toBeNull();
    });

    it("returns null for empty-string scope in array", () => {
      expect(roleFromScopes([""])).toBeNull();
    });

    it("returns null for completely unknown scope strings", () => {
      expect(roleFromScopes(["unknown:scope", "foo:bar"])).toBeNull();
    });

    it("does not treat 'admin:keys' as admin:* (no escalation)", () => {
      expect(roleFromScopes(["admin:keys"])).toBe("operations_admin");
      expect(roleFromScopes(["admin:keys"])).not.toBe("super_admin");
    });
  });

  // ── Type-safety boundary (non-array runtime values) ────────────────────────

  describe("type-safety boundary", () => {
    const cast = (v: unknown) => roleFromScopes(v as string[]);

    it("returns null for null without throwing", () => {
      expect(() => cast(null)).not.toThrow();
      expect(cast(null)).toBeNull();
    });

    it("returns null for undefined without throwing", () => {
      expect(() => cast(undefined)).not.toThrow();
      expect(cast(undefined)).toBeNull();
    });

    it("returns null for a plain string without throwing", () => {
      expect(() => cast("admin:*")).not.toThrow();
      expect(cast("admin:*")).toBeNull();
    });

    it("returns null for a number without throwing", () => {
      expect(() => cast(42)).not.toThrow();
      expect(cast(42)).toBeNull();
    });

    it("returns null for a plain object without throwing", () => {
      expect(() => cast({ scope: "admin:*" })).not.toThrow();
      expect(cast({ scope: "admin:*" })).toBeNull();
    });

    it("returns null for boolean without throwing", () => {
      expect(() => cast(true)).not.toThrow();
      expect(cast(true)).toBeNull();
    });
  });

  // ── Determinism sweep ───────────────────────────────────────────────────────

  describe("determinism", () => {
    const cases: Array<[string, string[], AdminRole | null]> = [
      ["super_admin", ["admin:*"], "super_admin"],
      ["operations_admin via write:*", ["write:*"], "operations_admin"],
      ["operations_admin via admin:keys", ["admin:keys"], "operations_admin"],
      ["support", ["read:*"], "support"],
      ["null — empty", [], null],
      ["null — granular only", ["read:users", "write:bids"], null],
      ["null — unknown scope", ["unknown:x"], null],
    ];

    for (const [label, input, expected] of cases) {
      it(`stable across 5 calls: ${label}`, () => {
        for (let i = 0; i < 5; i++) {
          expect(roleFromScopes(input)).toBe(expected);
        }
      });
    }

    it("no cross-contamination between sequential calls", () => {
      expect(roleFromScopes(["admin:*"])).toBe("super_admin");
      expect(roleFromScopes([])).toBeNull();
      expect(roleFromScopes(["read:*"])).toBe("support");
      expect(roleFromScopes(["admin:*"])).toBe("super_admin");
    });
  });

  // ── Concurrency ─────────────────────────────────────────────────────────────

  describe("concurrency safety", () => {
    it("parallel calls with different inputs never cross-contaminate", async () => {
      const tasks: Array<{ input: string[]; expected: AdminRole | null }> = [
        { input: ["admin:*"], expected: "super_admin" },
        { input: ["write:*"], expected: "operations_admin" },
        { input: ["read:*"], expected: "support" },
        { input: [], expected: null },
        { input: ["admin:keys"], expected: "operations_admin" },
        { input: ["read:users"], expected: null },
      ];
      const results = await Promise.all(
        tasks.map(({ input }) => Promise.resolve().then(() => roleFromScopes(input)))
      );
      results.forEach((result, i) => {
        expect(result).toBe(tasks[i].expected);
      });
    });
  });

  // ── Security invariants ─────────────────────────────────────────────────────

  describe("security invariants", () => {
    it("every returned role is a member of ADMIN_ROLES or null", () => {
      const allInputs: string[][] = [
        ["admin:*"], ["write:*"], ["admin:keys"], ["read:*"],
        ["read:*", "write:*"], [], ["read:users"], ["service:ingest"],
      ];
      for (const input of allInputs) {
        const role = roleFromScopes(input);
        if (role !== null) expect(ADMIN_ROLES).toContain(role);
      }
    });

    it("no scope combination produces security_admin (intentionally unmapped)", () => {
      const combos: string[][] = [
        SCOPE_REGISTRY.map((s) => s.scope),
        ["admin:*"],
        ["write:*", "admin:keys", "read:*"],
        ["admin:*", "write:*", "admin:keys", "read:*"],
      ];
      for (const combo of combos) {
        expect(roleFromScopes(combo)).not.toBe("security_admin");
      }
    });

    it("admin:* is the only scope that yields super_admin", () => {
      const nonAdminWildcard = SCOPE_REGISTRY.map((s) => s.scope).filter((s) => s !== "admin:*");
      for (const scope of nonAdminWildcard) {
        expect(roleFromScopes([scope])).not.toBe("super_admin");
      }
    });
  });
});
