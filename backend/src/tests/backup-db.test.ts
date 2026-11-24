import Database from "better-sqlite3";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createBackup, verifyBackup } from "../../scripts/backup-db";

describe("SQLite database backup", () => {
  let temporaryDirectory: string;
  let databasePath: string;
  let backupDirectory: string;

  beforeEach(() => {
    temporaryDirectory = fs.mkdtempSync(
      path.join(os.tmpdir(), "quicklendx-backup-test-"),
    );
    databasePath = path.join(temporaryDirectory, "service.db");
    backupDirectory = path.join(temporaryDirectory, "backups");
  });

  afterEach(() => {
    fs.rmSync(temporaryDirectory, { recursive: true, force: true });
  });

  test("backs up a live WAL database and verifies its key table counts", async () => {
    const database = new Database(databasePath);
    database.pragma("journal_mode = WAL");
    database.exec(
      "CREATE TABLE invoices (id TEXT PRIMARY KEY); INSERT INTO invoices VALUES ('invoice-1'), ('invoice-2')",
    );

    try {
      const result = await createBackup({
        databasePath,
        backupDirectory,
      });

      expect(fs.existsSync(result.backupPath)).toBe(true);
      expect(result.rowCounts).toEqual({ invoices: 2 });
      verifyBackup(result.backupPath, result.rowCounts);
    } finally {
      database.close();
    }
  });

  test("rejects a corrupt backup copy", () => {
    const corruptBackupPath = path.join(backupDirectory, "corrupt.sqlite");
    fs.mkdirSync(backupDirectory);
    fs.writeFileSync(corruptBackupPath, "not a SQLite database");

    expect(() => verifyBackup(corruptBackupPath, {})).toThrow();
  });

  test("prunes older backups to the configured retention count", async () => {
    const database = new Database(databasePath);
    database.exec("CREATE TABLE invoices (id TEXT PRIMARY KEY)");

    try {
      for (let day = 1; day <= 3; day++) {
        await createBackup({
          databasePath,
          backupDirectory,
          retentionCount: 2,
          now: new Date(`2026-01-0${day}T00:00:00.000Z`),
        });
      }

      const retainedBackups = fs.readdirSync(backupDirectory);
      expect(retainedBackups).toHaveLength(2);
      expect(retainedBackups[0]).toContain("20260102T000000000Z");
      expect(retainedBackups[1]).toContain("20260103T000000000Z");
    } finally {
      database.close();
    }
  });
});
