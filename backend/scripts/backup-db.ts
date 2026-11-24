import Database from "better-sqlite3";
import fs from "node:fs";
import path from "node:path";
import { config } from "../src/config";

const KEY_TABLES = [
  "invoices",
  "bids",
  "settlements",
  "raw_events",
  "backend_audit_log",
  "api_keys",
  "webhook_deliveries",
  "webhook_subscriptions",
  "indexer_state",
] as const;

const BACKUP_SUFFIX = ".sqlite";

type SqliteDatabase = InstanceType<typeof Database>;

export interface BackupOptions {
  databasePath?: string;
  backupDirectory?: string;
  retentionCount?: number;
  now?: Date;
}

export interface BackupResult {
  backupPath: string;
  rowCounts: Record<string, number>;
}

function getTableRowCounts(db: SqliteDatabase): Record<string, number> {
  const existingTables = new Set(
    db
      .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
      .all()
      .map((row) => (row as { name: string }).name),
  );
  const rowCounts: Record<string, number> = {};

  for (const table of KEY_TABLES) {
    if (existingTables.has(table)) {
      const result = db
        .prepare(`SELECT COUNT(*) AS count FROM "${table}"`)
        .get() as { count: number };
      rowCounts[table] = Number(result.count);
    }
  }

  return rowCounts;
}

export function verifyBackup(
  backupPath: string,
  expectedRowCounts: Record<string, number>,
): void {
  const backup = new Database(backupPath, {
    readonly: true,
    fileMustExist: true,
  });

  try {
    const integrityResults = backup
      .prepare("PRAGMA integrity_check")
      .all() as Array<{ integrity_check: string }>;
    if (
      integrityResults.length === 0 ||
      integrityResults.some((result) => result.integrity_check !== "ok")
    ) {
      throw new Error(
        `Backup verification failed: integrity_check reported ${integrityResults
          .map((result) => result.integrity_check)
          .join("; ") || "no result"}`,
      );
    }

    const actualRowCounts = getTableRowCounts(backup);
    for (const [table, expectedCount] of Object.entries(expectedRowCounts)) {
      const actualCount = actualRowCounts[table];
      if (actualCount !== expectedCount) {
        throw new Error(
          `Backup verification failed: row count mismatch for ${table} (expected ${expectedCount}, found ${actualCount ?? "missing table"})`,
        );
      }
    }
  } finally {
    backup.close();
  }
}

function pruneBackups(
  backupDirectory: string,
  backupPrefix: string,
  retentionCount: number,
): void {
  const backups = fs
    .readdirSync(backupDirectory)
    .filter(
      (file) =>
        file.startsWith(backupPrefix) && file.endsWith(BACKUP_SUFFIX),
    )
    .sort()
    .reverse();

  for (const oldBackup of backups.slice(retentionCount)) {
    fs.unlinkSync(path.join(backupDirectory, oldBackup));
  }
}

export async function createBackup(
  options: BackupOptions = {},
): Promise<BackupResult> {
  const databasePath = options.databasePath ?? config.DATABASE_PATH;
  const backupDirectory =
    options.backupDirectory ?? path.join(path.dirname(databasePath), "backups");
  const retentionCount = options.retentionCount ?? 7;

  if (!Number.isInteger(retentionCount) || retentionCount < 1) {
    throw new Error("BACKUP_RETENTION must be a positive integer");
  }

  fs.mkdirSync(backupDirectory, { recursive: true });

  const source = new Database(databasePath, {
    readonly: true,
    fileMustExist: true,
  });
  try {
    const rowCounts = getTableRowCounts(source);
    const backupPrefix = `${path.basename(databasePath)}-`;
    const timestamp = (options.now ?? new Date())
      .toISOString()
      .replace(/[-:]/g, "")
      .replace(".", "");
    const backupPath = path.join(
      backupDirectory,
      `${backupPrefix}${timestamp}${BACKUP_SUFFIX}`,
    );

    await source.backup(backupPath);
    verifyBackup(backupPath, rowCounts);
    pruneBackups(backupDirectory, backupPrefix, retentionCount);

    return { backupPath, rowCounts };
  } finally {
    source.close();
  }
}

function parseRetention(value: string | undefined): number {
  if (value === undefined) {
    return 7;
  }
  if (!/^[1-9]\d*$/.test(value)) {
    throw new Error("BACKUP_RETENTION must be a positive integer");
  }
  return Number(value);
}

export async function main(): Promise<void> {
  try {
    const { backupPath } = await createBackup({
      retentionCount: parseRetention(process.env.BACKUP_RETENTION),
    });
    console.log(`Database backup created and verified: ${backupPath}`);
  } catch (error) {
    console.error(
      "Database backup failed:",
      error instanceof Error ? error.message : String(error),
    );
    process.exitCode = 1;
  }
}

if (
  process.argv[1] &&
  ["backup-db.ts", "backup-db.js"].includes(path.basename(process.argv[1]))
) {
  void main();
}
