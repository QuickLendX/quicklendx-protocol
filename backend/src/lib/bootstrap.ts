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
import { constants } from "os";
import * as path from "path";

const DATA_DIR = path.resolve(process.cwd(), ".data");
const HOTFIX_DIR = path.resolve(process.cwd(), ".hotfix-approvals");

function errorCode(error: unknown): string {
  const code =
    error && typeof error === "object" && "code" in error
      ? error.code
      : undefined;
  // Log only known errno names, never raw messages, paths, or arbitrary payloads.
  return typeof code === "string" &&
    Object.prototype.hasOwnProperty.call(constants.errno, code)
    ? code
    : "UNKNOWN";
}

async function ensureDirectory(
  directory: string,
  label: string,
): Promise<void> {
  try {
    try {
      await fs.mkdir(directory, { recursive: true });
    } catch (error: unknown) {
      // A competing creator is harmless only if the path is now a directory.
      // A blocking file must be diagnosed and never removed or overwritten.
      if (
        errorCode(error) !== "EEXIST" ||
        !(await fs.stat(directory)).isDirectory()
      ) {
        throw error;
      }
    }
    console.log(`📁 Ensured ${label}/ directory exists`);
  } catch (error: unknown) {
    console.warn(`⚠️  Could not create ${label}/:`, errorCode(error));
  }
}

export async function ensureRuntimeDirs(): Promise<void> {
  // Preserve best-effort Promise<void> behavior: attempt each directory even if
  // the other fails. No rollback, chmod, or cached failures: retries and concurrent
  // calls remain idempotent without deleting database or approval contents.
  await ensureDirectory(DATA_DIR, ".data");
  await ensureDirectory(HOTFIX_DIR, ".hotfix-approvals");
}
