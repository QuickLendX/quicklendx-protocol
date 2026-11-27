import { createHash } from "crypto";
import * as fs from "fs/promises";
import * as os from "os";
import * as path from "path";
import {
  computeChecksum,
  loadMigrationsFromFS,
  runMigrations,
  verifyAppliedChecksums,
  type DatabaseClient,
} from "../lib/migrations/runner";

let migrationsDir: string;
let originalMigrationsDir: string | undefined;

async function writeMigration(
  filename: string,
  source: string
): Promise<string> {
  const filePath = path.join(migrationsDir, filename);
  await fs.writeFile(filePath, source, "utf8");
  return filePath;
}

function migrationSource(
  version: number,
  name: string,
  upBody = "async () => {}"
): string {
  return `module.exports = {
    version: ${version},
    name: "${name}",
    authoredAt: "2026-01-01",
    author: "test",
    up: ${upBody}
  };`;
}

function makeDatabase() {
  const statements: string[] = [];
  const exec = jest.fn((sql: string) => {
    statements.push(sql);
  });
  const prepare = jest.fn((sql: string) => ({
    all: jest.fn(() => []),
    get: jest.fn(() => undefined),
    run: jest.fn(() => {
      statements.push(sql);
      return { lastInsertRowId: 1, changes: 1 };
    }),
  }));
  const db: DatabaseClient = {
    exec,
    prepare,
    transaction: jest.fn((callback: () => void) => callback()),
  };
  return { db, exec, prepare, statements };
}

beforeEach(async () => {
  originalMigrationsDir = process.env.MIGRATIONS_DIR;
  migrationsDir = await fs.mkdtemp(path.join(os.tmpdir(), "quicklendx-migrations-"));
  process.env.MIGRATIONS_DIR = migrationsDir;
});

afterEach(async () => {
  if (originalMigrationsDir === undefined) {
    delete process.env.MIGRATIONS_DIR;
  } else {
    process.env.MIGRATIONS_DIR = originalMigrationsDir;
  }
  await fs.rm(migrationsDir, { recursive: true, force: true });
});

describe("Migration runner release-gate cases", () => {
  it("loads migrations in ascending numeric version order", async () => {
    await writeMigration("002_second.ts", migrationSource(2, "second"));
    await writeMigration("v001_first.ts", migrationSource(1, "first"));

    const migrations = await loadMigrationsFromFS(migrationsDir);

    expect(migrations.map(({ version }) => version)).toEqual([1, 2]);
    expect(migrations.map(({ name }) => name)).toEqual(["first", "second"]);
  });

  it("detects checksum drift while accepting the current migration checksum", async () => {
    const migrationFile = await writeMigration(
      "v001_initial.ts",
      migrationSource(1, "initial")
    );
    const checksum = computeChecksum(await fs.readFile(migrationFile, "utf8"));
    const appliedRows = [{ version: 1, name: "initial", checksum }];

    await expect(
      verifyAppliedChecksums(undefined, { migrationsDir, appliedRows })
    ).resolves.toEqual({ valid: true, errors: [] });

    const drifted = await verifyAppliedChecksums(undefined, {
      migrationsDir,
      appliedRows: [{ ...appliedRows[0], checksum: createHash("sha256").update("old migration").digest("hex") }],
    });
    expect(drifted.valid).toBe(false);
    expect(drifted.errors).toContain("Checksum mismatch for 1_initial");
  });

  it("rolls back migration changes and does not record a failed migration", async () => {
    await writeMigration(
      "v001_failing.ts",
      migrationSource(
        1,
        "failing",
        `async (ctx) => {
          await ctx.db.run("INSERT INTO migration_probe (value) VALUES (?)", ["pending"]);
          throw new Error("migration failed");
        }`
      )
    );
    const { db, exec, statements } = makeDatabase();

    await expect(runMigrations({ db, skipChecksumVerify: true })).rejects.toThrow(
      "migration failed"
    );

    expect(exec).toHaveBeenCalledWith("BEGIN IMMEDIATE");
    expect(exec).toHaveBeenCalledWith("ROLLBACK");
    expect(exec).not.toHaveBeenCalledWith("COMMIT");
    expect(statements).toContain("INSERT INTO migration_probe (value) VALUES (?)");
    expect(
      statements.some((sql) => sql.includes("INSERT INTO _migrations"))
    ).toBe(false);
  });
});
