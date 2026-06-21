import { MigrationPolicy } from "../lib/migrations/policy";
import type { MigrationDefinition } from "../lib/migrations/types";

describe("MigrationPolicy", () => {
  beforeEach(() => {
    process.env.NODE_ENV = "development";
    process.env.ALLOW_DOWN_MIGRATIONS = "false";
  });

  test("isDownAllowed returns false by default", () => {
    expect(MigrationPolicy.isDownAllowed()).toBe(false);
  });

  test("isHotfix returns true when meta.hotfix is set", () => {
    const mig: MigrationDefinition = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "alice",
      up: async () => {},
      meta: { hotfix: true },
    };
    expect(MigrationPolicy.isHotfix(mig)).toBe(true);
  });

  test("validateMetadata requires all required fields", () => {
    const bad: any = { version: 1, name: "test" };
    const result = MigrationPolicy.validateMetadata(bad);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("Migration author is required");
    expect(result.errors).toContain("Migration authoredAt date is required");
    expect(result.errors).toContain("Migration up function is required");
  });

  test("validateMetadata validates hotfix requirements", () => {
    const mig: MigrationDefinition = {
      version: 1,
      name: "hotfix_test",
      authoredAt: "2026-04-26",
      author: "alice",
      meta: { hotfix: true, reason: "test", rollback_risk: "low" },
      up: async () => {},
      // Missing down function
    };
    const result = MigrationPolicy.validateMetadata(mig);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("Hotfix migrations must include a down function");
  });
});
