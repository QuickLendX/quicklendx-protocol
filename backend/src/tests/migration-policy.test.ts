import { MigrationPolicy, migrateCommand, migrateDownCommand } from "../lib/migrations/policy";
import type { MigrationDefinition } from "../lib/migrations/types";

const mkMig = (over: Partial<MigrationDefinition> = {}): MigrationDefinition => ({
  version: 1,
  name: "test",
  authoredAt: "2026-04-26",
  author: "alice",
  up: async () => {},
  ...over,
});

describe("MigrationPolicy", () => {
  beforeEach(() => {
    process.env.NODE_ENV = "development";
    process.env.ALLOW_DOWN_MIGRATIONS = "false";
  });

  test("isDownAllowed returns false by default", () => {
    expect(MigrationPolicy.isDownAllowed()).toBe(false);
  });

  test("isDownAllowed returns true when env is true", () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    expect(MigrationPolicy.isDownAllowed()).toBe(true);
  });

  test("isHotfix returns true when meta.hotfix is set", () => {
    const mig = mkMig({ meta: { hotfix: true } });
    expect(MigrationPolicy.isHotfix(mig)).toBe(true);
  });

  test("isHotfix returns false when meta is missing", () => {
    expect(MigrationPolicy.isHotfix(mkMig())).toBe(false);
  });

  test("validateMetadata requires all required fields", () => {
    const bad: any = { version: 1, name: "test" };
    const result = MigrationPolicy.validateMetadata(bad);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("Migration author is required");
    expect(result.errors).toContain("Migration authoredAt date is required");
    expect(result.errors).toContain("Migration up function is required");
  });

  test("validateMetadata returns a deterministic result for null input", () => {
    const result = MigrationPolicy.validateMetadata(null as any);
    expect(result.valid).toBe(false);
    expect(result.errors).toEqual(["Migration definition is required"]);
  });

  test("validateMetadata validates hotfix requirements", () => {
    const mig = mkMig(
      mkMig({
        name: "hotfix_test",
        meta: { hotfix: true, reason: "test", rollback_risk: "low" },
        // Missing down function
      }),
    );
    const result = MigrationPolicy.validateMetadata(mig);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("Hotfix migrations must include a down function");
  });

  test("validateMetadata accepts a well-formed hotfix", () => {
    const mig = mkMig({
      meta: { hotfix: true, reason: "test", rollback_risk: "low" },
      down: async () => {},
    });
    expect(MigrationPolicy.validateMetadata(mig).valid).toBe(true);
  });

  describe("dryRun", () => {
    test("accepts a valid list of migrations", async () => {
      const result = await MigrationPolicy.dryRun([mkMig(), mkMig({ version: 2, name: "two" })]);
      expect(result.valid).toBe(true);
      expect(result.errors).toEqual([]);
    });

    test("reports duplicate versions deterministically", async () => {
      const result = await MigrationPolicy.dryRun([mkMig(), mkMig()]);
      expect(result.valid).toBe(false);
      expect(result.errors.filter((e) => e.includes("Duplicate migration version")).length).toBe(1);
    });

    test("reports metadata errors with migration label", async () => {
      const result = await MigrationPolicy.dryRun([mkMig({ author: "" })]);
      expect(result.valid).toBe(false);
      expect(result.errors.some((e) => e.startsWith("1_test: "))).toBe(true);
    });

    test("rejects non-array input", async () => {
      const result = await MigrationPolicy.dryRun(null as any);
      expect(result.valid).toBe(false);
      expect(result.errors).toEqual(["Migrations must be an array"]);
    });

    test("flags non-numeric versions", async () => {
      const result = await MigrationPolicy.dryRun([mkMig({ version: "two" as any })]);
      expect(result.valid).toBe(false);
      expect(result.errors.some((e) => e.includes("Migration version is required"))).toBe(true);
    });
  });

  describe("migrateCommand failure boundaries", () => {
    test("refuses down without emergency flag", async () => {
      const result = await migrateCommand({ allowDown: true });
      expect(result.success).toBe(false);
      expect(result.code).toBe("DOWN_REQUIRES_EMERGENCY");
    });

    test("refuses down when globally disabled", async () => {
      const result = await migrateCommand({ allowDown: true, emergency: true });
      expect(result.success).toBe(false);
      expect(result.code).toBe("DOWN_GLOBALLY_DISABLED");
    });
  });

  describe("migrateDownCommand failure boundaries", () => {
    test("rejects when down not allowed", async () => {
      const result = await migrateDownCommand({ emergency: false });
      expect(result.success).toBe(false);
      expect(result.code).toBe("DOWN_NOT_ALLOWED_");
    });

    test("rejects conflicting --to and --all flags", async () => {
      const result = await migrateDownCommand({ emergency: true, to: "1", all: true });
      expect(result.success).toBe(false);
      expect(result.code).toBe("CONFLICTING_FLAGS");
    });
  });
});
