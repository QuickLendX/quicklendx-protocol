import { backfillService, BackfillError } from "../services/backfillService";

describe("BackfillService branch coverage", () => {
  beforeEach(async () => {
    process.env.BACKFILL_MAX_LEDGER_RANGE = "50";
    process.env.BACKFILL_MAX_CONCURRENCY = "2";
    await backfillService.resetForTests();
  });

  it("returns null for missing run and empty runs list", () => {
    expect(backfillService.getRun("missing")).toBeNull();
    expect(backfillService.listRuns()).toEqual([]);
  });

  it("throws when range is invalid", async () => {
    await expect(
      backfillService.startBackfill(
        { startLedger: 10, endLedger: 5, dryRun: false, concurrency: 1 },
        "ops",
      ),
    ).rejects.toMatchObject<Partial<BackfillError>>({ code: "INVALID_LEDGER_RANGE" });
  });

  it("uses default max range when env is invalid", async () => {
    process.env.BACKFILL_MAX_LEDGER_RANGE = "not-a-number";
    await expect(
      backfillService.startBackfill(
        { startLedger: 1, endLedger: 6001, dryRun: false, concurrency: 1 },
        "ops",
      ),
    ).rejects.toMatchObject<Partial<BackfillError>>({ code: "MAX_RANGE_EXCEEDED" });
  });

  it("uses default max concurrency when env is invalid", async () => {
    process.env.BACKFILL_MAX_CONCURRENCY = "NaN";
    await expect(
      backfillService.startBackfill(
        { startLedger: 1, endLedger: 5, dryRun: false, concurrency: 5 },
        "ops",
      ),
    ).rejects.toMatchObject<Partial<BackfillError>>({ code: "MAX_CONCURRENCY_EXCEEDED" });
  });

  it("throws for pause/resume on missing runs", async () => {
    await expect(backfillService.pauseRun("missing", "ops")).rejects.toMatchObject<
      Partial<BackfillError>
    >({ code: "RUN_NOT_FOUND" });
    await expect(backfillService.resumeRun("missing", "ops")).rejects.toMatchObject<
      Partial<BackfillError>
    >({ code: "RUN_NOT_FOUND" });
  });

  it("throws when pausing a non-running run", async () => {
    const started = await backfillService.startBackfill(
      { startLedger: 1, endLedger: 1, dryRun: false, concurrency: 1 },
      "ops",
    );
    expect(started.run).toBeDefined();
    await new Promise((resolve) => setTimeout(resolve, 15));
    await expect(backfillService.pauseRun(started.run!.id, "ops")).rejects.toMatchObject<
      Partial<BackfillError>
    >({ code: "RUN_NOT_RUNNING" });
  });

  it("throws when resuming a non-resumable run", async () => {
    const started = await backfillService.startBackfill(
      { startLedger: 1, endLedger: 1, dryRun: false, concurrency: 1 },
      "ops",
    );
    await new Promise((resolve) => setTimeout(resolve, 15));
    await expect(backfillService.resumeRun(started.run!.id, "ops")).rejects.toMatchObject<
      Partial<BackfillError>
    >({ code: "RUN_NOT_RESUMABLE" });
  });

  it("covers no-op processRun for unknown run id", async () => {
    await expect((backfillService as any).processRun("missing")).resolves.toBeUndefined();
  });

  it("resumes failed runs and clears previous error", async () => {
    backfillService.setFailureAtLedgerForTests(5);
    const started = await backfillService.startBackfill(
      { startLedger: 1, endLedger: 30, dryRun: false, concurrency: 1 },
      "ops",
    );
    await new Promise((resolve) => setTimeout(resolve, 20));

    const failedRun = backfillService.getRun(started.run!.id);
    expect(failedRun?.status).toBe("failed");
    expect(failedRun?.error).toBeDefined();

    backfillService.setFailureAtLedgerForTests(null);
    const resumed = await backfillService.resumeRun(started.run!.id, "ops");
    expect(resumed.status).toBe("running");
    expect(resumed.error).toBeUndefined();
  });

  it("handles stale idempotency index entries", async () => {
    (backfillService as any).idempotencyIndex.set("stale-key", "missing-run");
    const result = await backfillService.startBackfill(
      { startLedger: 1, endLedger: 3, dryRun: false, concurrency: 1, idempotencyKey: "stale-key" },
      "ops",
    );
    expect(result.idempotentReuse).toBeUndefined();
    expect(result.run).toBeDefined();
  });
});
