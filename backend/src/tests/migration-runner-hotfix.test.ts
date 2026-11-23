/**
 * Deterministic failure-boundary coverage for isHotfixApproved
 * (src/lib/migrations/runner.ts).
 *
 * The contract these tests pin down is the H1..H6 block documented above
 * isHotfixApproved. isHotfixApproved is the authorization gate that stops an
 * unapproved hotfix migration from being applied in production, so every
 * branch must be deterministic: it either produces a trustworthy verdict or
 * reports the fault, and it never guesses "not approved" for a reason the
 * operator cannot see.
 *
 * Filesystem behaviour is controlled two ways so the suite is deterministic
 * without going blind to the real implementation:
 *   - the default is the *real* fs/promises, exercised against fixtures in a
 *     temp approvals directory;
 *   - specific errno cases are injected by overriding `stat` for one call.
 */

import * as fs from "fs/promises";
import * as path from "path";
import { isHotfixApproved, runMigrations } from "../lib/migrations/runner";
import type { ParsedMigration } from "../lib/migrations/types";

jest.mock("../config", () => ({
  config: { NODE_ENV: "production" },
}));

jest.mock("../lib/database", () => ({
  getDatabase: jest.fn(),
  closeDatabase: jest.fn(),
}));

jest.mock("fs/promises", () => {
  const actual = jest.requireActual<typeof import("fs/promises")>("fs/promises");
  return { ...actual, readdir: jest.fn(), stat: jest.fn() };
});

const realFs = jest.requireActual<typeof import("fs/promises")>("fs/promises");
const mockedReaddir = fs.readdir as jest.MockedFunction<typeof fs.readdir>;
const mockedStat = fs.stat as unknown as jest.Mock;

/** Directory isHotfixApproved resolves at import time, from process.cwd(). */
const APPROVALS_DIR = path.resolve(process.cwd(), ".hotfix-approvals");
const HOTFIX_FILE = "v003_hotfix_add_invoice_id_to_backfill_audit.ts";
const HOTFIX_VERSION = 3;
const HOTFIX_NAME = "hotfix_add_invoice_id_to_backfill_audit";

let approvalsDirCreatedByUs = false;

function makeMigration(overrides: Partial<ParsedMigration> = {}): ParsedMigration {
  return {
    file: "v001_example.ts",
    version: 1,
    name: "example",
    content: {
      version: 1,
      name: "example",
      authoredAt: "2026-04-26",
      author: "test",
      up: async () => {},
    },
    ...overrides,
  };
}

function makeHotfix(overrides: Partial<ParsedMigration> = {}): ParsedMigration {
  const migration = makeMigration(overrides);
  return {
    ...migration,
    content: {
      ...migration.content,
      meta: { ...(migration.content.meta || {}), hotfix: true },
    },
  };
}

function fsError(code: string): NodeJS.ErrnoException {
  const error = new Error(`${code}: simulated failure`) as NodeJS.ErrnoException;
  error.code = code;
  return error;
}

function approvalPath(version: number, name: string): string {
  return path.join(APPROVALS_DIR, `${version}_${name}.approval`);
}

beforeEach(async () => {
  mockedStat.mockImplementation(realFs.stat);
  mockedReaddir.mockReset();

  if (!(await realFs.stat(APPROVALS_DIR).catch(() => null))) {
    await realFs.mkdir(APPROVALS_DIR, { recursive: true });
    approvalsDirCreatedByUs = true;
  }
});

afterEach(async () => {
  mockedStat.mockReset();
  if (approvalsDirCreatedByUs) {
    await realFs.rm(APPROVALS_DIR, { recursive: true, force: true });
    approvalsDirCreatedByUs = false;
  }
});

afterAll(async () => {
  if (approvalsDirCreatedByUs) {
    await realFs.rm(APPROVALS_DIR, { recursive: true, force: true });
  }
});

describe("isHotfixApproved: hotfix detection short-circuits (H1)", () => {
  it("approves a migration that is not a hotfix without touching the filesystem", async () => {
    const cases: Array<[string, Record<string, unknown> | undefined]> = [
      ["meta absent", undefined],
      ["empty meta", {}],
      ["hotfix false", { hotfix: false }],
      ["hotfix 0", { hotfix: 0 }],
      ["hotfix empty string", { hotfix: "" }],
      ["hotfix null", { hotfix: null }],
      ["hotfix undefined", { hotfix: undefined }],
      ["hotfix NaN", { hotfix: Number.NaN }],
    ];

    for (const [, meta] of cases) {
      const migration = makeMigration({
        content: { ...makeMigration().content, meta },
      });

      await expect(isHotfixApproved(migration)).resolves.toBe(true);
    }

    expect(mockedStat).not.toHaveBeenCalled();
  });

  it("treats any truthy meta.hotfix as a hotfix and consults the filesystem", async () => {
    // Fails closed: a sloppy truthy value such as the string "false" still
    // requires an approval artifact rather than silently skipping the gate.
    const truthyValues: unknown[] = [true, "false", "no", 1, [], {}];

    for (const hotfix of truthyValues) {
      const migration = makeHotfix({
        content: { ...makeMigration().content, meta: { hotfix } },
      });

      await expect(isHotfixApproved(migration)).resolves.toBe(false);
    }

    expect(mockedStat).toHaveBeenCalledTimes(truthyValues.length);
  });
});

describe("isHotfixApproved: approval artifact requirements (H2)", () => {
  it("approves a hotfix backed by a regular file", async () => {
    const migration = makeHotfix();
    await realFs.writeFile(approvalPath(migration.version, migration.name), "approved");

    await expect(isHotfixApproved(migration)).resolves.toBe(true);
  });

  it("approves an empty approval file, since presence is the approval", async () => {
    const migration = makeHotfix();
    await realFs.writeFile(approvalPath(migration.version, migration.name), "");

    await expect(isHotfixApproved(migration)).resolves.toBe(true);
  });

  it("rejects a directory sitting where the approval file should be", async () => {
    const migration = makeHotfix();
    await realFs.mkdir(approvalPath(migration.version, migration.name), { recursive: true });

    await expect(isHotfixApproved(migration)).rejects.toThrow(/is not a regular file/);
  });

  it("follows a symlink that resolves to a regular file", async () => {
    const migration = makeHotfix();
    const target = path.join(APPROVALS_DIR, "real_approval_file");
    await realFs.writeFile(target, "approved");
    await realFs.symlink(target, approvalPath(migration.version, migration.name));

    await expect(isHotfixApproved(migration)).resolves.toBe(true);
  });
});

describe("isHotfixApproved: a missing approval is a verdict, not a fault (H3)", () => {
  it("returns false when the approvals directory exists but the file does not", async () => {
    await expect(isHotfixApproved(makeHotfix())).resolves.toBe(false);
  });

  it("returns false when the approvals directory itself is missing", async () => {
    await realFs.rm(APPROVALS_DIR, { recursive: true, force: true });
    approvalsDirCreatedByUs = false;

    await expect(isHotfixApproved(makeHotfix())).resolves.toBe(false);
  });

  it.each([
    ["ENOTDIR", "approvals path is a regular file"],
    ["ENOTDIR", "a parent segment is not a directory"],
  ])("returns false on %s (%s)", async (code) => {
    mockedStat.mockRejectedValueOnce(fsError(code));

    await expect(isHotfixApproved(makeHotfix())).resolves.toBe(false);
  });

  it("ignores the contents of the approval file", async () => {
    const migration = makeHotfix();
    await realFs.writeFile(
      approvalPath(migration.version, migration.name),
      "explicitly rejected by the reviewer"
    );

    await expect(isHotfixApproved(migration)).resolves.toBe(true);
  });
});

describe("isHotfixApproved: operational faults stay diagnosable (H4)", () => {
  it.each(["EACCES", "EPERM", "ELOOP", "EMFILE", "ENFILE", "EIO"])(
    "surfaces %s instead of reporting the hotfix as unapproved",
    async (code) => {
      mockedStat.mockRejectedValueOnce(fsError(code));

      const migration = makeHotfix();
      await expect(isHotfixApproved(migration)).rejects.toThrow(
        new RegExp(`Unable to evaluate hotfix approval.*${code}`)
      );
    }
  );

  it("never reports a fault as a missing approval", async () => {
    for (const code of ["EACCES", "ELOOP"]) {
      mockedStat.mockRejectedValueOnce(fsError(code));

      const outcome = await isHotfixApproved(makeHotfix()).then(
        () => "resolved",
        () => "rejected"
      );

      expect(outcome).toBe("rejected");
    }
  });

  it("reports an error that carries no errno using its message", async () => {
    mockedStat.mockRejectedValueOnce(new Error("something unusual happened"));

    await expect(isHotfixApproved(makeHotfix())).rejects.toThrow(
      /Unable to evaluate hotfix approval.*something unusual happened/
    );
  });

  it("names the approval file but never the absolute approvals path", async () => {
    mockedStat.mockRejectedValueOnce(fsError("EACCES"));
    const migration = makeHotfix();

    const error = await isHotfixApproved(migration).then(
      () => null,
      (err: Error) => err
    );

    expect(error).toBeInstanceOf(Error);
    expect(error!.message).toContain(`${migration.version}_${migration.name}.approval`);
    expect(error!.message).toContain("EACCES");
    expect(error!.message).not.toContain(APPROVALS_DIR);
  });
});

describe("isHotfixApproved: the approval path cannot be redirected (H5)", () => {
  it.each([
    ["parent traversal", "../../etc/passwd"],
    ["embedded traversal", "sub/../../escape"],
    ["path separator", "nested/name"],
    ["absolute path", "/etc/passwd"],
    ["uppercase", "Example"],
    ["dot segment", "."],
  ])("refuses a migration name with a %s", async (_label, name) => {
    await expect(isHotfixApproved(makeHotfix({ name }))).rejects.toThrow(
      /not a valid identifier/
    );
    expect(mockedStat).not.toHaveBeenCalled();
  });

  it("resolves the approval path from the parsed name, not from content.name", async () => {
    const migration = makeHotfix();
    // content.name is attacker-controlled migration source; the runner only
    // ever populates ParsedMigration.name from the filename.
    (migration.content as { name: string }).name = "../../escape";

    await expect(isHotfixApproved(migration)).resolves.toBe(false);
    expect(mockedStat).toHaveBeenCalledWith(approvalPath(migration.version, migration.name));
  });

  it("keeps the approval path inside the approvals directory", async () => {
    const migration = makeHotfix();

    await isHotfixApproved(migration);

    const requested = mockedStat.mock.calls[0][0] as string;
    expect(path.dirname(requested)).toBe(APPROVALS_DIR);
  });
});

describe("isHotfixApproved: determinism and reentrancy (H6)", () => {
  it("returns the same verdict on repeated calls", async () => {
    const migration = makeHotfix();

    const results = await Promise.all([
      isHotfixApproved(migration),
      isHotfixApproved(migration),
      isHotfixApproved(migration),
    ]);

    expect(new Set(results).size).toBe(1);
    expect(results[0]).toBe(false);
  });

  it("returns the same verdict once the approval is added, for every concurrent caller", async () => {
    const migration = makeHotfix();
    await realFs.writeFile(approvalPath(migration.version, migration.name), "approved");

    const results = await Promise.all(
      Array.from({ length: 10 }, () => isHotfixApproved(migration))
    );

    expect(results.every((result) => result === true)).toBe(true);
  });

  it("does not cache a negative verdict across a later approval", async () => {
    const migration = makeHotfix();

    await expect(isHotfixApproved(migration)).resolves.toBe(false);

    await realFs.writeFile(approvalPath(migration.version, migration.name), "approved");

    await expect(isHotfixApproved(migration)).resolves.toBe(true);
  });
});

describe("runMigrations: the production hotfix gate uses isHotfixApproved (H3, H4)", () => {
  const makeDb = () => ({
    exec: jest.fn(),
    // buildContext exposes db.all / db.get / db.run directly to migration
    // `validate` hooks, so the double has to carry both shapes.
    all: jest.fn(() => []),
    get: jest.fn(() => undefined),
    run: jest.fn(() => ({ lastInsertRowId: 1, changes: 1 })),
    prepare: jest.fn(() => ({
      all: jest.fn(() => []),
      get: jest.fn(() => undefined),
      run: jest.fn(() => ({ lastInsertRowId: 1, changes: 1 })),
    })),
    transaction: jest.fn(),
  });

  beforeEach(() => {
    mockedReaddir.mockResolvedValue([HOTFIX_FILE] as never);
  });

  it("blocks an unapproved hotfix in production", async () => {
    await expect(runMigrations({ dryRun: true, db: makeDb() })).rejects.toThrow(
      `Hotfix migration ${HOTFIX_VERSION}_${HOTFIX_NAME} lacks production approval.`
    );
  });

  it("lets an approved hotfix through the production gate", async () => {
    await realFs.writeFile(approvalPath(HOTFIX_VERSION, HOTFIX_NAME), "approved");

    const result = await runMigrations({ dryRun: true, db: makeDb() });

    expect(result.applied.map((state) => state.version)).toContain(HOTFIX_VERSION);
  });

  it("surfaces an unreadable approval as a diagnosable error, not as a missing approval", async () => {
    mockedStat.mockRejectedValueOnce(fsError("EACCES"));

    await expect(runMigrations({ dryRun: true, db: makeDb() })).rejects.toThrow(
      /Unable to evaluate hotfix approval.*EACCES/
    );
  });

  it("skips the gate entirely for a non-hotfix migration in production", async () => {
    mockedReaddir.mockResolvedValue(["v001_initial_schema.ts"] as never);

    const result = await runMigrations({ dryRun: true, db: makeDb() });

    expect(mockedStat).not.toHaveBeenCalled();
    expect(result.applied.map((state) => state.version)).toContain(1);
  });
});
