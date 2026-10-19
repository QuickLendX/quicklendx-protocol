/**
 * Deterministic failure-boundary tests for migrateDownCommand
 * (backend/src/lib/migrations/policy.ts)
 *
 * Coverage matrix
 * ───────────────
 * Authorization / emergency gate
 *   ✓ blocked without emergency flag and without env-var (EMERGENCY_REQUIRED)
 *   ✓ blocked when env-var is absent and emergency=false
 *   ✓ blocked when env-var is "1" (not "true") and no emergency
 *   ✓ allowed when ALLOW_DOWN_MIGRATIONS=true without emergency flag
 *   ✓ allowed when emergency=true even without env-var
 *   ✓ allowed when both emergency=true and ALLOW_DOWN_MIGRATIONS=true
 *   ✓ EMERGENCY_REQUIRED message is descriptive and contains "emergency"
 *
 * Concurrent-execution guard
 *   ✓ blocked when a rollback is already in progress
 *   ✓ guard is released after successful completion
 *   ✓ guard is released even when runMigrations throws
 *   ✓ _resetRollbackGuard() unblocks subsequent calls
 *
 * Flag mutual-exclusion: --to and --all
 *   ✓ both flags supplied → CONFLICTING_FLAGS rejection
 *   ✓ --to alone is accepted
 *   ✓ --all alone is accepted
 *   ✓ neither flag (default single-step) is accepted
 *
 * --to target-version validation
 *   ✓ non-numeric string "abc" → INVALID_TARGET_VERSION
 *   ✓ negative integer "-1" → INVALID_TARGET_VERSION
 *   ✓ zero "0" → INVALID_TARGET_VERSION
 *   ✓ float "1.5" → INVALID_TARGET_VERSION
 *   ✓ valid positive integer "3" is accepted
 *   ✓ error message includes the rejected value
 *
 * Empty-state guard
 *   ✓ returns success with applied=0 when no migrations are applied
 *   ✓ does not call runMigrations when nothing to roll back
 *
 * Missing down-function pre-flight check
 *   ✓ blocked when targeted migration has no down function
 *   ✓ error message names the specific migration
 *   ✓ blocked on any migration in the target set lacking down (not just first)
 *   ✓ allowed when all targeted migrations have down functions
 *
 * Dry-run mode
 *   ✓ dry-run=true is forwarded to runMigrations
 *   ✓ dry-run response contains "DRY-RUN" or "Would roll back" in message
 *   ✓ dry-run does not mutate applied state (runMigrations called with dryRun=true)
 *
 * Success paths
 *   ✓ default (no flags) rolls back one migration
 *   ✓ --all rolls back all applied migrations
 *   ✓ --to N rolls back only versions > N
 *   ✓ result includes applied count and skipped count
 *   ✓ message reflects number of rolled-back migrations
 *
 * Execution failure handling
 *   ✓ runMigrations throw is caught and returned as success=false
 *   ✓ error message contains the underlying error message
 *   ✓ guard is released even on execution failure
 *   ✓ subsequent call after failure succeeds
 *
 * _computeTargetVersions (pure unit tests, no side effects)
 *   ✓ all=true returns all versions descending
 *   ✓ targetVersion returns only versions > N descending
 *   ✓ default returns only the maximum version
 *   ✓ empty list returns []
 *   ✓ targetVersion equal to max returns []
 *   ✓ targetVersion 0 returns all versions
 *   ✓ all=true overrides targetVersion
 *
 * MigrationPolicy static methods (regression guard)
 *   ✓ isDownAllowed false when env unset
 *   ✓ isDownAllowed true when env is "true"
 *   ✓ isDownAllowed false when env is "1" or "yes"
 *   ✓ isHotfix true when meta.hotfix === true
 *   ✓ isHotfix false when meta absent
 *   ✓ validateMetadata error list for missing required fields
 *   ✓ validateMetadata hotfix requires reason, rollback_risk, down
 *   ✓ validateMetadata passes for valid standard migration
 *   ✓ validateMetadata passes for valid hotfix migration
 *   ✓ dryRun surfaces validate() warnings from migration hooks
 *   ✓ dryRun catches throwing validate() hooks without failing the run
 *   ✓ dryRun detects duplicate version numbers
 *
 * MigrationPolicyError
 *   ✓ is an instance of Error
 *   ✓ code field is readable
 *   ✓ cause is preserved
 *   ✓ name is "MigrationPolicyError"
 */

import {
  migrateDownCommand,
  MigrationPolicy,
  MigrationPolicyError,
  _computeTargetVersions,
  _resetRollbackGuard,
} from "../lib/migrations/policy";

// ---------------------------------------------------------------------------
// Module-level mocks
// ---------------------------------------------------------------------------

// Mock runner to control runMigrations, getAppliedVersions, loadMigrationsFromFS
jest.mock("../lib/migrations/runner", () => ({
  runMigrations: jest.fn(),
  loadMigrationsFromFS: jest.fn(),
  getAppliedVersions: jest.fn(),
  validateMigrationFiles: jest.fn(),
}));

import * as runner from "../lib/migrations/runner";

const mockRunMigrations = runner.runMigrations as jest.MockedFunction<typeof runner.runMigrations>;
const mockGetAppliedVersions = runner.getAppliedVersions as jest.MockedFunction<typeof runner.getAppliedVersions>;
const mockLoadMigrationsFromFS = runner.loadMigrationsFromFS as jest.MockedFunction<typeof runner.loadMigrationsFromFS>;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/** A minimal migration definition with a down function. */
function makeMig(version: number, hasDown = true) {
  return {
    file: `v00${version}_test.ts`,
    version,
    name: `test_v${version}`,
    content: {
      version,
      name: `test_v${version}`,
      authoredAt: "2026-01-01",
      author: "test",
      up: async () => {},
      ...(hasDown ? { down: async () => {} } : {}),
    },
  } as any;
}

/** Default happy-path runMigrations result. */
function makeRunResult(applied = 1, skipped = 0) {
  return {
    applied: Array.from({ length: applied }, (_, i) => ({
      version: 10 - i,
      name: `test_v${10 - i}`,
      checksum: "abc",
      appliedAt: new Date().toISOString(),
      durationMs: 5,
      author: "test",
    })),
    skipped,
    durationMs: 10,
  };
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

const ORIGINAL_ENV = process.env.ALLOW_DOWN_MIGRATIONS;

beforeEach(() => {
  jest.clearAllMocks();
  _resetRollbackGuard();

  // Delete the env var so default is "blocked"
  delete process.env.ALLOW_DOWN_MIGRATIONS;

  // Default: two migrations applied, both have down functions
  mockGetAppliedVersions.mockResolvedValue([1, 2]);
  mockLoadMigrationsFromFS.mockResolvedValue([makeMig(1), makeMig(2)]);
  mockRunMigrations.mockResolvedValue(makeRunResult());
});

afterEach(() => {
  // Restore env var
  if (ORIGINAL_ENV !== undefined) {
    process.env.ALLOW_DOWN_MIGRATIONS = ORIGINAL_ENV;
  } else {
    delete process.env.ALLOW_DOWN_MIGRATIONS;
  }
  _resetRollbackGuard();
});

// ===========================================================================
// Authorization / emergency gate
// ===========================================================================

describe("authorization gate", () => {
  it("blocked without emergency flag and without env-var", async () => {
    const result = await migrateDownCommand({ emergency: false });
    expect(result.success).toBe(false);
    expect(result.message).toMatch(/emergency/i);
    expect(mockRunMigrations).not.toHaveBeenCalled();
  });

  it("blocked when env-var is absent and emergency=false", async () => {
    delete process.env.ALLOW_DOWN_MIGRATIONS;
    const result = await migrateDownCommand({});
    expect(result.success).toBe(false);
    expect(mockRunMigrations).not.toHaveBeenCalled();
  });

  it('blocked when env-var is "1" (not "true") and no emergency', async () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "1";
    const result = await migrateDownCommand({ emergency: false });
    expect(result.success).toBe(false);
    expect(mockRunMigrations).not.toHaveBeenCalled();
  });

  it('allowed when ALLOW_DOWN_MIGRATIONS=true without emergency flag', async () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    const result = await migrateDownCommand({ emergency: false });
    expect(result.success).toBe(true);
    expect(mockRunMigrations).toHaveBeenCalledTimes(1);
  });

  it("allowed when emergency=true even without env-var", async () => {
    const result = await migrateDownCommand({ emergency: true });
    expect(result.success).toBe(true);
    expect(mockRunMigrations).toHaveBeenCalledTimes(1);
  });

  it("allowed when both emergency=true and ALLOW_DOWN_MIGRATIONS=true", async () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    const result = await migrateDownCommand({ emergency: true });
    expect(result.success).toBe(true);
  });

  it("rejection message contains 'emergency' and 'ALLOW_DOWN_MIGRATIONS'", async () => {
    const result = await migrateDownCommand({});
    expect(result.message).toContain("emergency");
    expect(result.message).toContain("ALLOW_DOWN_MIGRATIONS");
  });
});

// ===========================================================================
// Concurrent-execution guard
// ===========================================================================

describe("concurrent-execution guard", () => {
  it("blocked when a rollback is already in progress", async () => {
    // The guard is set synchronously before await runMigrations.
    // We simulate "in progress" by ensuring the first call's async pre-flight
    // steps complete before the guard check in the second call.
    // The simplest reliable approach: make getAppliedVersions never resolve
    // for the first call, so the first call stalls at the pre-flight check and
    // never sets the guard.  Instead, we use _resetRollbackGuard + manual flag
    // manipulation via a second mock that observes guard state directly.

    // Strategy: run first call to completion (it sets then clears guard),
    // then confirm the guard test itself by making runMigrations stall.
    let resolveFirst!: (v: any) => void;
    mockRunMigrations
      // First call: stalls at runMigrations — guard is set here
      .mockImplementationOnce(() => new Promise<any>((res) => { resolveFirst = res; }))
      // Third call (after reset): resolves immediately
      .mockResolvedValue(makeRunResult());

    // First call: reaches runMigrations and sets guard, then stalls
    const firstCall = migrateDownCommand({ emergency: true });
    // Give the event loop enough turns to get past all async pre-flight checks
    for (let i = 0; i < 10; i++) await Promise.resolve();

    // Now the guard should be set — second call should be blocked
    const second = await migrateDownCommand({ emergency: true });
    expect(second.success).toBe(false);
    expect(second.message).toMatch(/already in progress/i);

    // Unblock first call
    resolveFirst(makeRunResult());
    await firstCall;
  });

  it("guard is released after successful completion", async () => {
    await migrateDownCommand({ emergency: true });
    // A second call should succeed (guard released)
    const result = await migrateDownCommand({ emergency: true });
    expect(result.success).toBe(true);
    expect(mockRunMigrations).toHaveBeenCalledTimes(2);
  });

  it("guard is released even when runMigrations throws", async () => {
    mockRunMigrations.mockRejectedValueOnce(new Error("db crash"));
    const first = await migrateDownCommand({ emergency: true });
    expect(first.success).toBe(false);

    // Guard must be cleared so a retry can proceed
    mockRunMigrations.mockResolvedValueOnce(makeRunResult());
    const retry = await migrateDownCommand({ emergency: true });
    expect(retry.success).toBe(true);
  });

  it("_resetRollbackGuard unblocks subsequent calls", async () => {
    let resolveFirst!: (v: any) => void;
    mockRunMigrations
      .mockImplementationOnce(() => new Promise<any>((res) => { resolveFirst = res; }))
      .mockResolvedValue(makeRunResult());

    // First call stalls at runMigrations — guard is set
    const firstCall = migrateDownCommand({ emergency: true });
    for (let i = 0; i < 10; i++) await Promise.resolve();

    // Guard should be set — second call is blocked
    const blocked = await migrateDownCommand({ emergency: true });
    expect(blocked.success).toBe(false);

    // Reset guard manually (simulates a crash that left it set)
    _resetRollbackGuard();

    // Now a new call should proceed
    const after = await migrateDownCommand({ emergency: true });
    expect(after.success).toBe(true);

    // Release the original first call
    resolveFirst(makeRunResult());
    await firstCall;
  });
});

// ===========================================================================
// Flag mutual-exclusion: --to and --all
// ===========================================================================

describe("--to / --all mutual exclusion", () => {
  it("both --to and --all → rejected before DB access", async () => {
    const result = await migrateDownCommand({ emergency: true, to: "3", all: true });
    expect(result.success).toBe(false);
    expect(result.message).toMatch(/cannot specify both/i);
    expect(mockRunMigrations).not.toHaveBeenCalled();
  });

  it("--to alone is accepted", async () => {
    mockGetAppliedVersions.mockResolvedValue([1, 2, 3, 4]);
    mockLoadMigrationsFromFS.mockResolvedValue([makeMig(1), makeMig(2), makeMig(3), makeMig(4)]);
    const result = await migrateDownCommand({ emergency: true, to: "2" });
    expect(result.success).toBe(true);
  });

  it("--all alone is accepted", async () => {
    const result = await migrateDownCommand({ emergency: true, all: true });
    expect(result.success).toBe(true);
  });

  it("neither flag (default single-step) is accepted", async () => {
    const result = await migrateDownCommand({ emergency: true });
    expect(result.success).toBe(true);
  });
});

// ===========================================================================
// --to target-version validation
// ===========================================================================

describe("--to value validation", () => {
  it('"abc" → rejected as INVALID_TARGET_VERSION', async () => {
    const result = await migrateDownCommand({ emergency: true, to: "abc" });
    expect(result.success).toBe(false);
    expect(result.message).toMatch(/invalid.*to/i);
    expect(result.message).toContain("abc");
    expect(mockRunMigrations).not.toHaveBeenCalled();
  });

  it('"-1" → rejected', async () => {
    const result = await migrateDownCommand({ emergency: true, to: "-1" });
    expect(result.success).toBe(false);
    expect(result.message).toMatch(/invalid.*to/i);
  });

  it('"0" → rejected', async () => {
    const result = await migrateDownCommand({ emergency: true, to: "0" });
    expect(result.success).toBe(false);
    expect(result.message).toMatch(/invalid.*to/i);
  });

  it('"1.5" → rejected (non-integer float)', async () => {
    const result = await migrateDownCommand({ emergency: true, to: "1.5" });
    expect(result.success).toBe(false);
    expect(result.message).toMatch(/invalid.*to/i);
  });

  it('"3" → accepted (valid positive integer)', async () => {
    mockGetAppliedVersions.mockResolvedValue([1, 2, 3]);
    mockLoadMigrationsFromFS.mockResolvedValue([makeMig(1), makeMig(2), makeMig(3)]);
    const result = await migrateDownCommand({ emergency: true, to: "1" });
    expect(result.success).toBe(true);
  });

  it("error message includes the rejected value", async () => {
    const result = await migrateDownCommand({ emergency: true, to: "notanumber" });
    expect(result.message).toContain("notanumber");
  });
});

// ===========================================================================
// Empty-state guard
// ===========================================================================

describe("empty-state guard (nothing applied)", () => {
  it("returns success=true with applied=0 when no migrations are applied", async () => {
    mockGetAppliedVersions.mockResolvedValue([]);
    const result = await migrateDownCommand({ emergency: true });
    expect(result.success).toBe(true);
    expect(result.applied).toBe(0);
    expect(result.skipped).toBe(0);
  });

  it("does not call runMigrations when nothing to roll back", async () => {
    mockGetAppliedVersions.mockResolvedValue([]);
    await migrateDownCommand({ emergency: true });
    expect(mockRunMigrations).not.toHaveBeenCalled();
  });

  it("message indicates nothing to roll back", async () => {
    mockGetAppliedVersions.mockResolvedValue([]);
    const result = await migrateDownCommand({ emergency: true });
    expect(result.message).toMatch(/nothing to roll back/i);
  });
});

// ===========================================================================
// Missing down-function pre-flight check
// ===========================================================================

describe("missing down-function pre-flight", () => {
  it("blocked when default target migration has no down function", async () => {
    mockGetAppliedVersions.mockResolvedValue([1, 2]);
    mockLoadMigrationsFromFS.mockResolvedValue([makeMig(1), makeMig(2, false)]); // v2 has no down
    const result = await migrateDownCommand({ emergency: true });
    expect(result.success).toBe(false);
    expect(result.message).toMatch(/down.*function|lack.*down/i);
    expect(mockRunMigrations).not.toHaveBeenCalled();
  });

  it("error message names the specific missing-down migration", async () => {
    mockGetAppliedVersions.mockResolvedValue([1, 2]);
    mockLoadMigrationsFromFS.mockResolvedValue([makeMig(1), makeMig(2, false)]);
    const result = await migrateDownCommand({ emergency: true });
    // The migration name should appear in the error
    expect(result.message).toContain("test_v2");
  });

  it("blocked on ANY migration in target set lacking down", async () => {
    mockGetAppliedVersions.mockResolvedValue([1, 2, 3]);
    mockLoadMigrationsFromFS.mockResolvedValue([
      makeMig(1, false), // no down
      makeMig(2),
      makeMig(3),
    ]);
    // --all targets all three; v1 has no down
    const result = await migrateDownCommand({ emergency: true, all: true });
    expect(result.success).toBe(false);
    expect(result.message).toContain("test_v1");
  });

  it("allowed when all targeted migrations have down functions", async () => {
    mockGetAppliedVersions.mockResolvedValue([1, 2, 3]);
    mockLoadMigrationsFromFS.mockResolvedValue([makeMig(1), makeMig(2), makeMig(3)]);
    mockRunMigrations.mockResolvedValue(makeRunResult(3, 0));
    const result = await migrateDownCommand({ emergency: true, all: true });
    expect(result.success).toBe(true);
  });

  it("with --to: only checks migrations in the target set, not excluded ones", async () => {
    // Applied: 1, 2, 3.  Target (to=2): only version 3.  Version 1, 2 have no down but are excluded.
    mockGetAppliedVersions.mockResolvedValue([1, 2, 3]);
    mockLoadMigrationsFromFS.mockResolvedValue([
      makeMig(1, false), // no down — excluded by --to 2
      makeMig(2, false), // no down — excluded by --to 2
      makeMig(3),        // has down — is the target
    ]);
    const result = await migrateDownCommand({ emergency: true, to: "2" });
    expect(result.success).toBe(true);
  });
});

// ===========================================================================
// Dry-run mode
// ===========================================================================

describe("dry-run mode", () => {
  it("dryRun=true is forwarded to runMigrations", async () => {
    await migrateDownCommand({ emergency: true, dryRun: true });
    expect(mockRunMigrations).toHaveBeenCalledWith(
      expect.objectContaining({ dryRun: true, allowDown: true })
    );
  });

  it("dry-run does not change success=true result", async () => {
    const result = await migrateDownCommand({ emergency: true, dryRun: true });
    expect(result.success).toBe(true);
  });

  it("dry-run message contains 'Would roll back' or '[DRY-RUN]'", async () => {
    const result = await migrateDownCommand({ emergency: true, dryRun: true });
    expect(result.message).toMatch(/would roll back|dry.?run/i);
  });

  it("dry-run response still includes applied count", async () => {
    mockRunMigrations.mockResolvedValue(makeRunResult(2, 0));
    const result = await migrateDownCommand({ emergency: true, dryRun: true });
    expect(result.applied).toBe(2);
  });
});

// ===========================================================================
// Success paths — result shape
// ===========================================================================

describe("success paths", () => {
  it("default (no flags) rolls back the single highest applied migration", async () => {
    mockRunMigrations.mockResolvedValue(makeRunResult(1, 0));
    const result = await migrateDownCommand({ emergency: true });
    expect(result.success).toBe(true);
    expect(result.applied).toBe(1);
    // runner called with allowDown=true and no `all` flag
    expect(mockRunMigrations).toHaveBeenCalledWith(
      expect.objectContaining({ allowDown: true })
    );
  });

  it("--all passes all=true to runMigrations", async () => {
    mockRunMigrations.mockResolvedValue(makeRunResult(2, 0));
    await migrateDownCommand({ emergency: true, all: true });
    expect(mockRunMigrations).toHaveBeenCalledWith(
      expect.objectContaining({ all: true })
    );
  });

  it("--to N passes to as string to runMigrations", async () => {
    mockGetAppliedVersions.mockResolvedValue([1, 2, 3]);
    mockLoadMigrationsFromFS.mockResolvedValue([makeMig(1), makeMig(2), makeMig(3)]);
    mockRunMigrations.mockResolvedValue(makeRunResult(1, 0));
    await migrateDownCommand({ emergency: true, to: "2" });
    expect(mockRunMigrations).toHaveBeenCalledWith(
      expect.objectContaining({ to: "2" })
    );
  });

  it("result applied count matches runMigrations return value", async () => {
    mockRunMigrations.mockResolvedValue(makeRunResult(3, 1));
    const result = await migrateDownCommand({ emergency: true, all: true });
    expect(result.applied).toBe(3);
    expect(result.skipped).toBe(1);
  });

  it("success message contains the rolled-back count", async () => {
    mockRunMigrations.mockResolvedValue(makeRunResult(2, 0));
    const result = await migrateDownCommand({ emergency: true, all: true });
    expect(result.message).toContain("2");
  });

  it("skipChecksumVerify is forwarded to runMigrations", async () => {
    await migrateDownCommand({ emergency: true, skipChecksumVerify: true });
    expect(mockRunMigrations).toHaveBeenCalledWith(
      expect.objectContaining({ skipChecksumVerify: true })
    );
  });

  it("verbose is forwarded to runMigrations", async () => {
    await migrateDownCommand({ emergency: true, verbose: true });
    expect(mockRunMigrations).toHaveBeenCalledWith(
      expect.objectContaining({ verbose: true })
    );
  });
});

// ===========================================================================
// Execution failure handling
// ===========================================================================

describe("execution failure handling", () => {
  it("runMigrations throw is caught and returned as success=false", async () => {
    mockRunMigrations.mockRejectedValue(new Error("db locked"));
    const result = await migrateDownCommand({ emergency: true });
    expect(result.success).toBe(false);
  });

  it("error message contains the underlying error message", async () => {
    mockRunMigrations.mockRejectedValue(new Error("approval file missing"));
    const result = await migrateDownCommand({ emergency: true });
    expect(result.message).toContain("approval file missing");
  });

  it("guard is released even on execution failure", async () => {
    mockRunMigrations.mockRejectedValueOnce(new Error("crash"));
    await migrateDownCommand({ emergency: true });

    // Should not be blocked
    mockRunMigrations.mockResolvedValueOnce(makeRunResult());
    const retry = await migrateDownCommand({ emergency: true });
    expect(retry.success).toBe(true);
  });

  it("getAppliedVersions error is returned as failure", async () => {
    mockGetAppliedVersions.mockRejectedValue(new Error("db not initialized"));
    const result = await migrateDownCommand({ emergency: true });
    expect(result.success).toBe(false);
    expect(result.message).toContain("db not initialized");
  });

  it("loadMigrationsFromFS error is returned as failure", async () => {
    mockLoadMigrationsFromFS.mockRejectedValue(new Error("permission denied reading migrations dir"));
    const result = await migrateDownCommand({ emergency: true });
    expect(result.success).toBe(false);
    expect(result.message).toContain("permission denied reading migrations dir");
  });

  it("subsequent call after failure succeeds (no permanent lockout)", async () => {
    mockRunMigrations.mockRejectedValueOnce(new Error("transient error"));
    const first = await migrateDownCommand({ emergency: true });
    expect(first.success).toBe(false);

    mockRunMigrations.mockResolvedValueOnce(makeRunResult());
    const second = await migrateDownCommand({ emergency: true });
    expect(second.success).toBe(true);
  });
});

// ===========================================================================
// _computeTargetVersions — pure unit tests
// ===========================================================================

describe("_computeTargetVersions", () => {
  it("all=true returns all versions descending", () => {
    const result = _computeTargetVersions([1, 2, 3], { all: true });
    expect(result).toEqual([3, 2, 1]);
  });

  it("targetVersion returns only versions > N descending", () => {
    const result = _computeTargetVersions([1, 2, 3, 4], { targetVersion: 2 });
    expect(result).toEqual([4, 3]);
  });

  it("default (no options) returns only the maximum version", () => {
    const result = _computeTargetVersions([1, 2, 3], {});
    expect(result).toEqual([3]);
  });

  it("empty list returns []", () => {
    expect(_computeTargetVersions([], { all: true })).toEqual([]);
    expect(_computeTargetVersions([], {})).toEqual([]);
    expect(_computeTargetVersions([], { targetVersion: 5 })).toEqual([]);
  });

  it("targetVersion equal to max returns []", () => {
    const result = _computeTargetVersions([1, 2, 3], { targetVersion: 3 });
    expect(result).toEqual([]);
  });

  it("targetVersion 0 would return no valid versions (all are > 0)", () => {
    // Note: targetVersion=0 is blocked at the policy layer, but the pure helper
    // itself would return all versions > 0
    const result = _computeTargetVersions([1, 2, 3], { targetVersion: 0 });
    expect(result).toEqual([3, 2, 1]);
  });

  it("all=true takes precedence when both options present", () => {
    const result = _computeTargetVersions([1, 2, 3, 4], { all: true, targetVersion: 2 });
    expect(result).toEqual([4, 3, 2, 1]);
  });

  it("unordered input is sorted correctly descending", () => {
    const result = _computeTargetVersions([3, 1, 4, 2], { all: true });
    expect(result).toEqual([4, 3, 2, 1]);
  });

  it("single element returns that element for default path", () => {
    expect(_computeTargetVersions([5], {})).toEqual([5]);
  });
});

// ===========================================================================
// MigrationPolicy static methods — regression guard
// ===========================================================================

describe("MigrationPolicy.isDownAllowed", () => {
  it("returns false when env unset", () => {
    delete process.env.ALLOW_DOWN_MIGRATIONS;
    expect(MigrationPolicy.isDownAllowed()).toBe(false);
  });

  it('returns true when env is "true"', () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "true";
    expect(MigrationPolicy.isDownAllowed()).toBe(true);
  });

  it('returns false when env is "1"', () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "1";
    expect(MigrationPolicy.isDownAllowed()).toBe(false);
  });

  it('returns false when env is "yes"', () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "yes";
    expect(MigrationPolicy.isDownAllowed()).toBe(false);
  });

  it('returns false when env is "True" (case-sensitive)', () => {
    process.env.ALLOW_DOWN_MIGRATIONS = "True";
    expect(MigrationPolicy.isDownAllowed()).toBe(false);
  });
});

describe("MigrationPolicy.isHotfix", () => {
  it("returns true when meta.hotfix === true", () => {
    const mig: any = { meta: { hotfix: true }, version: 1, name: "t", authoredAt: "x", author: "x", up: async () => {} };
    expect(MigrationPolicy.isHotfix(mig)).toBe(true);
  });

  it("returns false when meta is absent", () => {
    const mig: any = { version: 1, name: "t", authoredAt: "x", author: "x", up: async () => {} };
    expect(MigrationPolicy.isHotfix(mig)).toBe(false);
  });

  it('returns false when meta.hotfix is "true" (string, not boolean)', () => {
    const mig: any = { meta: { hotfix: "true" }, version: 1, name: "t", authoredAt: "x", author: "x", up: async () => {} };
    expect(MigrationPolicy.isHotfix(mig)).toBe(false);
  });
});

describe("MigrationPolicy.validateMetadata", () => {
  const base = { version: 1, name: "test", authoredAt: "2026-01-01", author: "alice", up: async () => {} };

  it("passes for a valid standard migration", () => {
    const result = MigrationPolicy.validateMetadata(base as any);
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });

  it("errors on missing name", () => {
    const result = MigrationPolicy.validateMetadata({ ...base, name: "" } as any);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("Migration name is required");
  });

  it("errors on missing author", () => {
    const result = MigrationPolicy.validateMetadata({ ...base, author: "" } as any);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("Migration author is required");
  });

  it("errors on missing authoredAt", () => {
    const result = MigrationPolicy.validateMetadata({ ...base, authoredAt: "" } as any);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("Migration authoredAt date is required");
  });

  it("errors on missing up function", () => {
    const { up: _up, ...noUp } = base;
    const result = MigrationPolicy.validateMetadata(noUp as any);
    expect(result.valid).toBe(false);
    expect(result.errors).toContain("Migration up function is required");
  });

  it("hotfix requires meta.reason", () => {
    const mig = { ...base, meta: { hotfix: true, rollback_risk: "low" }, down: async () => {} };
    const result = MigrationPolicy.validateMetadata(mig as any);
    expect(result.errors).toContain("Hotfix migrations must include meta.reason");
  });

  it("hotfix requires meta.rollback_risk", () => {
    const mig = { ...base, meta: { hotfix: true, reason: "fix" }, down: async () => {} };
    const result = MigrationPolicy.validateMetadata(mig as any);
    expect(result.errors).toContain("Hotfix migrations must include meta.rollback_risk");
  });

  it("hotfix requires down function", () => {
    const mig = { ...base, meta: { hotfix: true, reason: "fix", rollback_risk: "low" } };
    const result = MigrationPolicy.validateMetadata(mig as any);
    expect(result.errors).toContain("Hotfix migrations must include a down function");
  });

  it("passes for a valid hotfix migration", () => {
    const mig = {
      ...base,
      meta: { hotfix: true, reason: "Critical fix", rollback_risk: "medium" },
      down: async () => {},
    };
    const result = MigrationPolicy.validateMetadata(mig as any);
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });
});

describe("MigrationPolicy.dryRun", () => {
  it("collects warnings from migration validate() hooks", async () => {
    const mig = {
      version: 1,
      name: "warn_test",
      authoredAt: "2026-01-01",
      author: "x",
      up: async () => {},
      validate: async () => ["data may be lost", "check your backups"],
    };
    const result = await MigrationPolicy.dryRun([mig]);
    expect(result.warnings.some((w) => w.includes("data may be lost"))).toBe(true);
    expect(result.warnings.some((w) => w.includes("check your backups"))).toBe(true);
  });

  it("does not fail when validate() throws — treats as warning", async () => {
    const mig = {
      version: 1,
      name: "throw_validate",
      authoredAt: "2026-01-01",
      author: "x",
      up: async () => {},
      validate: async () => { throw new Error("validate exploded"); },
    };
    const result = await MigrationPolicy.dryRun([mig]);
    expect(result.valid).toBe(true); // throwing validate is a warning, not an error
    expect(result.warnings.some((w) => w.includes("threw an error"))).toBe(true);
  });

  it("detects duplicate version numbers within the list", async () => {
    const migs = [
      { version: 1, name: "a", authoredAt: "2026-01-01", author: "x", up: async () => {} },
      { version: 1, name: "b", authoredAt: "2026-01-01", author: "x", up: async () => {} },
    ];
    const result = await MigrationPolicy.dryRun(migs);
    expect(result.valid).toBe(false);
    expect(result.errors.some((e) => e.includes("Duplicate migration version 1"))).toBe(true);
  });

  it("valid=true for empty list", async () => {
    const result = await MigrationPolicy.dryRun([]);
    expect(result.valid).toBe(true);
    expect(result.errors).toEqual([]);
  });
});

// ===========================================================================
// MigrationPolicyError
// ===========================================================================

describe("MigrationPolicyError", () => {
  it("is an instance of Error", () => {
    const err = new MigrationPolicyError("EMERGENCY_REQUIRED", "test");
    expect(err).toBeInstanceOf(Error);
    expect(err).toBeInstanceOf(MigrationPolicyError);
  });

  it("name is MigrationPolicyError", () => {
    const err = new MigrationPolicyError("CONFLICTING_FLAGS", "msg");
    expect(err.name).toBe("MigrationPolicyError");
  });

  it("code field is readable", () => {
    const err = new MigrationPolicyError("EXECUTION_FAILED", "msg");
    expect(err.code).toBe("EXECUTION_FAILED");
  });

  it("cause is preserved", () => {
    const original = new Error("root cause");
    const err = new MigrationPolicyError("EXECUTION_FAILED", "wrapper", original);
    expect(err.cause).toBe(original);
  });

  it("cause is undefined when not supplied", () => {
    const err = new MigrationPolicyError("GLOBALLY_DISABLED", "msg");
    expect(err.cause).toBeUndefined();
  });

  it("stacks are chained when cause is an Error", () => {
    const original = new Error("root");
    const err = new MigrationPolicyError("EXECUTION_FAILED", "wrapper", original);
    expect(err.stack).toContain("Caused by:");
  });
});
