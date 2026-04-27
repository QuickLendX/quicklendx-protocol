/**
 * Bootstrapper — ensures runtime directories exist before starting server.
 *
 * In development:
 *   - Creates .data/ directory for SQLite database
 *   - Creates .hotfix-approvals/ directory (empty; approvals added manually in prod)
 *
 * This prevents "ENOENT" errors on first run.
 */

import * as fs from "fs/promises";
import * as path from "path";

const DATA_DIR = path.resolve(process.cwd(), ".data");
const HOTFIX_DIR = path.resolve(process.cwd(), ".hotfix-approvals");

export async function ensureRuntimeDirs(): Promise<void> {
  try {
    await fs.mkdir(DATA_DIR, { recursive: true });
    console.log(`📁 Ensured .data/ directory exists`);
  } catch (err: any) {
    if (err.code !== "EEXIST") {
      console.warn("⚠️  Could not create .data/: ", err.message);
    }
  }

  try {
    await fs.mkdir(HOTFIX_DIR, { recursive: true });
    console.log(`📁 Ensured .hotfix-approvals/ directory exists`);
  } catch (err: any) {
    if (err.code !== "EEXIST") {
      console.warn("⚠️  Could not create .hotfix-approvals/: ", err.message);
    }
  }
}
