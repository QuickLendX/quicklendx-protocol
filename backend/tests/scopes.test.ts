import {
  isValidScope,
  getValidScopes,
  validateScopes,
  SCOPE_REGISTRY,
} from "../src/config/scopes";

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

describe("validateScopes (isValidScope consumer)", () => {
  it("should mark an all-scope set as valid", () => {
    const result = validateScopes(getValidScopes());
    expect(result.valid).toBe(true);
    expect(result.invalid).toEqual([]);
  });

  it("should mark an empty scope list as valid", () => {
    const result = validateScopes([]);
    expect(result.valid).toBe(true);
    expect(result.invalid).toEqual([]);
  });

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
});
