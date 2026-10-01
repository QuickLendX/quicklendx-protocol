/**
 * Deterministic failure-boundary coverage for the migration type contracts in
 * `src/lib/migrations/types.ts`.
 *
 * Strategy:
 * - The interfaces in types.ts are erased at build time, so these tests drive
 *   the runtime validators that stand in for the type system on values loaded
 *   from disk with `require()`.
 * - Every case asserts on the report (valid flag, error codes, field paths,
 *   message ordering) rather than on thrown errors: validation is total and must
 *   degrade to a report for malformed input instead of crashing a migration run.
 * - Boundaries are explicit: version range and integrality, name length and
 *   snake_case shape, real-calendar ISO dates including leap years, and
 *   hotfix metadata required by the hotfix emergency protocol.
 * - Secret hygiene is asserted directly: a sentinel planted in a malformed
 *   field must never appear in rendered diagnostics.
 * - Property-based cases assert that valid definitions are always accepted and
 *   that arbitrary junk never throws.
 */

import * as fs from "fs";
import * as path from "path";
import * as fc from "fast-check";
import {
  HotfixFlags,
  MigrationErrorCodes,
  MIGRATION_NAME_PATTERN,
  isHotfixFlag,
  isMigrationDefinition,
  validateMigrationDefinition,
  validateMigrationDefinitions,
} from "../lib/migrations/types";
import type { MigrationDefinition } from "../lib/migrations/types";

const CODE = MigrationErrorCodes.MIGRATION_VALIDATION_FAILED;

function validDefinition(overrides: Partial<MigrationDefinition> = {}): Record<string, unknown> {
  return {
    version: 1,
    name: "add_invoice_index",
    authoredAt: "2026-04-26",
    author: "QuickLendX Engineering",
    up: async () => undefined,
    ...overrides,
  };
}

function fieldsOf(result: { issues: { field: string }[] }): string[] {
  return result.issues.map((entry) => entry.field);
}

function issueFor(result: { issues: { field: string }[] }, field: string): boolean {
  return result.issues.some((entry) => entry.field === field);
}

describe("migration definition constants", () => {
  test("error codes remain stable for downstream consumers", () => {
    expect(MigrationErrorCodes).toEqual({
      MIGRATION_ALREADY_APPLIED: "MIGRATION_ALREADY_APPLIED",
      MIGRATION_MISSING: "MIGRATION_MISSING",
      DOWN_MIGRATION_NOT_ALLOWED: "DOWN_MIGRATION_NOT_ALLOWED",
      MIGRATION_VALIDATION_FAILED: "MIGRATION_VALIDATION_FAILED",
      MIGRATION_EXECUTION_FAILED: "MIGRATION_EXECUTION_FAILED",
      CHECKSUM_MISMATCH: "CHECKSUM_MISMATCH",
      HOTFIX_REQUIRES_APPROVAL: "HOTFIX_REQUIRES_APPROVAL",
      UNSUPPORTED_IN_PRODUCTION: "UNSUPPORTED_IN_PRODUCTION",
    });
    expect(Object.values(HotfixFlags)).toEqual(["critical", "urgent", "standard"]);
  });

  test("name pattern accepts snake_case and rejects surrounding whitespace", () => {
    expect(MIGRATION_NAME_PATTERN.test("add_invoice_index")).toBe(true);
    expect(MIGRATION_NAME_PATTERN.test("v2_fix")).toBe(true);
    expect(MIGRATION_NAME_PATTERN.test("add_invoice_index ")).toBe(false);
    expect(MIGRATION_NAME_PATTERN.test("_leading")).toBe(false);
  });
});

describe("valid definitions", () => {
  test("accepts a minimal forward-only definition", () => {
    const result = validateMigrationDefinition(validDefinition());
    expect(result).toEqual({ valid: true, errors: [], issues: [] });
  });

  test("accepts a full definition with down, validate, and meta", () => {
    const result = validateMigrationDefinition(
      validDefinition({
        down: async () => undefined,
        validate: async () => [],
        meta: { links: ["https://github.com/QuickLendX/quicklendx-protocol/issues/1"] },
      }),
    );
    expect(result.valid).toBe(true);
  });

  test("accepts an object with a null prototype", () => {
    const definition = Object.assign(Object.create(null), validDefinition());
    expect(validateMigrationDefinition(definition).valid).toBe(true);
  });

  test("accepts a hotfix definition that satisfies the emergency protocol", () => {
    const result = validateMigrationDefinition(
      validDefinition({
        name: "hotfix_add_invoice_id",
        meta: {
          hotfix: true,
          hotfix_flag: HotfixFlags.CRITICAL,
          reason: "Compliance gap: audit trail lacks per-invoice granularity",
          rollback_risk: "medium",
          required_approvals: 2,
        },
        down: async () => undefined,
      }),
    );
    expect(result).toEqual({ valid: true, errors: [], issues: [] });
  });

  test("accepts the date-only and timestamp authoredAt forms used by real migrations", () => {
    expect(validateMigrationDefinition(validDefinition({ authoredAt: "2026-04-26" })).valid).toBe(true);
    expect(validateMigrationDefinition(validDefinition({ authoredAt: "2026-04-26T14:30:00Z" })).valid).toBe(true);
    const offsetForm = validateMigrationDefinition(validDefinition({ authoredAt: "2026-04-26T14:30:00+02:00" }));
    expect(offsetForm.valid).toBe(true);
  });

  test("does not mutate the definition it validates", () => {
    const definition = validDefinition({ meta: { hotfix: true } });
    const snapshot = JSON.stringify(definition);
    validateMigrationDefinition(definition);
    expect(JSON.stringify(definition)).toBe(snapshot);
  });
});

describe("malformed containers", () => {
  const nonObjects: [string, unknown][] = [
    ["null", null],
    ["undefined", undefined],
    ["string", "v001_add_index"],
    ["number", 1],
    ["boolean", true],
    ["array", [validDefinition()]],
    ["empty array", []],
    ["function", () => undefined],
    ["map", new Map()],
  ];

  test.each(nonObjects)("rejects a %s without throwing", (_label, value) => {
    const result = validateMigrationDefinition(value);
    expect(result.valid).toBe(false);
    expect(fieldsOf(result)).toEqual(["definition"]);
    expect(result.errors[0]).toContain(CODE);
  });

  test("rejects a class instance because its prototype is not Object.prototype", () => {
    class Migration {
      version = 1;
    }
    const result = validateMigrationDefinition(new Migration());
    expect(result.valid).toBe(false);
    expect(fieldsOf(result)).toEqual(["definition"]);
  });

  test("reports a rejected container without echoing its contents", () => {
    const result = validateMigrationDefinition({ password: "SENTINEL_SECRET" });
    expect(result.valid).toBe(false);
    expect(JSON.stringify(result)).not.toContain("SENTINEL_SECRET");
  });
});

describe("required fields", () => {
  test("rejects a definition with no fields at all", () => {
    const result = validateMigrationDefinition({});
    expect(result.valid).toBe(false);
    expect(fieldsOf(result)).toEqual(["version", "name", "authoredAt", "author", "up"]);
  });

  test("rejects non-string and blank identifiers", () => {
    expect(issueFor(validateMigrationDefinition(validDefinition({ name: 42 as never })), "name")).toBe(true);
    expect(issueFor(validateMigrationDefinition(validDefinition({ name: "" })), "name")).toBe(true);
    expect(issueFor(validateMigrationDefinition(validDefinition({ name: "   " })), "name")).toBe(true);
    expect(issueFor(validateMigrationDefinition(validDefinition({ author: "" })), "author")).toBe(true);
    expect(issueFor(validateMigrationDefinition(validDefinition({ author: " \t " })), "author")).toBe(true);
  });

  test("rejects an author past the identifier length limit", () => {
    const result = validateMigrationDefinition(validDefinition({ author: "a".repeat(129) }));
    expect(result.issues.find((entry) => entry.field === "author")?.message).toContain("at most 128 characters");
  });

  test("requires a callable up function", () => {
    const result = validateMigrationDefinition(validDefinition({ up: "async () => {}" as never }));
    expect(issueFor(result, "up")).toBe(true);
    expect(result.issues.find((entry) => entry.field === "up")?.message).toContain("received string");
  });

  test("accepts an omitted down and validate but rejects a non-callable one", () => {
    expect(validateMigrationDefinition(validDefinition({ down: undefined })).valid).toBe(true);
    expect(validateMigrationDefinition(validDefinition({ validate: undefined })).valid).toBe(true);

    const nullDown = validateMigrationDefinition(validDefinition({ down: null as never }));
    expect(issueFor(nullDown, "down")).toBe(true);
    expect(nullDown.issues.find((entry) => entry.field === "down")?.message).toContain("received null");

    expect(issueFor(validateMigrationDefinition(validDefinition({ validate: {} as never })), "validate")).toBe(true);
  });

  test("rejects a meta value that is not a plain object", () => {
    expect(issueFor(validateMigrationDefinition(validDefinition({ meta: [] as never })), "meta")).toBe(true);
    expect(issueFor(validateMigrationDefinition(validDefinition({ meta: "hotfix" as never })), "meta")).toBe(true);
    expect(issueFor(validateMigrationDefinition(validDefinition({ meta: null as never })), "meta")).toBe(true);
  });
});

describe("version boundaries", () => {
  const accepted: [string, number][] = [
    ["the first version", 1],
    ["a mid-range version", 906],
    ["Number.MAX_SAFE_INTEGER", Number.MAX_SAFE_INTEGER],
  ];

  test.each(accepted)("accepts %s", (_label, version) => {
    expect(validateMigrationDefinition(validDefinition({ version })).valid).toBe(true);
  });

  const rejected: [string, number][] = [
    ["zero", 0],
    ["a negative version", -1],
    ["an unsafe integer above the range", Number.MAX_SAFE_INTEGER + 2],
    ["a fractional version", 1.5],
    ["NaN", Number.NaN],
    ["Infinity", Number.POSITIVE_INFINITY],
    ["-Infinity", Number.NEGATIVE_INFINITY],
  ];

  test.each(rejected)("rejects %s", (_label, version) => {
    const result = validateMigrationDefinition(validDefinition({ version }));
    expect(issueFor(result, "version")).toBe(true);
  });

  test("reports a non-numeric version without rendering object contents", () => {
    const result = validateMigrationDefinition(validDefinition({ version: { token: "SENTINEL_SECRET" } as never }));
    const versionIssue = result.issues.find((entry) => entry.field === "version");
    expect(versionIssue?.message).toBe("Migration version must be a number, received object");
    expect(JSON.stringify(result)).not.toContain("SENTINEL_SECRET");
  });

  test("names non-finite numbers without stringifying NaN as an empty value", () => {
    const nanResult = validateMigrationDefinition(validDefinition({ version: Number.NaN }));
    expect(nanResult.issues.find((entry) => entry.field === "version")?.message).toContain("received NaN");

    const infinityResult = validateMigrationDefinition(validDefinition({ version: Number.POSITIVE_INFINITY }));
    expect(infinityResult.issues.find((entry) => entry.field === "version")?.message).toContain("received Infinity");
  });
});

describe("name boundaries", () => {
  test("accepts a name at the identifier length limit", () => {
    const name = "a".repeat(128);
    expect(validateMigrationDefinition(validDefinition({ name })).valid).toBe(true);
  });

  test("rejects a name one character over the limit", () => {
    const name = "a".repeat(129);
    const result = validateMigrationDefinition(validDefinition({ name }));
    expect(result.issues.find((entry) => entry.field === "name")?.message).toContain("at most 128 characters");
  });

  const rejected: [string, string][] = [
    ["an uppercase character", "Add_Index"],
    ["a space", "add invoice index"],
    ["a hyphen", "add-invoice-index"],
    ["a leading underscore", "_add_index"],
    ["a trailing underscore", "add_index_"],
    ["a doubled underscore", "add__index"],
    ["a path traversal attempt", "../../etc/passwd"],
  ];

  test.each(rejected)("rejects %s", (_label, name) => {
    const result = validateMigrationDefinition(validDefinition({ name }));
    expect(issueFor(result, "name")).toBe(true);
  });

  test("accepts digits anywhere in a snake_case name, matching the filename pattern", () => {
    expect(validateMigrationDefinition(validDefinition({ name: "1add_index" })).valid).toBe(true);
    expect(validateMigrationDefinition(validDefinition({ name: "add_v2_index" })).valid).toBe(true);
  });
});

describe("authoredAt calendar boundaries", () => {
  const accepted: [string, string][] = [
    ["a leap day in a leap year", "2024-02-29"],
    ["the last day of a 31-day month", "2026-04-30"],
    ["the epoch boundary", "1970-01-01"],
  ];

  test.each(accepted)("accepts %s", (_label, authoredAt) => {
    expect(validateMigrationDefinition(validDefinition({ authoredAt })).valid).toBe(true);
  });

  const rejected: [string, string][] = [
    ["a leap day in a non-leap year", "2026-02-29"],
    ["month zero", "2026-00-10"],
    ["month thirteen", "2026-13-01"],
    ["day zero", "2026-04-00"],
    ["day thirty-two", "2026-04-32"],
    ["a day past the end of a 30-day month", "2026-04-31"],
    ["a non-ISO string", "26/04/2026"],
    ["a bare local-time timestamp with no zone", "2026-04-26T14:30:00"],
    ["an empty string", ""],
  ];

  test.each(rejected)("rejects %s", (_label, authoredAt) => {
    const result = validateMigrationDefinition(validDefinition({ authoredAt }));
    expect(issueFor(result, "authoredAt")).toBe(true);
  });
});

describe("hotfix metadata matrix", () => {
  const hotfix = (meta: Record<string, unknown>, down?: unknown): Record<string, unknown> =>
    validDefinition({ meta, down: down as never });

  test("reports every missing hotfix requirement in one pass", () => {
    const result = validateMigrationDefinition(hotfix({ hotfix: true }));
    expect(fieldsOf(result)).toEqual(["meta.reason", "meta.rollback_risk", "down"]);
    expect(result.issues.every((entry) => entry.code === CODE)).toBe(true);
  });

  test("reports a blank hotfix reason as missing", () => {
    const result = validateMigrationDefinition(
      hotfix({ hotfix: true, reason: "   ", rollback_risk: "low" }, async () => undefined),
    );
    expect(fieldsOf(result)).toEqual(["meta.reason"]);
  });

  test("reports a non-callable down on a hotfix once, under the down field", () => {
    const result = validateMigrationDefinition(
      hotfix({ hotfix: true, reason: "deadlock", rollback_risk: "low" }, true),
    );
    expect(fieldsOf(result)).toEqual(["down"]);
  });

  test("reads the rollback from the top level of the definition, not from meta", () => {
    // The documented shape puts `down` beside `up`; a `down` nested under `meta`
    // must not satisfy the hotfix requirement.
    const result = validateMigrationDefinition(
      validDefinition({
        meta: { hotfix: true, reason: "deadlock", rollback_risk: "low", down: async () => undefined },
      }),
    );
    expect(fieldsOf(result)).toEqual(["down"]);
  });

  test("does not apply hotfix rules when meta.hotfix is not strictly true", () => {
    for (const value of [false, "true", 1, undefined, null]) {
      const result = validateMigrationDefinition(hotfix({ hotfix: value }));
      expect(result).toEqual({ valid: true, errors: [], issues: [] });
    }
  });

  test("validates an optional meta.hotfix_flag against the flag union", () => {
    expect(validateMigrationDefinition(hotfix({ hotfix_flag: HotfixFlags.URGENT })).valid).toBe(true);
    expect(issueFor(validateMigrationDefinition(hotfix({ hotfix_flag: "CRITICAL" })), "meta.hotfix_flag")).toBe(true);
    expect(issueFor(validateMigrationDefinition(hotfix({ hotfix_flag: 3 })), "meta.hotfix_flag")).toBe(true);
  });

  test("keeps hotfix_flag validation independent of the hotfix gate", () => {
    const result = validateMigrationDefinition(
      hotfix({ hotfix: false, hotfix_flag: "escalate" }, async () => undefined),
    );
    expect(fieldsOf(result)).toEqual(["meta.hotfix_flag"]);
    expect(result.issues[0].message).toContain("critical, urgent, standard");
  });
});

describe("list-level invariants", () => {
  test("accepts an empty list", () => {
    expect(validateMigrationDefinitions([])).toEqual({ valid: true, errors: [], issues: [] });
  });

  test("rejects a non-array without throwing", () => {
    const result = validateMigrationDefinitions({ version: 1 });
    expect(fieldsOf(result)).toEqual(["definitions"]);
    expect(result.errors[0]).toContain(CODE);
  });

  test("accepts a gap-free, strictly increasing list", () => {
    const result = validateMigrationDefinitions([
      validDefinition({ version: 1, name: "a" }),
      validDefinition({ version: 2, name: "b" }),
      validDefinition({ version: 3, name: "c" }),
    ]);
    expect(result.valid).toBe(true);
  });

  test("reports every repeat of a duplicated version against its first index", () => {
    const result = validateMigrationDefinitions([
      validDefinition({ version: 906, name: "dup_alpha" }),
      validDefinition({ version: 907, name: "unique" }),
      validDefinition({ version: 906, name: "dup_beta" }),
      validDefinition({ version: 906, name: "dup_gamma" }),
    ]);
    const duplicates = result.issues.filter((entry) => entry.message.startsWith("Duplicate migration version"));
    expect(duplicates.map((entry) => entry.field)).toEqual([
      "definitions[2] (906_dup_beta).version",
      "definitions[3] (906_dup_gamma).version",
    ]);
    expect(duplicates[0].message).toContain("first declared at index 0");
  });

  test("reports descending versions against the entry that broke the sequence", () => {
    const result = validateMigrationDefinitions([
      validDefinition({ version: 3, name: "c" }),
      validDefinition({ version: 1, name: "a" }),
    ]);
    expect(result.issues.map((entry) => entry.field)).toEqual(["definitions[1] (1_a).version"]);
    expect(result.issues[0].message).toContain("1 follows 3");
  });

  test("allows a version gap, which is a file-level check rather than a definition invariant", () => {
    expect(validateMigrationDefinitions([
      validDefinition({ version: 1, name: "a" }),
      validDefinition({ version: 7, name: "b" }),
    ]).valid).toBe(true);
  });

  test("excludes entries with an unusable version from the list-level checks", () => {
    const result = validateMigrationDefinitions([
      validDefinition({ version: 5, name: "a" }),
      validDefinition({ version: Number.NaN, name: "b" }),
      validDefinition({ version: 4, name: "c" }),
    ]);
    // The NaN entry is reported for itself, and the 4-after-5 inversion is
    // still caught because both sides are usable versions.
    expect(fieldsOf(result)).toEqual(["definitions[1].version", "definitions[2] (4_c).version"]);
    expect(result.issues[1].message).toContain("4 follows 5");
  });

  test("labels entries without a derivable name by index alone", () => {
    const result = validateMigrationDefinitions([validDefinition({ version: 1, name: "BAD NAME" })]);
    // The malformed name is not embedded in the field path, because the path
    // must stay parseable even when the value it refers to is not.
    expect(fieldsOf(result)).toEqual(["definitions[0].name"]);
  });

  test("returns per-entry issues before list-level issues", () => {
    const result = validateMigrationDefinitions([
      validDefinition({ version: 2, name: "b" }),
      validDefinition({ version: 2, name: "b_again", up: "not a function" as never }),
    ]);
    expect(result.issues.map((entry) => entry.field)).toEqual([
      "definitions[1] (2_b_again).up",
      "definitions[1] (2_b_again).version",
    ]);
    expect(result.issues[1].message).toContain("Duplicate migration version 2");
    expect(result.issues.every((entry) => entry.code === CODE)).toBe(true);
  });
});

describe("determinism and secret hygiene", () => {
  test("repeated validation of equal inputs returns deep-equal reports", () => {
    const input = [validDefinition({ version: 1, name: "a" }), validDefinition({ version: 1, name: "b" })];
    expect(validateMigrationDefinitions(input)).toEqual(validateMigrationDefinitions(input));
  });

  test("issue ordering is stable across repeated calls", () => {
    const broken = { version: -1, name: "Bad Name", authoredAt: "not-a-date", author: "", up: "nope" };
    const first = validateMigrationDefinition(broken);
    const second = validateMigrationDefinition(broken);
    expect(first.errors).toEqual(second.errors);
    expect(fieldsOf(first)).toEqual(["version", "name", "authoredAt", "author", "up"]);
  });

  test("every error string carries the validation code and a field path", () => {
    const result = validateMigrationDefinition({ version: 0 });
    for (const error of result.errors) {
      expect(error.startsWith(`${CODE} `)).toBe(true);
      expect(error).toMatch(new RegExp(`^${CODE} [a-zA-Z.[\\]0-9]+: `));
    }
  });

  test("does not leak object contents from any malformed field", () => {
    const result = validateMigrationDefinition({
      version: "SENTINEL_SECRET" as never,
      name: { toString: () => "SENTINEL_SECRET" } as never,
      authoredAt: ["SENTINEL_SECRET"] as never,
      author: { secret: "SENTINEL_SECRET" } as never,
      up: { secret: "SENTINEL_SECRET" } as never,
      meta: { hotfix: true, reason: { secret: "SENTINEL_SECRET" }, rollback_risk: { secret: "SENTINEL_SECRET" } },
    });
    expect(result.valid).toBe(false);
    expect(JSON.stringify(result)).not.toContain("SENTINEL_SECRET");
  });
});

describe("type guards", () => {
  test("isHotfixFlag accepts only the declared flag values", () => {
    for (const flag of Object.values(HotfixFlags)) {
      expect(isHotfixFlag(flag)).toBe(true);
    }
    for (const value of ["CRITICAL", "Critical", "", " critical", null, undefined, 0, {}]) {
      expect(isHotfixFlag(value)).toBe(false);
    }
  });

  test("isMigrationDefinition mirrors the scalar and callable checks", () => {
    expect(isMigrationDefinition(validDefinition())).toBe(true);
    const withHotfixMeta = validDefinition({ down: async () => undefined, meta: { hotfix: true } });
    expect(isMigrationDefinition(withHotfixMeta)).toBe(true);

    expect(isMigrationDefinition(validDefinition({ version: 0 }))).toBe(false);
    expect(isMigrationDefinition(validDefinition({ name: "Bad Name" }))).toBe(false);
    expect(isMigrationDefinition(validDefinition({ authoredAt: "2026-02-30" }))).toBe(false);
    expect(isMigrationDefinition(validDefinition({ author: "" }))).toBe(false);
    expect(isMigrationDefinition(validDefinition({ up: undefined as never }))).toBe(false);
    expect(isMigrationDefinition(validDefinition({ down: "nope" as never }))).toBe(false);
    expect(isMigrationDefinition(validDefinition({ meta: [] as never }))).toBe(false);
    expect(isMigrationDefinition(null)).toBe(false);
    expect(isMigrationDefinition([])).toBe(false);
  });

  test("isMigrationDefinition stays true for a hotfix missing protocol metadata", () => {
    // Hotfix completeness is reported as issues rather than folded into the
    // boolean, so triage can distinguish a shape error from a policy error.
    expect(isMigrationDefinition(validDefinition({ meta: { hotfix: true } }))).toBe(true);
    expect(validateMigrationDefinition(validDefinition({ meta: { hotfix: true } })).valid).toBe(false);
  });

  test("agrees with validateMigrationDefinition on the scalar subset", () => {
    const samples: unknown[] = [
      validDefinition(),
      validDefinition({ version: 0 }),
      validDefinition({ name: "Bad" }),
      validDefinition({ authoredAt: "2026-02-30" }),
      validDefinition({ up: "nope" as never }),
      validDefinition({ meta: "nope" as never }),
      {},
      null,
      "v001",
    ];
    for (const sample of samples) {
      const guard = isMigrationDefinition(sample);
      const report = validateMigrationDefinition(sample);
      if (!report.valid) expect(guard).toBe(false);
    }
  });
});

describe("property-based boundaries", () => {
  const nameArb = fc
    .array(fc.stringMatching(/^[a-z0-9]+$/), { minLength: 1, maxLength: 4 })
    .map((segments) => segments.join("_"));

  const authorArb = fc.stringMatching(/^[A-Za-z][A-Za-z0-9 ._-]{0,40}$/);

  const validDefinitionArb = fc.record({
    version: fc.integer({ min: 1, max: 10_000 }),
    name: nameArb,
    authoredAt: fc.constantFrom("2026-04-26", "2024-02-29", "2026-05-28T00:00:00Z", "1970-01-01"),
    author: authorArb,
    up: fc.constant(async () => undefined),
  });

  test("any well-formed definition is accepted and passes both guards", () => {
    fc.assert(
      fc.property(validDefinitionArb, (definition) => {
        const result = validateMigrationDefinition(definition);
        expect(result.issues).toEqual([]);
        expect(result.valid).toBe(true);
        expect(isMigrationDefinition(definition)).toBe(true);
        expect(validateMigrationDefinitions([definition])).toEqual({ valid: true, errors: [], issues: [] });
      }),
      { numRuns: 200 },
    );
  });

  test("arbitrary junk never throws and always yields a coded report", () => {
    const junkArb = fc.oneof(
      fc.anything({ maxDepth: 2 }),
      fc.dictionary(fc.string(), fc.anything({ maxDepth: 1 })),
      fc.constantFrom(null, undefined, 0, "", [], new Date(0)),
    );

    fc.assert(
      fc.property(junkArb, (value) => {
        const definitionReport = validateMigrationDefinition(value);
        const listReport = validateMigrationDefinitions(value);
        expect(typeof definitionReport.valid).toBe("boolean");
        expect(typeof listReport.valid).toBe("boolean");
        expect(definitionReport.errors.length).toBe(definitionReport.issues.length);
        expect(listReport.errors.length).toBe(listReport.issues.length);
        expect(definitionReport.issues.every((entry) => entry.code === CODE)).toBe(true);
        expect(() => isMigrationDefinition(value)).not.toThrow();
        expect(() => isHotfixFlag(value)).not.toThrow();
      }),
      { numRuns: 300 },
    );
  });

  test("duplicate versions are always detected exactly once per repeat", () => {
    fc.assert(
      fc.property(
        fc.array(fc.integer({ min: 1, max: 5 }), { minLength: 2, maxLength: 8 }),
        (versions) => {
          const list = versions.map((version) => validDefinition({ version, name: `step_${version}` }));
          const duplicates = validateMigrationDefinitions(list).issues.filter((entry) =>
            entry.message.startsWith("Duplicate migration version"),
          );
          const counts = new Map<number, number>();
          for (const version of versions) counts.set(version, (counts.get(version) ?? 0) + 1);
          const expected = [...counts.values()].reduce((total, count) => total + Math.max(count - 1, 0), 0);
          expect(duplicates).toHaveLength(expected);
        },
      ),
      { numRuns: 200 },
    );
  });

  test("validation is idempotent under repeated application", () => {
    fc.assert(
      fc.property(validDefinitionArb, (definition) => {
        const snapshot = JSON.stringify(definition, Object.keys(definition).sort());
        validateMigrationDefinition(definition);
        validateMigrationDefinition(definition);
        expect(JSON.stringify(definition, Object.keys(definition).sort())).toBe(snapshot);
      }),
      { numRuns: 100 },
    );
  });
});

describe("checked-in migrations satisfy the contracts", () => {
  const migrationsDir = path.resolve(__dirname, "..", "migrations");
  const files = fs.existsSync(migrationsDir)
    ? fs
        .readdirSync(migrationsDir)
        .filter((file) => /^v\d{3}_[a-z0-9_]+\.ts$/.test(file))
        .sort()
    : [];

  const load = (file: string): unknown => {
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const loaded = require(path.join(migrationsDir, file)) as { default?: unknown };
    return (loaded.default ?? loaded) as unknown;
  };

  test("loads at least one checked-in migration", () => {
    expect(files.length).toBeGreaterThan(0);
  });

  test("every checked-in migration satisfies the per-definition contracts", () => {
    for (const file of files) {
      const result = validateMigrationDefinition(load(file));
      expect({ file, errors: result.errors }).toEqual({ file, errors: [] });
    }
  });

  test("surfaces the duplicate version numbers already present in the checked-in set", () => {
    // Five files declare version 6 and four declare version 11. The runner keys
    // applied state on the version alone, so a repeat is skipped as already
    // applied; this test pins the current, deterministic report so the collision
    // stays visible instead of being silently accepted.
    const result = validateMigrationDefinitions(files.map(load));
    const duplicates = result.issues.filter((entry) => entry.message.startsWith("Duplicate migration version"));
    expect(duplicates.map((entry) => entry.message.split(" ")[3])).toEqual(["6", "6", "6", "6", "11", "11", "11"]);
    expect(result.issues.every((entry) => entry.code === CODE)).toBe(true);
  });
});
