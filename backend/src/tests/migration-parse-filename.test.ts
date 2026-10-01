/**
 * Deterministic failure-boundary coverage for `parseMigrationFilename`
 * (`backend/src/lib/migrations/runner.ts`).
 *
 * The function is the single gate that decides whether a directory entry is a
 * migration at all, and the `{version, name}` pair it returns is also the input
 * used to build hotfix approval paths. It must therefore be a pure, total
 * function: every input either yields a canonical `{version, name}` or `null`,
 * never throws, and never accepts an ambiguous name.
 */

// The runner imports `../database`, which pulls in the native better-sqlite3
// binding. This suite only exercises the pure parser, so stub the database.
jest.mock("../lib/database", () => ({
  getDatabase: jest.fn(),
  closeDatabase: jest.fn(),
}));

import { parseMigrationFilename } from "../lib/migrations/runner";

describe("parseMigrationFilename — deterministic boundaries", () => {
  describe("accepts the canonical shape", () => {
    const valid: Array<[string, number, string]> = [
      ["v901_baseline.ts", 901, "baseline"],
      ["001_init.ts", 1, "init"],
      ["000_zero.ts", 0, "zero"],
      ["007_bond_two.ts", 7, "bond_two"],
      ["999_alpha_2.ts", 999, "alpha_2"],
      ["042_invoice_v2_final.ts", 42, "invoice_v2_final"],
      // name boundaries: leading / trailing / doubled underscores, numeric name
      ["001__init.ts", 1, "_init"],
      ["001_init_.ts", 1, "init_"],
      ["000_0.ts", 0, "0"],
    ];

    test.each(valid)("%s -> {version: %i, name: %s}", (filename, version, name) => {
      expect(parseMigrationFilename(filename)).toEqual({ version, name });
    });
  });

  describe("rejects malformed names", () => {
    const invalid: string[] = [
      // version must be exactly three digits
      "1_init.ts",
      "12_init.ts",
      "0001_init.ts",
      "abc_init.ts",
      // separator must be a single underscore
      "001-init.ts",
      "001 init.ts",
      "001init.ts",
      // names and the optional leading 'v' are lowercase only
      "001_Init.ts",
      "001_INIT.ts",
      "V001_init.ts",
      "vv001_init.ts",
      // extension must be exactly '.ts'
      "001_init.js",
      "001_init.tsx",
      "001_init.TS",
      "001_init.ts.bak",
      "001_init",
      "001_init.test.ts",
      // the name must be non-empty
      "001_.ts",
      // surrounding whitespace / control characters are not tolerated
      " 001_init.ts",
      "001_init.ts ",
      "001_init.ts\n",
      "001_init.ts\t",
      // directory entries, not paths
      "dir/001_init.ts",
      "",
      "README.txt",
      "notes.ts",
    ];

    test.each(invalid)("%s -> null", (filename) => {
      expect(parseMigrationFilename(filename)).toBeNull();
    });
  });

  describe("invariants", () => {
    test("a successful parse yields a 0..=999 version and a bare identifier name", () => {
      const samples = [
        "v901_baseline.ts",
        "000_zero.ts",
        "999_alpha_2.ts",
        "042_invoice_v2_final.ts",
      ];
      for (const filename of samples) {
        const parsed = parseMigrationFilename(filename);
        expect(parsed).not.toBeNull();
        expect(parsed!.version).toBeGreaterThanOrEqual(0);
        expect(parsed!.version).toBeLessThanOrEqual(999);
        // Matches APPROVAL_NAME_PATTERN that the runner re-checks downstream.
        expect(parsed!.name).toMatch(/^[a-z0-9_]+$/);
      }
    });

    test("is pure: repeated calls deep-equal and return independent objects", () => {
      const first = parseMigrationFilename("042_invoice_v2_final.ts")!;
      const second = parseMigrationFilename("042_invoice_v2_final.ts")!;
      expect(second).toEqual(first);

      // Mutating one result must not leak into a later call.
      first.version = 0;
      first.name = "mutated";
      expect(parseMigrationFilename("042_invoice_v2_final.ts")).toEqual({
        version: 42,
        name: "invoice_v2_final",
      });
    });

    test("round-trips the canonical filename it is meant to parse", () => {
      const versions = [0, 1, 7, 42, 123, 999];
      const names = ["a", "bond_two", "x_1"];
      for (const version of versions) {
        for (const name of names) {
          const filename = `v${String(version).padStart(3, "0")}_${name}.ts`;
          expect(parseMigrationFilename(filename)).toEqual({ version, name });
        }
      }
    });

    test("is total: adversarial inputs never throw and always resolve to null", () => {
      const adversarial = [
        "\u0000",
        "001_\u0000",
        ".ts",
        "_.ts",
        "v.ts",
        "001.ts",
        "../../001_x.ts",
        "001_x.ts\u0000",
      ];
      for (const input of adversarial) {
        let result: { version: number; name: string } | null = null;
        expect(() => {
          result = parseMigrationFilename(input);
        }).not.toThrow();
        expect(result).toBeNull();
      }
    });
  });
});
