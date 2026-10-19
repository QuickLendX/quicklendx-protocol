/**
 * migrations-directory.test.ts
 *
 * Static invariant checks on the src/migrations/ directory.
 *
 * Scope of this file:
 *   1. Every default.version is unique.
 *   2. default.version equals the numeric filename prefix.
 *   3. The versions sorted ascending have no gaps.
 *   4. No two files share the same `name` field.
 *
 * Design note: these tests import migration modules directly and reproduce
 * the runner's parseMigrationFilename() regex locally so they do NOT depend
 * on runMigrations() or any other runner internals.
 */

import { describe, it, expect } from "vitest";
import * as path from "path";
import * as fs from "fs";
import { pathToFileURL } from "url";

// ── Filename grammar (mirrored from runner.ts parseMigrationFilename) ───────
const MIGRATION_FILENAME_RE = /^v?(\d{3})_([a-z0-9_]+)\.ts$/;

function parseFilename(filename: string): { version: number; slug: string } | null {
  const match = filename.match(MIGRATION_FILENAME_RE);
  if (!match) return null;
  return { version: parseInt(match[1], 10), slug: match[2] };
}

// ── Helpers ──────────────────────────────────────────────────────────────────

const MIGRATIONS_DIR = path.resolve(__dirname, "../migrations");

interface MigrationMeta {
  file: string;
  filenameVersion: number;
  slug: string;
  exportedVersion: number;
  exportedName: string;
}

/**
 * Loads all migration files from src/migrations/ and returns a metadata array.
 * Uses require() so this works under ts-node/vitest without a separate build step.
 */
function loadMigrationMeta(): MigrationMeta[] {
  const files = fs.readdirSync(MIGRATIONS_DIR).sort();
  const results: MigrationMeta[] = [];

  for (const file of files) {
    const parsed = parseFilename(file);
    if (!parsed) continue; // skip non-migration files

    const fullPath = path.join(MIGRATIONS_DIR, file);
    // eslint-disable-next-line @typescript-eslint/no-var-requires
    const mod = require(fullPath);
    const def = mod && (mod.default ?? mod);

    results.push({
      file,
      filenameVersion: parsed.version,
      slug: parsed.slug,
      exportedVersion: def?.version,
      exportedName: def?.name,
    });
  }

  return results;
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("migrations directory invariants", () => {
  let metas: MigrationMeta[];

  beforeAll(() => {
    metas = loadMigrationMeta();
  });

  it("finds at least one migration file", () => {
    expect(metas.length).toBeGreaterThan(0);
  });

  it("every default.version is a number", () => {
    for (const m of metas) {
      expect(typeof m.exportedVersion, `${m.file} — version must be a number`).toBe("number");
    }
  });

  it("every default.version is unique", () => {
    const seen = new Map<number, string>();
    for (const m of metas) {
      const prior = seen.get(m.exportedVersion);
      expect(prior, `Duplicate version ${m.exportedVersion}: ${prior} and ${m.file}`).toBeUndefined();
      seen.set(m.exportedVersion, m.file);
    }
  });

  it("default.version equals the numeric filename prefix", () => {
    for (const m of metas) {
      expect(m.exportedVersion, `${m.file} — export version ${m.exportedVersion} ≠ filename version ${m.filenameVersion}`)
        .toBe(m.filenameVersion);
    }
  });

  it("versions sorted ascending have no gaps", () => {
    const versions = metas
      .map((m) => m.exportedVersion)
      .sort((a, b) => a - b);

    for (let i = 1; i < versions.length; i++) {
      expect(
        versions[i],
        `Gap between version ${versions[i - 1]} and ${versions[i]}`
      ).toBe(versions[i - 1] + 1);
    }
  });

  it("no two files share the same name field", () => {
    const seen = new Map<string, string>();
    for (const m of metas) {
      const prior = seen.get(m.exportedName);
      expect(prior, `Duplicate name "${m.exportedName}": ${prior} and ${m.file}`).toBeUndefined();
      seen.set(m.exportedName, m.file);
    }
  });
});
