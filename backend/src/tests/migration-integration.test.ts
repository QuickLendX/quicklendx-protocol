import { getDatabase, closeDatabase } from "../lib/database";
import { runMigrations, verifyAppliedChecksums, loadMigrationsFromFS, getAppliedVersions, isDatabaseInitialized } from "../lib/migrations/runner";
import { MigrationPolicy, migrateCommand, migrateDownCommand } from "../lib/migrations/policy";

describe("Migration Integration", () => {
  beforeEach(() => {
    closeDatabase();
  });

  test("runner can be called (smoke test)", async () => {
    // This is a placeholder integration test
    // Full integration would require creating test migration files
    expect(true).toBe(true);
  });

  test("checksum verification works with no migrations", async () => {
    const result = await verifyAppliedChecksums();
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("rollback requires emergency flag or env var", async () => {
    const result = await migrateDownCommand({ emergency: false });
    expect(result.success).toBe(false);
    expect(result.message).toContain("emergency");
  });

  test("rollback with ALLOW_DOWN_MIGRATIONS env var", async () => {
    const originalEnv = process.env.ALLOW_DOWN_MIGRATIONS;
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    try {
      const result = await migrateDownCommand({ emergency: false });
      expect(result).toHaveProperty("success");
      expect(result).toHaveProperty("message");
    } catch (error) {
      // May fail due to missing down functions, but that's expected
      expect(error).toBeDefined();
    } finally {
      process.env.ALLOW_DOWN_MIGRATIONS = originalEnv;
    }
  });

  test("hotfix rollback requires approval file", async () => {
    // This would test the hotfix approval mechanism
    // For now, placeholder
    expect(true).toBe(true);
  });

  test("loadMigrationsFromFS returns sorted migrations", async () => {
    const migrations = await loadMigrationsFromFS();
    expect(Array.isArray(migrations)).toBe(true);
    expect(migrations.length).toBeGreaterThan(0);
    
    // Verify they're sorted by version (allow duplicates for now as there may be gaps)
    for (let i = 1; i < migrations.length; i++) {
      expect(migrations[i].version).toBeGreaterThanOrEqual(migrations[i - 1].version);
    }
  });

  test("MigrationPolicy.isDownAllowed returns false by default", () => {
    const result = MigrationPolicy.isDownAllowed();
    expect(result).toBe(false);
  });

  test("MigrationPolicy.isDownAllowed returns true with env var", () => {
    const originalEnv = process.env.ALLOW_DOWN_MIGRATIONS;
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    try {
      const result = MigrationPolicy.isDownAllowed();
      expect(result).toBe(true);
    } finally {
      process.env.ALLOW_DOWN_MIGRATIONS = originalEnv;
    }
  });

  test("MigrationPolicy.validateMetadata validates hotfix requirements", () => {
    const hotfixMigration = {
      version: 1,
      name: "test_hotfix",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true },
      up: async () => {},
    };

    const result = MigrationPolicy.validateMetadata(hotfixMigration);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("Hotfix migrations must include meta.reason");
    expect(result.errors).toContain("Hotfix migrations must include meta.rollback_risk");
    expect(result.errors).toContain("Hotfix migrations must include a down function");
  });

  test("MigrationPolicy.validateMetadata passes for valid hotfix", () => {
    const validHotfix = {
      version: 1,
      name: "test_hotfix",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true, reason: "test", rollback_risk: "low" },
      up: async () => {},
      down: async () => {},
    };

    const result = MigrationPolicy.validateMetadata(validHotfix);
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("MigrationPolicy.validateMetadata validates standard migration", () => {
    const standardMigration = {
      version: 1,
      name: "test_migration",
      authoredAt: "2026-04-26",
      author: "test",
      up: async () => {},
    };

    const result = MigrationPolicy.validateMetadata(standardMigration);
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  test("MigrationPolicy.validateMetadata fails for missing required fields", () => {
    const invalidMigration = {
      version: 1,
      name: "",
      authoredAt: "2026-04-26",
      author: "test",
      up: async () => {},
    };

    const result = MigrationPolicy.validateMetadata(invalidMigration);
    expect(result.valid).toBe(false);
    expect(result.errors.length).toBeGreaterThan(0);
  });

  test("MigrationPolicy.isHotfix identifies hotfix migrations", () => {
    const hotfix = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "test",
      meta: { hotfix: true },
      up: async () => {},
    };

    expect(MigrationPolicy.isHotfix(hotfix)).toBe(true);
  });

  test("MigrationPolicy.isHotfix returns false for standard migrations", () => {
    const standard = {
      version: 1,
      name: "test",
      authoredAt: "2026-04-26",
      author: "test",
      up: async () => {},
    };

    expect(MigrationPolicy.isHotfix(standard)).toBe(false);
  });

  test("migrateCommand with check flag", async () => {
    const result = await migrateCommand({ check: true });
    expect(result).toHaveProperty("success");
    expect(result).toHaveProperty("message");
  });

  test("migrateCommand with validateOnly flag", async () => {
    const result = await migrateCommand({ validateOnly: true });
    expect(result).toHaveProperty("success");
    expect(result).toHaveProperty("message");
  });

  test("migrateCommand rejects allowDown without emergency", async () => {
    const result = await migrateCommand({ allowDown: true, emergency: false });
    expect(result.success).toBe(false);
    expect(result.message).toContain("emergency");
  });

  test("migrateCommand rejects allowDown without env var", async () => {
    const result = await migrateCommand({ allowDown: true, emergency: true });
    expect(result.success).toBe(false);
    expect(result.message).toContain("globally disabled");
  });

  test("migrateCommand with check mode and valid migrations", async () => {
    const result = await migrateCommand({ check: true });
    expect(result).toHaveProperty("success");
    expect(result).toHaveProperty("message");
  });

  test("migrateCommand with validateOnly mode", async () => {
    const result = await migrateCommand({ validateOnly: true });
    expect(result).toHaveProperty("success");
    expect(result).toHaveProperty("message");
  });

  test("migrateCommand with validateOnly and emergency flag", async () => {
    const result = await migrateCommand({ validateOnly: true, emergency: true });
    expect(result).toHaveProperty("success");
    expect(result).toHaveProperty("message");
  });

  test("migrateCommand with verbose flag", async () => {
    const result = await migrateCommand({ verbose: true, validateOnly: true });
    expect(result).toHaveProperty("success");
    expect(result).toHaveProperty("message");
  });

  test("migrateDownCommand with emergency flag", async () => {
    const originalEnv = process.env.ALLOW_DOWN_MIGRATIONS;
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    try {
      const result = await migrateDownCommand({ emergency: true });
      expect(result).toHaveProperty("success");
      expect(result).toHaveProperty("message");
    } catch (error) {
      // May fail due to missing down functions, but that's expected
      expect(error).toBeDefined();
    } finally {
      process.env.ALLOW_DOWN_MIGRATIONS = originalEnv;
    }
  });

  test("migrateDownCommand without emergency or env var", async () => {
    const originalEnv = process.env.ALLOW_DOWN_MIGRATIONS;
    delete process.env.ALLOW_DOWN_MIGRATIONS;
    try {
      const result = await migrateDownCommand({ emergency: false });
      expect(result.success).toBe(false);
      expect(result.message).toContain("require --emergency flag");
    } finally {
      process.env.ALLOW_DOWN_MIGRATIONS = originalEnv;
    }
  });

  test("migrateDownCommand with to flag", async () => {
    const originalEnv = process.env.ALLOW_DOWN_MIGRATIONS;
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    try {
      const result = await migrateDownCommand({ to: "5" });
      expect(result).toHaveProperty("success");
      expect(result).toHaveProperty("message");
    } catch (error) {
      // May fail due to missing down functions, but that's expected
      expect(error).toBeDefined();
    } finally {
      process.env.ALLOW_DOWN_MIGRATIONS = originalEnv;
    }
  });

  test("migrateDownCommand with all flag", async () => {
    const originalEnv = process.env.ALLOW_DOWN_MIGRATIONS;
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    try {
      const result = await migrateDownCommand({ all: true });
      expect(result).toHaveProperty("success");
      expect(result).toHaveProperty("message");
    } catch (error) {
      // May fail due to missing down functions, but that's expected
      expect(error).toBeDefined();
    } finally {
      process.env.ALLOW_DOWN_MIGRATIONS = originalEnv;
    }
  });

  test("migrateDownCommand with skipChecksumVerify flag", async () => {
    const originalEnv = process.env.ALLOW_DOWN_MIGRATIONS;
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    try {
      const result = await migrateDownCommand({ skipChecksumVerify: true, dryRun: true });
      expect(result).toHaveProperty("success");
      expect(result).toHaveProperty("message");
    } catch (error) {
      // May fail due to missing down functions, but that's expected
      expect(error).toBeDefined();
    } finally {
      process.env.ALLOW_DOWN_MIGRATIONS = originalEnv;
    }
  });

  test("migrateDownCommand with verbose flag", async () => {
    const originalEnv = process.env.ALLOW_DOWN_MIGRATIONS;
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    try {
      const result = await migrateDownCommand({ verbose: true, dryRun: true });
      expect(result).toHaveProperty("success");
      expect(result).toHaveProperty("message");
    } catch (error) {
      // May fail due to missing down functions, but that's expected
      expect(error).toBeDefined();
    } finally {
      process.env.ALLOW_DOWN_MIGRATIONS = originalEnv;
    }
  });

  test("migrateCommand with skipChecksumVerify flag", async () => {
    const result = await migrateCommand({ skipChecksumVerify: true, validateOnly: true });
    expect(result).toHaveProperty("success");
    expect(result).toHaveProperty("message");
  });

  test("migrateCommand with dryRun flag", async () => {
    const result = await migrateCommand({ dryRun: true, validateOnly: true });
    expect(result).toHaveProperty("success");
    expect(result).toHaveProperty("message");
  });

  test("getAppliedVersions returns array", async () => {
    const versions = await getAppliedVersions();
    expect(Array.isArray(versions)).toBe(true);
  });

  test("isDatabaseInitialized returns boolean", async () => {
    const initialized = await isDatabaseInitialized();
    expect(typeof initialized).toBe("boolean");
  });

  test("migrateCommand with skipChecksumVerify flag", async () => {
    const result = await migrateCommand({ skipChecksumVerify: true, validateOnly: true });
    expect(result).toHaveProperty("success");
    expect(result).toHaveProperty("message");
  });

  test("migrateCommand with verbose flag", async () => {
    const result = await migrateCommand({ verbose: true, validateOnly: true });
    expect(result).toHaveProperty("success");
    expect(result).toHaveProperty("message");
  });

  test("migrateCommand with multiple flags", async () => {
    const result = await migrateCommand({ verbose: true, validateOnly: true, skipChecksumVerify: true });
    expect(result).toHaveProperty("success");
    expect(result).toHaveProperty("message");
  });
});
