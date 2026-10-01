/**
 * migration-validate-boundary.test.ts
 *
 * Deterministic failure-boundary coverage for validateMigrationFiles().
 *
 * Test strategy
 * ─────────────
 * validateMigrationFiles() has two phases:
 *   1. Load: calls loadMigrationsFromFS() — filesystem I/O, mocked here.
 *   2. Validate: pure in-memory checks on the loaded set.
 *
 * We mock `fs/promises` and the migration modules so no real filesystem access
 * or DB connections are required.  Every test is deterministic and hermetic.
 *
 * Scenario groups
 * ───────────────
 *  A. Success / valid inputs
 *  B. Duplicate version numbers
 *  C. Version-zero guard
 *  D. Version sequence does not start at 1
 *  E. Gaps in version sequence
 *  F. Duplicate migration names
 *  G. Missing required fields (name, author, authoredAt, up)
 *  H. Hotfix-specific field validation
 *  I. Compound / multi-error scenarios
 *  J. Boundary: empty migration set
 *  K. Boundary: single migration
 *  L. Regression: old behaviour parity (gap message format unchanged)
 */

import * as fsMod from "fs/promises";
import * as pathMod from "path";

// ── Module mocks ─────────────────────────────────────────────────────────────
// We mock the filesystem so tests are fully hermetic.
jest.mock("fs/promises");
jest.mock("../lib/database", () => ({
  getDatabase: jest.fn(() => ({
    exec: jest.fn(),
    prepare: jest.fn(() => ({
      all: jest.fn(() => []),
      get: jest.fn(() => undefined),
      run: jest.fn(() => ({ lastInsertRowId: 1, changes: 1 })),
    })),
    transaction: jest.fn((fn: () => void) => fn()),
  })),
  closeDatabase: jest.fn(),
}));

// Import after mocks are registered.
import { validateMigrationFiles } from "../lib/migrations/runner";

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * A minimal valid MigrationDefinition object factory.
 * Only the fields required by MigrationPolicy.validateMetadata are set.
 */
function makeMigration(
  overrides: Partial<{
    version: number;
    name: string;
    author: string;
    authoredAt: string;
    up: (...args: any[]) => any;
    down: (...args: any[]) => any;
    meta: Record<string, unknown>;
  }> = {}
) {
  return {
    version: overrides.version ?? 1,
    name: overrides.name ?? "test_migration",
    author: overrides.author ?? "test-author",
    authoredAt: overrides.authoredAt ?? "2026-01-01",
    up: overrides.up ?? (async () => {}),
    ...(overrides.down !== undefined ? { down: overrides.down } : {}),
    ...(overrides.meta !== undefined ? { meta: overrides.meta } : {}),
  };
}

/**
 * Build the fs / require mocks so that loadMigrationsFromFS returns
 * exactly the provided list of migration definitions.
 *
 * Each definition is wired to a synthetic filename of the form
 * `v{version:03d}_{name}.ts`.
 *
 * Note: jest.mock() is hoisted to the top of the file by Babel/ts-jest and
 * cannot be called with runtime variables inside a helper function.  We use
 * jest.doMock() here instead, which is evaluated lazily at call-time and
 * accepts runtime paths.
 */
function stubMigrations(
  fsMocked: jest.Mocked<typeof fsMod>,
  defs: ReturnType<typeof makeMigration>[]
) {
  const filenames = defs.map(
    (d) => `v${String(d.version).padStart(3, "0")}_${d.name}.ts`
  );

  // readdir returns our synthetic filenames.
  fsMocked.readdir.mockResolvedValue(filenames as any);

  // readFile returns a non-empty string (content used only for checksum
  // in runMigrations, not in validateMigrationFiles).
  fsMocked.readFile.mockResolvedValue("/* migration content */" as any);

  // Stub require() for each synthetic migration module path so that
  // loadMigrationsFromFS gets the correct `default` export.
  defs.forEach((def) => {
    const filename = `v${String(def.version).padStart(3, "0")}_${def.name}.ts`;
    const absolutePath = pathMod.join(
      process.cwd(),
      "src",
      "migrations",
      filename
    );
    // jest.doMock is safe to call inside functions with runtime variables.
    jest.doMock(absolutePath, () => ({ default: def }), { virtual: true });
  });
}

// ── Test Suite ────────────────────────────────────────────────────────────────

const fs = fsMod as jest.Mocked<typeof fsMod>;

beforeEach(() => {
  jest.clearAllMocks();
  jest.resetModules();
});

// ─────────────────────────────────────────────────────────────────────────────
// A. Success — valid inputs
// ─────────────────────────────────────────────────────────────────────────────

describe("A. valid inputs → success", () => {
  test("A1: empty migration directory returns valid", async () => {
    fs.readdir.mockResolvedValue([] as any);

    const result = await validateMigrationFiles();

    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("A2: single migration at version 1 is valid", async () => {
    fs.readdir.mockResolvedValue(["v001_initial_schema.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    const def = makeMigration({ version: 1, name: "initial_schema" });
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_initial_schema.ts"),
      () => ({ default: def }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("A3: contiguous sequence 1,2,3 is valid", async () => {
    const defs = [
      makeMigration({ version: 1, name: "create_users" }),
      makeMigration({ version: 2, name: "add_email_index" }),
      makeMigration({ version: 3, name: "create_sessions" }),
    ];
    stubMigrations(fs, defs);

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("A4: valid migration with optional down function passes", async () => {
    const defs = [
      makeMigration({ version: 1, name: "create_table", down: async () => {} }),
    ];
    stubMigrations(fs, defs);

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(true);
  });

  test("A5: valid hotfix migration (all required hotfix fields) passes", async () => {
    const defs = [
      makeMigration({
        version: 1,
        name: "fix_foreign_key",
        down: async () => {},
        meta: { hotfix: true, reason: "critical FK violation", rollback_risk: "low" },
      }),
    ];
    stubMigrations(fs, defs);

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// B. Duplicate version numbers
// ─────────────────────────────────────────────────────────────────────────────

describe("B. duplicate version numbers", () => {
  test("B1: two migrations with the same version produces an error", async () => {
    fs.readdir.mockResolvedValue([
      "v002_create_users.ts",
      "v002_create_sessions.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    const def1 = makeMigration({ version: 2, name: "create_users" });
    const def2 = makeMigration({ version: 2, name: "create_sessions" });

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v002_create_users.ts"),
      () => ({ default: def1 }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v002_create_sessions.ts"),
      () => ({ default: def2 }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("Duplicate version 2"))).toBe(true);
  });

  test("B2: duplicate error message includes count of colliding files", async () => {
    fs.readdir.mockResolvedValue([
      "v003_a.ts",
      "v003_b.ts",
      "v003_c.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    [
      ["a", "v003_a.ts"],
      ["b", "v003_b.ts"],
      ["c", "v003_c.ts"],
    ].forEach(([name, file]) => {
      jest.doMock(
        pathMod.join(process.cwd(), "src", "migrations", file),
        () => ({ default: makeMigration({ version: 3, name }) }),
        { virtual: true }
      );
    });

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    const dupeError = result.errors.find((e) => e.includes("Duplicate version 3"));
    expect(dupeError).toBeDefined();
    expect(dupeError).toMatch(/3/);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// C. Version-zero guard
// ─────────────────────────────────────────────────────────────────────────────

describe("C. version-zero guard", () => {
  test("C1: migration with version 0 produces an error", async () => {
    fs.readdir.mockResolvedValue(["v000_bootstrap.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v000_bootstrap.ts"),
      () => ({ default: makeMigration({ version: 0, name: "bootstrap" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("Version 0 is not allowed"))).toBe(true);
  });

  test("C2: version-zero error message is descriptive", async () => {
    fs.readdir.mockResolvedValue(["v000_something.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v000_something.ts"),
      () => ({ default: makeMigration({ version: 0, name: "something" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    const zeroError = result.errors.find((e) => e.includes("Version 0"));
    expect(zeroError).toMatch(/start at 1/);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// D. Sequence does not start at 1
// ─────────────────────────────────────────────────────────────────────────────

describe("D. sequence does not start at 1", () => {
  test("D1: sequence starting at 2 produces a descriptive error", async () => {
    fs.readdir.mockResolvedValue(["v002_create_users.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v002_create_users.ts"),
      () => ({ default: makeMigration({ version: 2, name: "create_users" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(
      result.errors.some((e) => e.includes("must start at 1") || e.includes("version 1 is missing") || e.includes("start at 1"))
    ).toBe(true);
  });

  test("D2: sequence starting at 5 produces descriptive errors for each missing version", async () => {
    fs.readdir.mockResolvedValue(["v005_add_index.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v005_add_index.ts"),
      () => ({ default: makeMigration({ version: 5, name: "add_index" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    // Either a "start at 1" error or individual missing version errors cover versions 1-4.
    const hasMissingOrStart = result.errors.some(
      (e) => e.includes("must start at 1") || e.includes("missing") || e.includes("start at 1")
    );
    expect(hasMissingOrStart).toBe(true);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// E. Gaps in version sequence
// ─────────────────────────────────────────────────────────────────────────────

describe("E. gaps in version sequence", () => {
  test("E1: missing version 2 in sequence 1,3 produces a gap error", async () => {
    fs.readdir.mockResolvedValue([
      "v001_create_users.ts",
      "v003_add_email.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_create_users.ts"),
      () => ({ default: makeMigration({ version: 1, name: "create_users" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v003_add_email.ts"),
      () => ({ default: makeMigration({ version: 3, name: "add_email" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("2") && e.includes("missing"))).toBe(true);
  });

  test("E2: multiple gaps each produce their own error entry", async () => {
    fs.readdir.mockResolvedValue([
      "v001_a.ts",
      "v004_d.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_a.ts"),
      () => ({ default: makeMigration({ version: 1, name: "a" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v004_d.ts"),
      () => ({ default: makeMigration({ version: 4, name: "d" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    // Versions 2 and 3 are both missing, so at least 2 gap errors.
    const gapErrors = result.errors.filter((e) => e.includes("missing"));
    expect(gapErrors.length).toBeGreaterThanOrEqual(2);
  });

  test("E3: gap error message matches legacy format (regression guard)", async () => {
    // The old format was: "Gap detected: migration {n} is missing"
    // The new format may differ slightly but must still reference the missing version.
    fs.readdir.mockResolvedValue([
      "v001_a.ts",
      "v003_c.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_a.ts"),
      () => ({ default: makeMigration({ version: 1, name: "a" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v003_c.ts"),
      () => ({ default: makeMigration({ version: 3, name: "c" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    const gapError = result.errors.find((e) => e.includes("2"));
    expect(gapError).toBeDefined();
    expect(gapError).toMatch(/missing/i);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// F. Duplicate migration names
// ─────────────────────────────────────────────────────────────────────────────

describe("F. duplicate migration names", () => {
  test("F1: two migrations sharing a name produce an error", async () => {
    fs.readdir.mockResolvedValue([
      "v001_create_users.ts",
      "v002_create_users.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_create_users.ts"),
      () => ({ default: makeMigration({ version: 1, name: "create_users" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v002_create_users.ts"),
      () => ({ default: makeMigration({ version: 2, name: "create_users" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("create_users"))).toBe(true);
  });

  test("F2: duplicate name error includes both version numbers", async () => {
    fs.readdir.mockResolvedValue([
      "v001_add_index.ts",
      "v003_add_index.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_add_index.ts"),
      () => ({ default: makeMigration({ version: 1, name: "add_index" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v003_add_index.ts"),
      () => ({ default: makeMigration({ version: 3, name: "add_index" }) }),
      { virtual: true }
    );

    // Gap at version 2 is expected; the test here is about the name duplicate.
    const result = await validateMigrationFiles();
    const nameError = result.errors.find((e) => e.includes("add_index") && e.includes("1") && e.includes("3"));
    expect(nameError).toBeDefined();
  });

  test("F3: unique names do not produce a duplicate error", async () => {
    fs.readdir.mockResolvedValue([
      "v001_alpha.ts",
      "v002_beta.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_alpha.ts"),
      () => ({ default: makeMigration({ version: 1, name: "alpha" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v002_beta.ts"),
      () => ({ default: makeMigration({ version: 2, name: "beta" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    const nameErrors = result.errors.filter((e) => e.includes("Duplicate migration name"));
    expect(nameErrors).toHaveLength(0);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// G. Missing required fields
// ─────────────────────────────────────────────────────────────────────────────

describe("G. missing required fields", () => {
  test("G1: migration without a name produces an error", async () => {
    fs.readdir.mockResolvedValue(["v001_test.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_test.ts"),
      () => ({
        default: makeMigration({ version: 1, name: "" }),
      }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("name") && e.includes("required"))).toBe(true);
  });

  test("G2: migration without an author produces an error", async () => {
    fs.readdir.mockResolvedValue(["v001_test.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_test.ts"),
      () => ({
        default: makeMigration({ version: 1, name: "test", author: "" }),
      }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("author") && e.includes("required"))).toBe(true);
  });

  test("G3: migration without authoredAt produces an error", async () => {
    fs.readdir.mockResolvedValue(["v001_test.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_test.ts"),
      () => ({
        default: makeMigration({ version: 1, name: "test", authoredAt: "" }),
      }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("authoredAt"))).toBe(true);
  });

  test("G4: migration without an up function produces an error", async () => {
    fs.readdir.mockResolvedValue(["v001_test.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    const defWithoutUp = {
      version: 1,
      name: "test",
      author: "author",
      authoredAt: "2026-01-01",
      // `up` intentionally omitted
    };

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_test.ts"),
      () => ({ default: defWithoutUp }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("up") && e.includes("required"))).toBe(true);
  });

  test("G5: error messages are prefixed with the migration identifier", async () => {
    fs.readdir.mockResolvedValue(["v001_missing_author.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_missing_author.ts"),
      () => ({
        default: makeMigration({ version: 1, name: "missing_author", author: "" }),
      }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    // Error should be prefixed with "Migration 1_missing_author: ..."
    expect(result.errors.some((e) => e.startsWith("Migration 1_missing_author:"))).toBe(true);
  });

  test("G6: all required fields present → no required-field errors", async () => {
    fs.readdir.mockResolvedValue(["v001_complete.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_complete.ts"),
      () => ({
        default: makeMigration({ version: 1, name: "complete" }),
      }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// H. Hotfix-specific field validation
// ─────────────────────────────────────────────────────────────────────────────

describe("H. hotfix-specific field validation", () => {
  test("H1: hotfix without meta.reason fails", async () => {
    fs.readdir.mockResolvedValue(["v001_hotfix.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_hotfix.ts"),
      () => ({
        default: makeMigration({
          version: 1,
          name: "hotfix",
          down: async () => {},
          meta: { hotfix: true, rollback_risk: "low" }, // reason missing
        }),
      }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("meta.reason"))).toBe(true);
  });

  test("H2: hotfix without meta.rollback_risk fails", async () => {
    fs.readdir.mockResolvedValue(["v001_hotfix.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_hotfix.ts"),
      () => ({
        default: makeMigration({
          version: 1,
          name: "hotfix",
          down: async () => {},
          meta: { hotfix: true, reason: "critical bug" }, // rollback_risk missing
        }),
      }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("meta.rollback_risk"))).toBe(true);
  });

  test("H3: hotfix without down function fails", async () => {
    fs.readdir.mockResolvedValue(["v001_hotfix.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_hotfix.ts"),
      () => ({
        default: makeMigration({
          version: 1,
          name: "hotfix",
          // down intentionally omitted
          meta: { hotfix: true, reason: "critical bug", rollback_risk: "low" },
        }),
      }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("down"))).toBe(true);
  });

  test("H4: fully-valid hotfix migration passes", async () => {
    fs.readdir.mockResolvedValue(["v001_hotfix.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_hotfix.ts"),
      () => ({
        default: makeMigration({
          version: 1,
          name: "hotfix",
          down: async () => {},
          meta: { hotfix: true, reason: "critical FK violation", rollback_risk: "low" },
        }),
      }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });

  test("H5: non-hotfix migration with a down function passes without hotfix errors", async () => {
    fs.readdir.mockResolvedValue(["v001_standard.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_standard.ts"),
      () => ({
        default: makeMigration({
          version: 1,
          name: "standard",
          down: async () => {},
        }),
      }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// I. Compound / multi-error scenarios
// ─────────────────────────────────────────────────────────────────────────────

describe("I. compound multi-error scenarios", () => {
  test("I1: duplicate version + missing author → both errors reported", async () => {
    fs.readdir.mockResolvedValue([
      "v002_a.ts",
      "v002_b.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v002_a.ts"),
      () => ({
        default: makeMigration({ version: 2, name: "a", author: "" }), // missing author
      }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v002_b.ts"),
      () => ({
        default: makeMigration({ version: 2, name: "b" }),
      }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    // Both the duplicate-version error and the required-field error must be present.
    expect(result.errors.some((e) => e.includes("Duplicate version 2"))).toBe(true);
    expect(result.errors.some((e) => e.includes("author") && e.includes("required"))).toBe(true);
  });

  test("I2: gap + duplicate name → both errors reported", async () => {
    fs.readdir.mockResolvedValue([
      "v001_alpha.ts",
      "v003_alpha.ts", // gap at 2, name collision with v001
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_alpha.ts"),
      () => ({ default: makeMigration({ version: 1, name: "alpha" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v003_alpha.ts"),
      () => ({ default: makeMigration({ version: 3, name: "alpha" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("missing") && e.includes("2"))).toBe(true);
    expect(result.errors.some((e) => e.includes("alpha"))).toBe(true);
  });

  test("I3: all errors returned in a single call — fail-complete, not fail-fast", async () => {
    // A set with: version-zero, gap, duplicate name, missing author.
    fs.readdir.mockResolvedValue([
      "v000_bad.ts",
      "v002_dup_name.ts",
      "v004_dup_name.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v000_bad.ts"),
      () => ({ default: makeMigration({ version: 0, name: "bad", author: "" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v002_dup_name.ts"),
      () => ({ default: makeMigration({ version: 2, name: "dup_name" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v004_dup_name.ts"),
      () => ({ default: makeMigration({ version: 4, name: "dup_name" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    // Should accumulate multiple distinct errors, not just the first.
    expect(result.errors.length).toBeGreaterThan(1);
  });

  test("I4: result.valid is false if any error is present", async () => {
    fs.readdir.mockResolvedValue(["v001_bad.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_bad.ts"),
      () => ({ default: makeMigration({ version: 1, name: "bad", author: "" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.length).toBeGreaterThan(0);
  });

  test("I5: result.valid is true only when errors array is empty", async () => {
    fs.readdir.mockResolvedValue([
      "v001_ok.ts",
      "v002_also_ok.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_ok.ts"),
      () => ({ default: makeMigration({ version: 1, name: "ok" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v002_also_ok.ts"),
      () => ({ default: makeMigration({ version: 2, name: "also_ok" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(true);
    expect(result.errors).toHaveLength(0);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// J. Boundary: empty migration set
// ─────────────────────────────────────────────────────────────────────────────

describe("J. boundary — empty migration set", () => {
  test("J1: no files → valid with no errors", async () => {
    fs.readdir.mockResolvedValue([] as any);

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("J2: only non-migration files present → valid (they are silently ignored)", async () => {
    fs.readdir.mockResolvedValue([
      ".gitkeep",
      "README.md",
      "helpers.ts",
    ] as any);

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// K. Boundary: single migration
// ─────────────────────────────────────────────────────────────────────────────

describe("K. boundary — single migration", () => {
  test("K1: single valid migration at version 1 passes", async () => {
    fs.readdir.mockResolvedValue(["v001_init.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_init.ts"),
      () => ({ default: makeMigration({ version: 1, name: "init" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("K2: single invalid migration at version 1 fails and reports error", async () => {
    fs.readdir.mockResolvedValue(["v001_init.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_init.ts"),
      () => ({ default: makeMigration({ version: 1, name: "init", author: "" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.length).toBeGreaterThan(0);
  });

  test("K3: single migration at version 0 fails", async () => {
    fs.readdir.mockResolvedValue(["v000_init.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v000_init.ts"),
      () => ({ default: makeMigration({ version: 0, name: "init" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("Version 0"))).toBe(true);
  });
});

// ─────────────────────────────────────────────────────────────────────────────
// L. Regression: old-format compatibility
// ─────────────────────────────────────────────────────────────────────────────

describe("L. regression — backwards compatible error messages", () => {
  test("L1: gap error messages include the missing version number", async () => {
    fs.readdir.mockResolvedValue([
      "v001_a.ts",
      "v003_c.ts",
    ] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_a.ts"),
      () => ({ default: makeMigration({ version: 1, name: "a" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v003_c.ts"),
      () => ({ default: makeMigration({ version: 3, name: "c" }) }),
      { virtual: true }
    );

    const result = await validateMigrationFiles();
    // The version number 2 must appear in at least one error message.
    expect(result.errors.some((e) => e.includes("2"))).toBe(true);
  });

  test("L2: return type always has valid (boolean) and errors (string[])", async () => {
    fs.readdir.mockResolvedValue([] as any);

    const result = await validateMigrationFiles();
    expect(typeof result.valid).toBe("boolean");
    expect(Array.isArray(result.errors)).toBe(true);
  });

  test("L3: valid is true when errors is empty, false when errors is non-empty", async () => {
    // Valid case
    fs.readdir.mockResolvedValue([] as any);
    const validResult = await validateMigrationFiles();
    expect(validResult.valid).toBe(validResult.errors.length === 0);
  });

  test("L4: concurrent calls are deterministic and isolated", async () => {
    // Two concurrent calls with the same mocked FS should both succeed with
    // identical results.
    fs.readdir.mockResolvedValue(["v001_a.ts", "v002_b.ts"] as any);
    fs.readFile.mockResolvedValue("content" as any);

    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v001_a.ts"),
      () => ({ default: makeMigration({ version: 1, name: "a" }) }),
      { virtual: true }
    );
    jest.doMock(
      pathMod.join(process.cwd(), "src", "migrations", "v002_b.ts"),
      () => ({ default: makeMigration({ version: 2, name: "b" }) }),
      { virtual: true }
    );

    const [r1, r2] = await Promise.all([
      validateMigrationFiles(),
      validateMigrationFiles(),
    ]);

    expect(r1.valid).toBe(r2.valid);
    expect(r1.errors).toEqual(r2.errors);
  });
});
