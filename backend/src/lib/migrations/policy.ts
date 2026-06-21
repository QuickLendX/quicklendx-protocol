import { runMigrations, loadMigrationsFromFS, getAppliedVersions, validateMigrationFiles } from "./runner";
import type { MigrationDefinition } from "./types";

interface MigrateArgs {
  dryRun?: boolean;
  allowDown?: boolean;
  emergency?: boolean;
  verbose?: boolean;
  validateOnly?: boolean;
  check?: boolean;
  to?: string;
  all?: boolean;
  skipChecksumVerify?: boolean;
}

export class MigrationPolicy {
  static isDownAllowed(): boolean {
    return process.env.ALLOW_DOWN_MIGRATIONS === "true";
  }

  static isHotfix(migration: MigrationDefinition): boolean {
    return migration.meta?.hotfix === true;
  }

  static validateMetadata(migration: MigrationDefinition): { valid: boolean; errors: string[] } {
    const errors: string[] = [];

    if (!migration.name) errors.push("Migration name is required");
    if (!migration.author) errors.push("Migration author is required");
    if (!migration.authoredAt) errors.push("Migration authoredAt date is required");
    if (!migration.up) errors.push("Migration up function is required");

    if (this.isHotfix(migration)) {
      if (!migration.meta?.reason) errors.push("Hotfix migrations must include meta.reason");
      if (!migration.meta?.rollback_risk) errors.push("Hotfix migrations must include meta.rollback_risk");
      if (!migration.down) errors.push("Hotfix migrations must include a down function");
    }

    return { valid: errors.length === 0, errors };
  }

  static async dryRun(migrations: MigrationDefinition[], options: { force?: boolean } = {}): Promise<{
    valid: boolean;
    errors: string[];
    warnings: string[];
  }> {
    const errors: string[] = [];
    const warnings: string[] = [];

    for (const mig of migrations) {
      const metaCheck = this.validateMetadata(mig);
      if (!metaCheck.valid) {
        errors.push(...metaCheck.errors.map((e) => `${mig.version}_${mig.name}: ${e}`));
      }
    }

    return { valid: errors.length === 0, errors, warnings };
  }
}

export async function migrateCommand(args: Record<string, unknown>): Promise<{
  success: boolean;
  message: string;
  applied?: number;
  skipped?: number;
}> {
  const {
    dryRun = false,
    allowDown = false,
    emergency = false,
    verbose = false,
    validateOnly = false,
    check = false,
    skipChecksumVerify = false,
  } = args as MigrateArgs;

  if (check) {
    const fileValid = await validateMigrationFiles();
    const fileMigs = await loadMigrationsFromFS();
    const appliedVersions = await getAppliedVersions();
    const missing = fileMigs.filter((m) => !appliedVersions.includes(m.version));
    const valid = fileValid.valid && missing.length === 0;
    const errors = [
      ...fileValid.errors,
      ...missing.map((m) => `Migration ${m.version}_${m.name} is not applied`),
    ];

    if (!valid) {
      console.error("❌ Migration check failed:");
      errors.forEach((e) => console.error(`   ${e}`));
      return { success: false, message: "Migrations out of sync or invalid" };
    }
    console.log("✅ Migrations are in sync");
    return { success: true, message: "Migrations valid" };
  }

  if (validateOnly) {
    const result = await MigrationPolicy.dryRun([], { force: emergency });
    if (!result.valid) {
      console.error("❌ Migration validation failed:");
      result.errors.forEach((e) => console.error(`   ${e}`));
      return { success: false, message: "Validation errors" };
    }
    console.log("✅ All migration files are valid");
    return { success: true, message: "Validation passed" };
  }

  if (allowDown && !emergency) {
    return {
      success: false,
      message: "Refusing to run down migrations without --emergency flag. This is a safety guard.",
    };
  }

  if (allowDown && !MigrationPolicy.isDownAllowed()) {
    return {
      success: false,
      message: "Down migrations are globally disabled (ALLOW_DOWN_MIGRATIONS not set).",
    };
  }

  try {
    const result = await runMigrations({ dryRun, allowDown, verbose, skipChecksumVerify });
    console.log(`\n✅ Migration run complete in ${result.durationMs}ms`);
    console.log(`   Applied: ${result.applied.length}, Skipped: ${result.skipped}`);

    if (dryRun && result.applied.length > 0) {
      console.log("\n[DRY-RUN] The following migrations would be applied:");
      result.applied.forEach((m) => console.log(`   ${m.version} ${m.name} by ${m.author}`));
    }

    return {
      success: true,
      message: `Applied ${result.applied.length} migrations`,
      applied: result.applied.length,
      skipped: result.skipped,
    };
  } catch (err: any) {
    console.error("❌ Migration failed:", err.message);
    return { success: false, message: `Migration error: ${err.message}` };
  }
}

export async function migrateDownCommand(args: Record<string, unknown>): Promise<{
  success: boolean;
  message: string;
  applied?: number;
  skipped?: number;
}> {
  const {
    dryRun = false,
    emergency = false,
    verbose = false,
    to,
    all = false,
    skipChecksumVerify = false,
  } = args as MigrateArgs;

  if (!emergency && !MigrationPolicy.isDownAllowed()) {
    return {
      success: false,
      message: "Down migrations require --emergency flag or ALLOW_DOWN_MIGRATIONS=true environment variable.",
    };
  }

  if (to && all) {
    return {
      success: false,
      message: "Cannot specify both --to and --all flags.",
    };
  }

  try {
    const result = await runMigrations({ dryRun, allowDown: true, verbose, skipChecksumVerify });
    console.log(`\n✅ Migration rollback complete in ${result.durationMs}ms`);
    console.log(`   Rolled back: ${result.applied.length}, Skipped: ${result.skipped}`);

    if (dryRun && result.applied.length > 0) {
      console.log("\n[DRY-RUN] The following migrations would be rolled back:");
      result.applied.forEach((m) => console.log(`   ${m.version} ${m.name} by ${m.author}`));
    }

    return {
      success: true,
      message: `Rolled back ${result.applied.length} migrations`,
      applied: result.applied.length,
      skipped: result.skipped,
    };
  } catch (err: any) {
    console.error("❌ Migration rollback failed:", err.message);
    return { success: false, message: `Rollback error: ${err.message}` };
  }
}
