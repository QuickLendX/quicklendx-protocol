/**
 * Deterministic failure-boundary coverage for `isPrivate` in
 * `backend/src/lib/logging/policy.ts`.
 *
 * `isPrivate(name)` answers one question: "must this field be masked before it
 * reaches a log sink?". It is the deny-by-default half of the policy, so its
 * failure mode is a *silent leak*: any input that is not classified PRIVATE
 * while also not being classified PUBLIC or SECRET means a caller gating on the
 * predicate has no instruction for that field. The invariant pinned throughout
 * this file is therefore:
 *
 *   I1  For every input, exactly one of `isPublic` / `isPrivate` / `isSecret`
 *       is true.
 *   I2  A name that is not an own key of the policy map is always PRIVATE.
 *   I3  The answer never depends on the clock, the scheduler, the filesystem,
 *       or call order.
 *
 * The interesting boundary is I2/I3 on the policy-load *failure* path. The
 * module initialises once at import; if the policy file cannot be read or fails
 * schema validation it falls back to an empty deny-by-default policy. That
 * fallback used to install a map with a normal prototype, so names that collide
 * with `Object.prototype` members resolved through the prototype chain and
 * `isPrivate("constructor")` answered **false** — I1 and I2 both broken on
 * exactly the most sensitive path. `loadPolicy` had already been hardened for
 * this (a null prototype); the fallback had not.
 *
 * A sibling suite, `backend/src/tests/logging-policy.test.ts`, covers the rest
 * of the module. It currently has pre-existing failures on `main` unrelated to
 * `isPrivate` (its `getPolicyFields` cases target a superseded array-returning
 * signature, and its `redactByTier`/`redactObject` hash expectations predate
 * the `hashValue` canonicalisation in #2758). Those are left alone here, so
 * this file stands on its own and its cases report independently.
 */

import {
  classifyField,
  FieldTier,
  getPolicyFields,
  getPolicyLoadError,
  isPrivate,
  isPublic,
  isSecret,
  redactObject,
} from "./policy";

type PolicyModule = typeof import("./policy");

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/**
 * Load a fresh copy of the policy module in an isolated registry with
 * `readFileSync` stubbed, so a given policy file (valid, invalid, or
 * unreadable) can be exercised without touching the real one.
 *
 * `initialisePolicy()` runs once per module instance, so each call to this
 * helper is one independent, deterministic initialisation — that is what makes
 * the load-failure and recovery cases repeatable.
 */
const loadPolicyModule = (readFileSync: (...args: any[]) => string): PolicyModule => {
  let mod: PolicyModule | undefined;
  jest.isolateModules(() => {
    jest.doMock("fs", () => ({
      ...jest.requireActual("fs"),
      readFileSync,
    }));
    mod = require("./policy") as PolicyModule;
  });
  jest.dontMock("fs");
  if (!mod) throw new Error("policy module was not loaded");
  return mod;
};

const unreadable = (message = "EACCES: permission denied") => (): string => {
  throw new Error(`${message}, open '.../redaction-policy.json'`);
};

const serving = (json: unknown) => (): string => JSON.stringify(json);

/** `Object.prototype` members a request body or log record can legitimately carry. */
const PROTOTYPE_MEMBERS = [
  "constructor",
  "toString",
  "toLocaleString",
  "valueOf",
  "hasOwnProperty",
  "isPrototypeOf",
  "propertyIsEnumerable",
  "__proto__",
];

/** Non-string inputs a caller can hand the predicate. */
const NON_STRING_INPUTS: Array<[string, unknown]> = [
  ["undefined", undefined],
  ["null", null],
  ["NaN", Number.NaN],
  ["zero", 0],
  ["a number", 42],
  ["a boolean", true],
  ["an empty array", []],
  ["a plain object", {}],
  ["an object with toString", { toString: () => "invoice_1" }],
  ["a symbol", Symbol("id")],
  ["a bigint", BigInt(1)],
  ["a function", function named() {}],
];

/** The one-of-three invariant (I1) plus the tier, for any input. */
const triState = (mod: PolicyModule, name: unknown): boolean[] => [
  mod.isPublic(name as string),
  mod.isPrivate(name as string),
  mod.isSecret(name as string),
];

describe("isPrivate", () => {
  afterEach(() => {
    jest.dontMock("fs");
    jest.resetModules();
  });

  // -------------------------------------------------------------------------
  // I1/I2 — the loaded policy
  // -------------------------------------------------------------------------
  describe("registry with the real policy loaded", () => {
    it("reports a clean load, so a later fallback cannot be mistaken for one", () => {
      expect(getPolicyLoadError()).toBeNull();
      expect(getPolicyFields(FieldTier.PUBLIC)).toContain("id");
    });

    it("classifies a field registered PRIVATE", () => {
      expect(isPrivate("amount")).toBe(true);
      expect(classifyField("amount")).toBe(FieldTier.PRIVATE);
    });

    it("does not classify a PUBLIC or SECRET field as private", () => {
      expect(isPrivate("id")).toBe(false);
      expect(isPrivate("token")).toBe(false);
    });

    it.each([
      ["PUBLIC", "id", FieldTier.PUBLIC],
      ["PRIVATE", "reason", FieldTier.PRIVATE],
      ["SECRET", "private_key", FieldTier.SECRET],
    ])("classifies a %s field", (_tier, name, expected) => {
      expect(classifyField(name)).toBe(expected);
    });

    it("defaults an unlisted field to PRIVATE", () => {
      expect(classifyField("some_field_absent_from_the_policy")).toBe(FieldTier.PRIVATE);
      expect(isPrivate("some_field_absent_from_the_policy")).toBe(true);
    });

    it("is case-sensitive: 'ID' is not 'id'", () => {
      expect(isPublic("id")).toBe(true);
      expect(isPublic("ID")).toBe(false);
      // The unknown spelling falls back to PRIVATE rather than PUBLIC.
      expect(isPrivate("ID")).toBe(true);
    });

    it("classifies the empty string as PRIVATE (unknown default)", () => {
      expect(isPrivate("")).toBe(true);
    });

    it("holds the one-of-three invariant across every registered field", () => {
      const everyField = [
        ...getPolicyFields(FieldTier.PUBLIC),
        ...getPolicyFields(FieldTier.PRIVATE),
        ...getPolicyFields(FieldTier.SECRET),
      ];
      expect(everyField.length).toBeGreaterThan(0);

      for (const name of everyField) {
        const flags = triState({ isPublic, isPrivate, isSecret } as PolicyModule, name);
        expect(flags.filter(Boolean)).toHaveLength(1);
        expect(isPrivate(name)).toBe(getPolicyFields(FieldTier.PRIVATE).includes(name));
      }
    });

    it.each(PROTOTYPE_MEMBERS)(
      "classifies the prototype member %s as PRIVATE, not as a prototype value",
      (name) => {
        // A null-prototype map is what keeps these from resolving through
        // Object.prototype; without it `isPrivate` would answer false.
        expect(isPrivate(name)).toBe(true);
        expect(classifyField(name)).toBe(FieldTier.PRIVATE);
        expect(triState({ isPublic, isPrivate, isSecret } as PolicyModule, name)).toEqual([
          false,
          true,
          false,
        ]);
      },
    );

    it.each(NON_STRING_INPUTS)("never throws on %s and defaults it to PRIVATE", (_label, input) => {
      expect(() => isPrivate(input as string)).not.toThrow();
      expect(isPrivate(input as string)).toBe(true);
      expect(triState({ isPublic, isPrivate, isSecret } as PolicyModule, input)).toEqual([
        false,
        true,
        false,
      ]);
    });

    it("does not coerce a non-string input into a registered field name", () => {
      // Property access would turn `undefined` into the key "undefined". If the
      // policy ever registered that key as PUBLIC, a caller passing the wrong
      // type would get PUBLIC back and log the value verbatim.
      const mod = loadPolicyModule(serving({ public: ["undefined"], private: [], secret: [] }));

      expect(mod.isPublic("undefined")).toBe(true);
      expect(mod.isPrivate(undefined as unknown as string)).toBe(true);
      expect(mod.classifyField(undefined as unknown as string)).toBe(FieldTier.PRIVATE);
    });

    it("never classifies a non-string input as PUBLIC or SECRET, whatever is registered", () => {
      // Each input below is coerced to a *registered* key by property access:
      // `undefined`/`null`/`42` become "undefined"/"null"/"42", and the object
      // below becomes "spoofed" through its own `toString`. Without a type
      // guard the lookup would succeed and hand back PUBLIC.
      const mod = loadPolicyModule(
        serving({
          public: ["undefined", "null", "42", "spoofed", "[object Object]"],
          private: [],
          secret: [],
        }),
      );

      const coercingInputs: unknown[] = [
        undefined,
        null,
        42,
        { toString: () => "spoofed" },
      ];
      for (const input of coercingInputs) {
        expect(mod.isPublic(input as string)).toBe(false);
        expect(mod.isSecret(input as string)).toBe(false);
        expect(mod.isPrivate(input as string)).toBe(true);
      }
    });

    it("returns the same answer for a Symbol each time, without accumulating state", () => {
      const name = Symbol("field");

      expect(isPrivate(name as unknown as string)).toBe(true);
      expect(isPrivate(name as unknown as string)).toBe(true);
      expect(isPrivate(name as unknown as string)).toBe(true);
    });
  });

  // -------------------------------------------------------------------------
  // I2 — the load-failure fallback (the regression this suite exists for)
  // -------------------------------------------------------------------------
  describe("policy fails to load", () => {
    it("records the failure and falls back to an empty deny-by-default policy", () => {
      const mod = loadPolicyModule(unreadable());

      expect(mod.getPolicyLoadError()).toBeInstanceOf(Error);
      expect(mod.getPolicyLoadError()?.message).toContain("EACCES");
      expect(mod.getPolicyFields(FieldTier.PUBLIC)).toEqual([]);
      expect(mod.getPolicyFields(FieldTier.PRIVATE)).toEqual([]);
      expect(mod.getPolicyFields(FieldTier.SECRET)).toEqual([]);
    });

    it("reads the policy from the packaged redaction-policy.json", () => {
      const paths: string[] = [];
      const empty = serving({ public: [], private: [], secret: [] });
      loadPolicyModule((path: string) => {
        paths.push(path);
        return empty();
      });

      expect(paths).toHaveLength(1);
      expect(paths[0]).toContain("redaction-policy.json");
    });

    it.each(PROTOTYPE_MEMBERS)(
      "still classifies the prototype member %s as PRIVATE",
      (name) => {
        // Before the fix the fallback installed a map with a normal prototype,
        // so all three predicates answered false for these names: the caller
        // had no instruction at all for a field that has to be masked.
        const mod = loadPolicyModule(unreadable());

        expect(mod.isPrivate(name)).toBe(true);
        expect(mod.isPublic(name)).toBe(false);
        expect(mod.isSecret(name)).toBe(false);
      },
    );

    it("holds the one-of-three invariant for every name once the policy is gone", () => {
      const mod = loadPolicyModule(unreadable());
      const names = [
        "id",
        "amount",
        "token",
        "email",
        ...PROTOTYPE_MEMBERS,
        "",
        "definitely_not_in_any_policy",
      ];

      for (const name of names) {
        const flags = triState(mod, name);
        expect({ name, flags }).toEqual({ name, flags: [false, true, false] });
      }
    });

    it("classifies every field that the real policy knows as PRIVATE", () => {
      const real = [
        ...getPolicyFields(FieldTier.PUBLIC),
        ...getPolicyFields(FieldTier.SECRET),
      ];
      const mod = loadPolicyModule(unreadable());

      // With no policy there is nothing to log verbatim and nothing to redact
      // outright, so masking every field is the only safe answer.
      for (const name of real) {
        expect(mod.isPrivate(name)).toBe(true);
      }
    });

    it("keeps values out of the log even though nothing is classified SECRET", () => {
      const mod = loadPolicyModule(unreadable());

      const out = mod.redactObject({
        token: "sk-live-should-never-appear",
        email: "payer@example.test",
        constructor: "value-for-constructor",
        id: "inv_1",
      });

      const serialised = JSON.stringify(out);
      expect(serialised).not.toContain("sk-live-should-never-appear");
      expect(serialised).not.toContain("payer@example.test");
      expect(serialised).not.toContain("value-for-constructor");
      for (const value of Object.values(out)) {
        expect(value).toMatch(/^sha256:[0-9a-f]{8}$/);
      }
    });

    it("rejects invalid JSON the same way as an unreadable file", () => {
      const mod = loadPolicyModule(() => "{ this is not json");

      expect(mod.getPolicyLoadError()).toBeInstanceOf(Error);
      expect(mod.isPrivate("id")).toBe(true);
      expect(mod.isPrivate("constructor")).toBe(true);
      expect(mod.getPolicyFields(FieldTier.PUBLIC)).toEqual([]);
    });

    it("rejects a schema-invalid policy the same way as an unreadable file", () => {
      const mod = loadPolicyModule(serving({ public: "not-an-array" }));

      expect(mod.getPolicyLoadError()).toBeInstanceOf(Error);
      expect(mod.isPrivate("id")).toBe(true);
      expect(mod.isPrivate("constructor")).toBe(true);
      expect(mod.getPolicyFields(FieldTier.PUBLIC)).toEqual([]);
    });

    it("rejects a policy that is missing a tier entirely", () => {
      const mod = loadPolicyModule(serving({ public: ["id"] }));

      expect(mod.getPolicyLoadError()).toBeInstanceOf(Error);
      expect(mod.getPolicyFields(FieldTier.PUBLIC)).toEqual([]);
    });

    it.each([
      ["an empty object", {}],
      ["null", null],
      ["a JSON array", []],
      ["a JSON scalar", 42],
    ])("rejects %s without throwing at import time", (_label, payload) => {
      let mod: PolicyModule | undefined;
      expect(() => {
        mod = loadPolicyModule(serving(payload));
      }).not.toThrow();
      expect(mod!.getPolicyLoadError()).toBeInstanceOf(Error);
      expect(mod!.isPrivate("id")).toBe(true);
    });

    it("recovers on the next load once the file is readable again", () => {
      // Retry semantics: the module initialises once per instance, so a retry is
      // a fresh instance. The recovered instance must not inherit the poisoned
      // fallback map from the failed one.
      const failed = loadPolicyModule(unreadable());
      expect(failed.getPolicyLoadError()).toBeInstanceOf(Error);
      expect(failed.isPublic("id")).toBe(false);

      const recovered = loadPolicyModule(
        serving({ public: ["id"], private: ["amount"], secret: ["token"] }),
      );

      expect(recovered.getPolicyLoadError()).toBeNull();
      expect(recovered.isPublic("id")).toBe(true);
      expect(recovered.isPrivate("amount")).toBe(true);
      expect(recovered.isSecret("token")).toBe(true);
      expect(recovered.isPrivate("constructor")).toBe(true);
    });

    it("keeps the fallback verdict stable across repeated loads", () => {
      const verdicts = PROTOTYPE_MEMBERS.map((name) => {
        const mod = loadPolicyModule(unreadable());
        return mod.isPrivate(name);
      });

      expect(verdicts).toEqual(PROTOTYPE_MEMBERS.map(() => true));
    });
  });

  // -------------------------------------------------------------------------
  // Duplicate and adversarial policy content
  // -------------------------------------------------------------------------
  describe("duplicate and adversarial policy content", () => {
    it("resolves a field listed in two tiers by a fixed, documented order", () => {
      // Tiers are applied public → private → secret, so the most restrictive
      // registration wins. The policy shipped in the repo has no such overlap;
      // this pins the answer so a reordering cannot silently change it.
      const mod = loadPolicyModule(
        serving({
          public: ["shared"],
          private: ["shared"],
          secret: ["shared"],
        }),
      );

      expect(mod.classifyField("shared")).toBe(FieldTier.SECRET);
      expect(mod.isSecret("shared")).toBe(true);
      expect(mod.isPrivate("shared")).toBe(false);
    });

    it("lets a later tier override an earlier one independently per field", () => {
      const mod = loadPolicyModule(
        serving({
          public: ["a", "b"],
          private: ["b"],
          secret: ["c", "b"],
        }),
      );

      expect(mod.classifyField("a")).toBe(FieldTier.PUBLIC);
      expect(mod.classifyField("b")).toBe(FieldTier.SECRET);
      expect(mod.classifyField("c")).toBe(FieldTier.SECRET);
    });

    it("preserves duplicate entries in the tier listing without deduplicating", () => {
      const mod = loadPolicyModule(serving({ public: ["id", "id", "id"], private: [], secret: [] }));

      expect(mod.getPolicyFields(FieldTier.PUBLIC)).toEqual(["id", "id", "id"]);
    });

    it("honours an explicitly registered prototype-named field as its own tier", () => {
      // The own-property guard rejects *prototype* lookups, not a real entry:
      // if the policy deliberately registers "constructor" as PUBLIC that is a
      // policy decision and must win over the deny-by-default rule.
      const mod = loadPolicyModule(
        serving({ public: ["constructor"], private: ["toString"], secret: [] }),
      );

      expect(mod.isPublic("constructor")).toBe(true);
      expect(mod.isPrivate("constructor")).toBe(false);
      expect(mod.isPrivate("toString")).toBe(true);
      expect(mod.isPublic("toString")).toBe(false);
    });

    it("honours a registered `__proto__` entry without poisoning the map", () => {
      const mod = loadPolicyModule(
        serving({ public: [], private: ["__proto__"], secret: [] }),
      );

      expect(mod.isPrivate("__proto__")).toBe(true);
      // A `__proto__` key must not have mutated the prototype of anything the
      // policy map hands out.
      expect(mod.getPolicyFields(FieldTier.PRIVATE)).toEqual(["__proto__"]);
      expect(mod.classifyField("anything_else")).toBe(FieldTier.PRIVATE);
    });

    it("accepts an empty but schema-valid policy as deny-by-default, not a failure", () => {
      const mod = loadPolicyModule(serving({ public: [], private: [], secret: [] }));

      expect(mod.getPolicyLoadError()).toBeNull();
      expect(mod.isPrivate("id")).toBe(true);
      expect(mod.isPrivate("constructor")).toBe(true);
    });
  });

  // -------------------------------------------------------------------------
  // I3 — concurrency, retries, and shared state
  // -------------------------------------------------------------------------
  describe("determinism under concurrency and retries", () => {
    it("gives the same answer for concurrent and sequential calls", async () => {
      const names = ["id", "amount", "token", "constructor", "toString", "unlisted"];
      const sequential = names.map((name) => isPrivate(name));

      const concurrent = await Promise.all(
        Array.from({ length: 200 }, (_unused, index) =>
          Promise.resolve().then(() => isPrivate(names[index % names.length])),
        ),
      );

      for (let index = 0; index < names.length; index += 1) {
        expect(concurrent[index]).toBe(sequential[index]);
      }
    });

    it("is not affected by interleaving reads of the other policy accessors", async () => {
      const before = isPrivate("constructor");

      await Promise.all([
        Promise.resolve().then(() => getPolicyFields(FieldTier.PUBLIC).push("injected")),
        Promise.resolve().then(() => getPolicyFields(FieldTier.SECRET).splice(0, 3)),
        Promise.resolve().then(() => void getPolicyLoadError()),
      ]);

      expect(isPrivate("constructor")).toBe(before);
      expect(isPrivate("injected")).toBe(true);
      expect(isPublic("id")).toBe(true);
      expect(isSecret("token")).toBe(true);
    });

    it("does not let a mutated tier listing change any later verdict", () => {
      const fields = getPolicyFields(FieldTier.PRIVATE);
      const amountVerdict = isPrivate("amount");

      fields.push("token");
      fields.length = 0;

      expect(isPrivate("amount")).toBe(amountVerdict);
      expect(isSecret("token")).toBe(true);
      expect(getPolicyFields(FieldTier.PRIVATE)).not.toBe(fields);
    });

    it("returns a stable verdict when a failed load is retried repeatedly", () => {
      const attempts = [0, 1, 2].map(() => {
        const mod = loadPolicyModule(unreadable("EIO: i/o error"));
        return {
          error: mod.getPolicyLoadError()?.message,
          privateAmount: mod.isPrivate("amount"),
          privateConstructor: mod.isPrivate("constructor"),
        };
      });

      expect(new Set(attempts.map((a) => a.privateAmount))).toEqual(new Set([true]));
      expect(new Set(attempts.map((a) => a.privateConstructor))).toEqual(new Set([true]));
      for (const attempt of attempts) {
        expect(attempt.error).toContain("EIO");
      }
    });

    it("accepts the string spelling of a tier, because the tiers are strings", () => {
      // `FieldTier.PRIVATE === "private"`, so a caller that passes the raw
      // string gets the same deterministic answer as one that passes the const.
      for (const tier of [FieldTier.PUBLIC, FieldTier.PRIVATE, FieldTier.SECRET]) {
        expect(getPolicyFields(tier)).toEqual(getPolicyFields(tier as unknown as FieldTier));
      }
    });

    it("returns an empty list, without throwing, for any other tier value", () => {
      const tiers = [undefined, null, "PUBLIC", "Private", 0, 1, [], {}, " public"];

      for (const tier of tiers) {
        expect(() => getPolicyFields(tier as FieldTier)).not.toThrow();
        expect(getPolicyFields(tier as FieldTier)).toEqual([]);
      }
    });
  });
});
