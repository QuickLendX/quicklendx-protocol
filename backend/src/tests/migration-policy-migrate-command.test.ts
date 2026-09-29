/**
 * #2685 — deterministic failure-boundary coverage for migrateCommand
 * (backend/src/lib/migrations/policy.ts).
 *
 * The runner is mocked so every state — loading, error, retry, stale,
 * permission and concurrency — is reproduced deterministically without a
 * database or migration files.
 */
import {
  describeFailure,
  migrateCommand,
  migrateDownCommand,
  parseMigrateFlags,
  redactSensitive,
} from "../lib/migrations/policy";
import {
  getAppliedVersions,
  loadMigrationsFromFS,
  runMigrations,
  validateMigrationFiles,
} from "../lib/migrations/runner";

jest.mock("../lib/migrations/runner", () => ({
  runMigrations: jest.fn(),
  loadMigrationsFromFS: jest.fn(),
  getAppliedVersions: jest.fn(),
  validateMigrationFiles: jest.fn(),
}));

const mockRun = runMigrations as jest.MockedFunction<typeof runMigrations>;
const mockLoad = loadMigrationsFromFS as jest.MockedFunction<typeof loadMigrationsFromFS>;
const mockApplied = getAppliedVersions as jest.MockedFunction<typeof getAppliedVersions>;
const mockValidateFiles = validateMigrationFiles as jest.MockedFunction<typeof validateMigrationFiles>;

type RunResult = Awaited<ReturnType<typeof runMigrations>>;
type FileMigration = Awaited<ReturnType<typeof loadMigrationsFromFS>>[number];

function runResult(appliedCount: number, skipped = 0): RunResult {
  return {
    applied: Array.from({ length: appliedCount }, (_, i) => ({
      version: i + 1,
      name: `m${i + 1}`,
      checksum: "x",
      appliedAt: "2026-01-01T00:00:00.000Z",
      durationMs: 1,
      author: "dev",
      meta: {},
    })),
    skipped,
    durationMs: 5,
  };
}

function fileMigration(version: number, overrides: Record<string, unknown> = {}): FileMigration {
  return {
    file: `v00${version}_m${version}.ts`,
    version,
    name: `m${version}`,
    content: {
      version,
      name: `m${version}`,
      author: "dev",
      authoredAt: "2026-01-01",
      up: () => undefined,
      ...overrides,
    },
  } as unknown as FileMigration;
}

/** A promise the test resolves or rejects by hand, to hold a run "in flight". */
function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

let errorSpy: jest.SpyInstance;
let logSpy: jest.SpyInstance;
const ORIGINAL_ALLOW_DOWN = process.env.ALLOW_DOWN_MIGRATIONS;

beforeEach(() => {
  jest.resetAllMocks();
  errorSpy = jest.spyOn(console, "error").mockImplementation(() => undefined);
  logSpy = jest.spyOn(console, "log").mockImplementation(() => undefined);
  delete process.env.ALLOW_DOWN_MIGRATIONS;
  mockApplied.mockResolvedValue([]);
  mockRun.mockResolvedValue(runResult(0));
});

afterEach(() => {
  errorSpy.mockRestore();
  logSpy.mockRestore();
  if (ORIGINAL_ALLOW_DOWN === undefined) delete process.env.ALLOW_DOWN_MIGRATIONS;
  else process.env.ALLOW_DOWN_MIGRATIONS = ORIGINAL_ALLOW_DOWN;
});

const loggedErrors = () => errorSpy.mock.calls.map((call) => call.join(" ")).join("\n");

// ── Success ───────────────────────────────────────────────────────────────────

describe("migrateCommand — success", () => {
  test("runs pending migrations with safe defaults", async () => {
    mockRun.mockResolvedValue(runResult(2, 1));

    const result = await migrateCommand({});

    expect(result).toEqual({ success: true, message: "Applied 2 migrations", applied: 2, skipped: 1 });
    expect(mockRun).toHaveBeenCalledTimes(1);
    expect(mockRun).toHaveBeenCalledWith({
      dryRun: false,
      allowDown: false,
      verbose: false,
      skipChecksumVerify: false,
    });
  });

  test("ignores CLI positional arguments stored under `_`", async () => {
    const result = await migrateCommand({ _: ["up"] });
    expect(result.success).toBe(true);
  });

  test.each(["dryRun", "dryrun", "dry-run", "dry_run", "DRY-RUN"])(
    "regression: %s reaches the runner as dryRun (cli.ts sends `dryrun` for --dry-run)",
    async (key) => {
      await migrateCommand({ [key]: true });
      expect(mockRun).toHaveBeenCalledWith(expect.objectContaining({ dryRun: true }));
    },
  );

  test("regression: every multi-word CLI flag is honoured", async () => {
    await migrateCommand({ skipchecksumverify: true, verbose: true });
    expect(mockRun).toHaveBeenCalledWith({
      dryRun: false,
      allowDown: false,
      verbose: true,
      skipChecksumVerify: true,
    });
  });

  test("a dry run never snapshots applied state", async () => {
    mockRun.mockResolvedValue(runResult(3));
    const result = await migrateCommand({ dryRun: true });
    expect(result).toMatchObject({ success: true, applied: 3 });
    expect(mockApplied).not.toHaveBeenCalled();
  });

  test("allow-down runs only with --emergency and ALLOW_DOWN_MIGRATIONS=true", async () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    const result = await migrateCommand({ allowDown: true, emergency: true });
    expect(result.success).toBe(true);
    expect(mockRun).toHaveBeenCalledWith(expect.objectContaining({ allowDown: true }));
  });
});

// ── Rejection: invalid input and permission guards ──────────────────────────

describe("migrateCommand — rejection", () => {
  test.each([
    ["a typo", { dryrn: true }, /Unknown flag --dryrn/],
    ["a non-boolean string (\"false\" is truthy)", { dryRun: "false" }, /dryRun must be a boolean \(got string\)/],
    ["a number", { verbose: 1 }, /verbose must be a boolean \(got number\)/],
    ["conflicting duplicates", { dryRun: true, "dry-run": false }, /more than once with different values/],
    ["--to on an up run", { to: true }, /only applies to "migrate down"/],
    ["--all on an up run", { all: true }, /only applies to "migrate down"/],
  ])("rejects %s with INVALID_ARGS before touching the database", async (_label, args, message) => {
    const result = await migrateCommand(args as Record<string, unknown>);
    expect(result).toMatchObject({ success: false, code: "INVALID_ARGS" });
    expect(result.message).toMatch(message);
    expect(mockRun).not.toHaveBeenCalled();
    expect(mockApplied).not.toHaveBeenCalled();
  });

  test.each([["a string", "--dry-run"], ["an array", ["--dry-run"]], ["a number", 7]])(
    "rejects %s as the argument object",
    async (_label, args) => {
      const result = await migrateCommand(args as unknown as Record<string, unknown>);
      expect(result).toMatchObject({ success: false, code: "INVALID_ARGS" });
      expect(mockRun).not.toHaveBeenCalled();
    },
  );

  test("security: a string emergency flag cannot bypass the down-migration guard", async () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    const result = await migrateCommand({ allowDown: true, emergency: "false" });
    expect(result).toMatchObject({ success: false, code: "INVALID_ARGS" });
    expect(mockRun).not.toHaveBeenCalled();
  });

  test("never echoes flag values (they may carry secrets)", async () => {
    const result = await migrateCommand({ verbose: "postgres://admin:hunter2@db.internal/app" });
    expect(result.message).not.toContain("hunter2");
    expect(loggedErrors()).not.toContain("hunter2");
  });

  test("sanitises odd characters in unknown keys", async () => {
    const result = await migrateCommand({ "x\n<script>": true });
    expect(result.message).not.toMatch(/[\n<>]/);
  });

  test.each([
    [{ check: true, validateOnly: true }, /--check and --validate-only/],
    [{ check: true, allowDown: true }, /--allow-down cannot be combined/],
    [{ validateonly: true, allowdown: true }, /--allow-down cannot be combined/],
  ])("rejects mutually exclusive modes %p", async (args, message) => {
    const result = await migrateCommand(args);
    expect(result).toMatchObject({ success: false, code: "CONFLICTING_FLAGS" });
    expect(result.message).toMatch(message);
    expect(mockRun).not.toHaveBeenCalled();
    expect(mockLoad).not.toHaveBeenCalled();
  });

  test("permission: allow-down without --emergency is refused", async () => {
    const result = await migrateCommand({ allowDown: true });
    expect(result).toEqual({
      success: false,
      code: "DOWN_REFUSED",
      message: "Refusing to run down migrations without --emergency flag. This is a safety guard.",
    });
    expect(mockRun).not.toHaveBeenCalled();
  });

  test("permission: allow-down is refused while ALLOW_DOWN_MIGRATIONS is unset", async () => {
    const result = await migrateCommand({ allowDown: true, emergency: true });
    expect(result).toMatchObject({ success: false, code: "DOWN_REFUSED" });
    expect(result.message).toMatch(/globally disabled/);
    expect(mockRun).not.toHaveBeenCalled();
  });

  test("permission: ALLOW_DOWN_MIGRATIONS must be exactly \"true\"", async () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "TRUE";
    const result = await migrateCommand({ allowDown: true, emergency: true });
    expect(result.code).toBe("DOWN_REFUSED");
  });
});

// ── --check: loading, stale and error states ─────────────────────────────────

describe("migrateCommand --check", () => {
  test("passes when files are valid and every file is applied", async () => {
    mockValidateFiles.mockResolvedValue({ valid: true, errors: [] });
    mockLoad.mockResolvedValue([fileMigration(1), fileMigration(2)]);
    mockApplied.mockResolvedValue([1, 2]);

    await expect(migrateCommand({ check: true })).resolves.toEqual({ success: true, message: "Migrations valid" });
    expect(mockRun).not.toHaveBeenCalled();
  });

  test("fails when a file migration is pending", async () => {
    mockValidateFiles.mockResolvedValue({ valid: true, errors: [] });
    mockLoad.mockResolvedValue([fileMigration(1), fileMigration(2)]);
    mockApplied.mockResolvedValue([1]);

    const result = await migrateCommand({ check: true });
    expect(result).toMatchObject({ success: false, code: "CHECK_FAILED" });
    expect(loggedErrors()).toContain("Migration 2_m2 is not applied");
  });

  test("stale: fails when the database has a migration this checkout does not know", async () => {
    mockValidateFiles.mockResolvedValue({ valid: true, errors: [] });
    mockLoad.mockResolvedValue([fileMigration(1)]);
    mockApplied.mockResolvedValue([1, 2]);

    const result = await migrateCommand({ check: true });
    expect(result).toMatchObject({ success: false, code: "CHECK_FAILED" });
    expect(loggedErrors()).toContain("Applied migration 2 has no file in this checkout");
  });

  test("fails when file validation fails", async () => {
    mockValidateFiles.mockResolvedValue({ valid: false, errors: ["Gap detected: migration 2 is missing"] });
    mockLoad.mockResolvedValue([fileMigration(1), fileMigration(3)]);
    mockApplied.mockResolvedValue([1, 3]);

    const result = await migrateCommand({ check: true });
    expect(result.code).toBe("CHECK_FAILED");
    expect(loggedErrors()).toContain("Gap detected");
  });

  test("resolves with LOAD_FAILED (never rejects) when applied state cannot be read", async () => {
    mockValidateFiles.mockResolvedValue({ valid: true, errors: [] });
    mockLoad.mockResolvedValue([fileMigration(1)]);
    mockApplied.mockRejectedValue(new Error("SQLITE_CANTOPEN: unable to open database file"));

    const result = await migrateCommand({ check: true });
    expect(result).toMatchObject({ success: false, code: "LOAD_FAILED" });
    expect(result.message).toContain("SQLITE_CANTOPEN");
  });
});

// ── --validate-only ──────────────────────────────────────────────────────────

describe("migrateCommand --validate-only", () => {
  test("passes for valid, unique migration files", async () => {
    mockLoad.mockResolvedValue([fileMigration(1), fileMigration(2)]);
    await expect(migrateCommand({ validateOnly: true })).resolves.toEqual({
      success: true,
      message: "Validation passed",
    });
  });

  test("regression: validates each file's definition, not the loader wrapper", async () => {
    // loadMigrationsFromFS returns { file, version, name, content }; author,
    // authoredAt and up live on `content`. Validating the wrapper reported
    // every valid migration as missing its author.
    mockLoad.mockResolvedValue([fileMigration(1)]);
    const result = await migrateCommand({ validateonly: true });
    expect(result.success).toBe(true);
    expect(loggedErrors()).not.toContain("author is required");
  });

  test("names the offending migration in validation errors", async () => {
    mockLoad.mockResolvedValue([fileMigration(4, { authoredAt: "" })]);
    await migrateCommand({ validateOnly: true });
    expect(loggedErrors()).toContain("4_m4: Migration authoredAt date is required");
  });

  test("fails on duplicate versions and missing metadata", async () => {
    mockLoad.mockResolvedValue([fileMigration(1), fileMigration(1, { author: "" })]);
    const result = await migrateCommand({ "validate-only": true });
    expect(result).toMatchObject({ success: false, code: "VALIDATION_FAILED" });
    expect(loggedErrors()).toContain("Duplicate migration version 1");
    expect(loggedErrors()).toContain("Migration author is required");
  });

  test("resolves with LOAD_FAILED when a migration file cannot be loaded", async () => {
    mockLoad.mockRejectedValue(new Error("Failed to load migration v002_x.ts: Unexpected token"));
    const result = await migrateCommand({ validateOnly: true });
    expect(result).toMatchObject({ success: false, code: "LOAD_FAILED" });
  });
});

// ── Run failures, partial failure and retry ──────────────────────────────────

describe("migrateCommand — run failures and recovery", () => {
  test("redacts credentials from the returned message and the log", async () => {
    mockRun.mockRejectedValue(
      new Error("connect ECONNREFUSED postgres://admin:hunter2@db.internal:5432/app password=s3cret"),
    );

    const result = await migrateCommand({});
    expect(result.code).toBe("RUN_FAILED");
    expect(result.message).toContain("postgres://***@db.internal:5432/app");
    expect(result.message).toContain("password=***");
    for (const text of [result.message, loggedErrors()]) {
      expect(text).not.toContain("hunter2");
      expect(text).not.toContain("s3cret");
    }
  });

  test.each([
    ["a string", "boom", "Migration error: boom"],
    ["undefined", undefined, "Migration error: Unknown error"],
    ["null", null, "Migration error: Unknown error"],
    ["a plain object", { reason: "x" }, "Migration error: Unknown error"],
    ["an Error with an empty message", new Error(""), "Migration error: Unknown error"],
  ])("handles a thrown %s without crashing", async (_label, thrown, prefix) => {
    mockRun.mockRejectedValue(thrown);
    const result = await migrateCommand({ dryRun: true });
    expect(result).toEqual({ success: false, code: "RUN_FAILED", message: prefix });
  });

  test("partial failure reports how many migrations were committed", async () => {
    mockApplied.mockResolvedValueOnce([1]).mockResolvedValueOnce([1, 2, 3]);
    mockRun.mockRejectedValue(new Error("migration v004 failed"));

    const result = await migrateCommand({});
    expect(result).toMatchObject({ success: false, code: "RUN_FAILED", appliedBeforeFailure: 2 });
    expect(result.message).toContain("2 migration(s) were committed before the failure");
    expect(result.message).toContain("re-run to resume");
  });

  test("a failure with no progress says nothing was recorded", async () => {
    mockApplied.mockResolvedValue([1]);
    mockRun.mockRejectedValue(new Error("checksum mismatch"));

    const result = await migrateCommand({});
    expect(result).toMatchObject({ appliedBeforeFailure: 0 });
    expect(result.message).toContain("no migrations were recorded as applied by this run");
  });

  test("still fails cleanly when applied state is unreadable before and after", async () => {
    mockApplied.mockRejectedValue(new Error("no such table: _migrations"));
    mockRun.mockRejectedValue(new Error("disk I/O error"));

    const result = await migrateCommand({});
    expect(result).toEqual({ success: false, code: "RUN_FAILED", message: "Migration error: disk I/O error" });
  });

  test("retry after a partial failure resumes and succeeds", async () => {
    mockApplied.mockResolvedValueOnce([]).mockResolvedValueOnce([1]).mockResolvedValue([1, 2]);
    mockRun
      .mockRejectedValueOnce(new Error("migration v002 failed"))
      .mockResolvedValueOnce(runResult(1, 1));

    const first = await migrateCommand({});
    expect(first).toMatchObject({ success: false, appliedBeforeFailure: 1 });

    const second = await migrateCommand({});
    expect(second).toEqual({ success: true, message: "Applied 1 migrations", applied: 1, skipped: 1 });
    expect(mockRun).toHaveBeenCalledTimes(2);
  });

  test("a failed dry run never reads applied state", async () => {
    mockRun.mockRejectedValue(new Error("validate() threw"));
    const result = await migrateCommand({ dryRun: true });
    expect(result.appliedBeforeFailure).toBeUndefined();
    expect(mockApplied).not.toHaveBeenCalled();
  });
});

// ── Concurrency ──────────────────────────────────────────────────────────────

describe("migrateCommand — concurrency", () => {
  test("a second concurrent run gets BUSY and the runner starts once", async () => {
    const gate = deferred<RunResult>();
    mockRun.mockReturnValueOnce(gate.promise);

    const first = migrateCommand({});
    const second = await migrateCommand({});

    expect(second).toMatchObject({ success: false, code: "BUSY" });
    expect(second.message).toContain('"migrate"');

    gate.resolve(runResult(1));
    await expect(first).resolves.toMatchObject({ success: true, applied: 1 });
    expect(mockRun).toHaveBeenCalledTimes(1);

    // The slot is released: a later run proceeds.
    await expect(migrateCommand({})).resolves.toMatchObject({ success: true });
  });

  test("up and down never overlap", async () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    const gate = deferred<RunResult>();
    mockRun.mockReturnValueOnce(gate.promise);

    const up = migrateCommand({});
    const down = await migrateDownCommand({ emergency: true });
    expect(down).toMatchObject({ success: false, code: "BUSY" });

    gate.resolve(runResult(0));
    await up;
  });

  test("the slot is released after a failed run and after a failed check", async () => {
    mockRun.mockRejectedValueOnce(new Error("boom"));
    await migrateCommand({});
    mockValidateFiles.mockRejectedValueOnce(new Error("EACCES: permission denied, scandir"));
    await expect(migrateCommand({ check: true })).resolves.toMatchObject({ code: "LOAD_FAILED" });

    await expect(migrateCommand({})).resolves.toMatchObject({ success: true });
  });

  test("invalid arguments are rejected even while another run holds the slot", async () => {
    const gate = deferred<RunResult>();
    mockRun.mockReturnValueOnce(gate.promise);
    const first = migrateCommand({});

    await expect(migrateCommand({ bogus: true })).resolves.toMatchObject({ code: "INVALID_ARGS" });

    gate.resolve(runResult(0));
    await first;
  });
});

// ── Regression: migrateDownCommand guards unchanged ─────────────────────────

describe("migrateDownCommand — existing guards", () => {
  test("still requires --emergency or ALLOW_DOWN_MIGRATIONS", async () => {
    const result = await migrateDownCommand({});
    expect(result.success).toBe(false);
    expect(result.message).toMatch(/require --emergency flag or ALLOW_DOWN_MIGRATIONS=true/);
    expect(mockRun).not.toHaveBeenCalled();
  });

  test("still rejects --to together with --all", async () => {
    const result = await migrateDownCommand({ emergency: true, to: "3", all: true });
    expect(result).toEqual({ success: false, message: "Cannot specify both --to and --all flags." });
  });
});

// ── Helpers ──────────────────────────────────────────────────────────────────

describe("helpers", () => {
  test("parseMigrateFlags treats undefined values and missing args as defaults", () => {
    expect(parseMigrateFlags(undefined)).toEqual({
      ok: true,
      flags: {
        dryRun: false,
        allowDown: false,
        emergency: false,
        verbose: false,
        validateOnly: false,
        check: false,
        skipChecksumVerify: false,
      },
    });
    expect(parseMigrateFlags({ dryRun: undefined })).toMatchObject({ ok: true, flags: { dryRun: false } });
    expect(parseMigrateFlags({ dryRun: true, dryrun: true })).toMatchObject({ ok: true, flags: { dryRun: true } });
  });

  test.each([
    ["mysql://root:pw@localhost/db", "mysql://***@localhost/db"],
    ['token: "abc.def"', "token: ***"],
    ["API_KEY=xyz123", "API_KEY=***"],
    ["secret:shh", "secret:***"],
    ["https://example.com/path", "https://example.com/path"],
  ])("redactSensitive(%p)", (input, expected) => {
    expect(redactSensitive(input)).toBe(expected);
  });

  test("describeFailure trims and falls back to a generic message", () => {
    expect(describeFailure(new Error("  spaced  "))).toBe("spaced");
    expect(describeFailure(42)).toBe("Unknown error");
  });
});
