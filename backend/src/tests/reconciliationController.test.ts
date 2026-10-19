/**
 * Deterministic failure-boundary coverage for `triggerBackfill`
 * (backend/src/controllers/v1/reconciliation.ts).
 *
 * The handler is the HTTP entry point for `POST /api/v1/reconciliation/backfill`
 * and is the only place that decides whether a drift report is safe to replay
 * through the backfill service. The boundaries it enforces are documented as
 * B1..B6 in the controller; each `describe` block below names the boundary it
 * pins down so a regression points straight at the invariant that broke.
 *
 * The suite is hermetic and clock-free:
 *   - the report fixtures carry a fixed timestamp, so the run id the backfill
 *     service would derive (`drift_<timestamp>`) is always the same value;
 *   - `ReconciliationWorker` is spied rather than exercised, so no RPC, no
 *     database file, and no `setTimeout` is involved;
 *   - concurrency is ordered by an explicitly resolved promise instead of
 *     timers, so the interleaving is fixed by the test, not by the scheduler.
 *
 * Route-level authorization (`requireAdminRoles`) is intentionally not mounted
 * here: `src/middleware/rbac.ts` transitively imports `src/services/
 * api-key-service.ts`, which does not currently resolve (`cypto`,
 * `./api-key-errors`). That is a pre-existing defect on `main` unrelated to this
 * handler, so this suite stays at the controller boundary it owns.
 */

import { NextFunction, Request, Response } from "express";
import { triggerBackfill } from "../controllers/v1/reconciliation";
import { errorHandler } from "../middleware/error-handler";
import { ReconciliationWorker } from "../services/reconciliationWorker";
import { BackfillResult, DriftItem, DriftReport } from "../types/reconciliation";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

// Fixed epoch seconds: no test may depend on the wall clock.
const TIMESTAMP = 1_800_000_000;

const drift = (id: string): DriftItem => ({
  id,
  type: "Invoice",
  driftType: "MISSING",
});

const report = (overrides: Record<string, unknown> = {}): DriftReport =>
  ({
    timestamp: TIMESTAMP,
    totalRecordsChecked: 3,
    driftCount: 2,
    drifts: [drift("invoice_1"), drift("invoice_2")],
    ...overrides,
  }) as unknown as DriftReport;

const summary = (overrides: Partial<BackfillResult> = {}): BackfillResult => ({
  successCount: 2,
  failCount: 0,
  errors: [],
  ...overrides,
});

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

type MockRes = Response & { status: jest.Mock; json: jest.Mock; headersSent: boolean };

const makeRes = (): MockRes => {
  const res: Partial<MockRes> = { headersSent: false };
  res.status = jest.fn().mockReturnValue(res as Response);
  res.json = jest.fn().mockReturnValue(res as Response);
  return res as MockRes;
};

const makeReq = (): Request => ({}) as Request;

interface Invocation {
  res: MockRes;
  next: jest.Mock;
  /** Body handed to `res.json`, or undefined when nothing was written. */
  body: unknown;
  /** Status handed to `res.status`, or undefined when `res.status` was skipped. */
  status: number | undefined;
}

const invoke = async (): Promise<Invocation> => {
  const res = makeRes();
  const next = jest.fn();
  await triggerBackfill(makeReq(), res, next as unknown as NextFunction);
  const statusCall = res.status.mock.calls[0];
  return {
    res,
    next,
    status: statusCall ? statusCall[0] : undefined,
    body: res.json.mock.calls[0] ? res.json.mock.calls[0][0] : undefined,
  };
};

/** Renders a forwarded error the way the app's shared error handler would. */
const renderThroughErrorHandler = (err: Error): { status: number | undefined; body: any } => {
  const res = makeRes();
  const log = jest.spyOn(console, "error").mockImplementation(() => undefined);
  try {
    errorHandler(err as any, makeReq(), res, jest.fn() as unknown as NextFunction);
  } finally {
    log.mockRestore();
  }
  return { status: res.status.mock.calls[0][0], body: res.json.mock.calls[0][0] };
};

type WorkerSpy = jest.SpyInstance & {
  mockReturnValue: (value: any) => unknown;
  mockResolvedValue: (value: any) => unknown;
  mockRejectedValue: (value: any) => unknown;
  mockRejectedValueOnce: (value: any) => unknown;
  mockImplementation: (fn: (...args: any[]) => any) => unknown;
  mockReset: () => unknown;
};

describe("triggerBackfill", () => {
  let getLatestReport: WorkerSpy;
  let triggerBoundedBackfill: WorkerSpy;
  let warn: jest.SpyInstance;
  let err: jest.SpyInstance;

  beforeEach(() => {
    warn = jest.spyOn(console, "warn").mockImplementation(() => undefined);
    err = jest.spyOn(console, "error").mockImplementation(() => undefined);
    getLatestReport = jest.spyOn(ReconciliationWorker, "getLatestReport") as WorkerSpy;
    triggerBoundedBackfill = jest.spyOn(
      ReconciliationWorker,
      "triggerBoundedBackfill",
    ) as WorkerSpy;
    getLatestReport.mockReset();
    triggerBoundedBackfill.mockReset();
  });

  afterEach(() => {
    warn.mockRestore();
    err.mockRestore();
    jest.restoreAllMocks();
  });

  // -------------------------------------------------------------------------
  // B1 — success: a well-formed report is relayed verbatim
  // -------------------------------------------------------------------------
  describe("B1 — success path", () => {
    it("returns the backfill summary with an implicit 200 and never calls next", async () => {
      const latest = report();
      const done = summary();
      getLatestReport.mockReturnValue(latest);
      triggerBoundedBackfill.mockResolvedValue(done);

      const { res, next, body } = await invoke();

      // The pre-existing contract is `res.json(result)` with no explicit
      // status; callers of this endpoint already depend on that 200.
      expect(res.status).not.toHaveBeenCalled();
      expect(res.json).toHaveBeenCalledTimes(1);
      expect(body).toEqual({ successCount: 2, failCount: 0, errors: [] });
      expect(body).toBe(done);
      expect(next).not.toHaveBeenCalled();
    });

    it("replays the exact report the worker returned, without copying or mutating it", async () => {
      const latest = report();
      getLatestReport.mockReturnValue(latest);
      triggerBoundedBackfill.mockResolvedValue(summary());

      await invoke();

      // Reference equality: a defensive clone here would let a later mutation of
      // the worker's report diverge from what the service actually consumed.
      expect(getLatestReport).toHaveBeenCalledTimes(1);
      expect(triggerBoundedBackfill).toHaveBeenCalledTimes(1);
      expect(triggerBoundedBackfill.mock.calls[0][0]).toBe(latest);
      expect(latest.drifts).toHaveLength(2);
    });

    it("relays a zeroed summary when the report contains no drift", async () => {
      getLatestReport.mockReturnValue(report({ driftCount: 0, drifts: [], totalRecordsChecked: 3 }));
      triggerBoundedBackfill.mockResolvedValue(summary({ successCount: 0 }));

      const { res, next, body } = await invoke();

      expect(res.status).not.toHaveBeenCalled();
      expect(body).toEqual({ successCount: 0, failCount: 0, errors: [] });
      expect(next).not.toHaveBeenCalled();
    });

    it("relays a partially failed replay with 200 and the per-record errors", async () => {
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockResolvedValue(
        summary({
          successCount: 1,
          failCount: 1,
          errors: ["Failed to backfill invoice_2: Simulated failure"],
        }),
      );

      const { res, next, body } = await invoke();

      // Partial failure is a completed run, not a transport error: the caller
      // needs the counts, so it is reported as 200 with an error list.
      expect(res.status).not.toHaveBeenCalled();
      expect(body).toEqual({
        successCount: 1,
        failCount: 1,
        errors: ["Failed to backfill invoice_2: Simulated failure"],
      });
      expect(next).not.toHaveBeenCalled();
    });

    it("is idempotent for a replayed report: the zeroed summary of a finished run is still 200", async () => {
      // `backfillService` short-circuits an already-completed run to an empty
      // summary. Retrying must not be mistaken for a fault.
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockResolvedValue(summary({ successCount: 0, failCount: 0 }));

      const first = await invoke();
      const second = await invoke();

      expect(first.status).toBeUndefined();
      expect(second.body).toEqual({ successCount: 0, failCount: 0, errors: [] });
      expect(triggerBoundedBackfill).toHaveBeenCalledTimes(2);
    });
  });

  // -------------------------------------------------------------------------
  // B1 — refusal: no report has been produced yet
  // -------------------------------------------------------------------------
  describe("B1 — no report available", () => {
    it("returns 400 with the unchanged legacy body and never starts a backfill", async () => {
      getLatestReport.mockReturnValue(null);

      const { res, next, status, body } = await invoke();

      // Body and status are part of the published contract for this endpoint.
      expect(status).toBe(400);
      expect(body).toEqual({ error: "No drift report available. Run reconciliation first." });
      expect(triggerBoundedBackfill).not.toHaveBeenCalled();
      expect(next).not.toHaveBeenCalled();
    });
  });

  // -------------------------------------------------------------------------
  // B3 — refusal: the latest report is a reconciliation failure record
  // -------------------------------------------------------------------------
  describe("B3 — latest report is a reconciliation failure", () => {
    it("returns 409 and does not replay a report that recorded a failure", async () => {
      getLatestReport.mockReturnValue(
        report({
          totalRecordsChecked: 0,
          driftCount: 0,
          drifts: [],
          error: "connect ECONNREFUSED 10.0.0.5:5432",
        }),
      );

      const { next, status, body } = await invoke();

      expect(status).toBe(409);
      expect(body).toEqual({
        error: "Latest drift report is a reconciliation failure. Re-run reconciliation before backfilling.",
      });
      expect(triggerBoundedBackfill).not.toHaveBeenCalled();
      expect(next).not.toHaveBeenCalled();
    });

    it("does not echo the underlying reconciliation error to the caller", async () => {
      getLatestReport.mockReturnValue(
        report({ driftCount: 0, drifts: [], error: "rpc failed for /home/user/.config/quicklendx/keys.json" }),
      );

      const { status, body } = await invoke();

      // The operator is pointed at the fix, not at internal transport detail.
      expect(status).toBe(409);
      expect(JSON.stringify(body)).not.toContain("keys.json");
      expect(JSON.stringify(body)).not.toContain("ECONNREFUSED");
    });

    it("still replays a report whose `error` field is empty", async () => {
      getLatestReport.mockReturnValue(report({ error: "" }));
      triggerBoundedBackfill.mockResolvedValue(summary());

      const { res, status, body } = await invoke();

      expect(status).toBeUndefined();
      expect(body).toEqual({ successCount: 2, failCount: 0, errors: [] });
      expect(res.status).not.toHaveBeenCalled();
    });
  });

  // -------------------------------------------------------------------------
  // B2 — refusal: structurally unusable report
  // -------------------------------------------------------------------------
  describe("B2 — malformed report is rejected before any state is written", () => {
    const cases: Array<{ name: string; overrides: Record<string, unknown>; error: string }> = [
      {
        name: "a zero timestamp (placeholder report, collides with run id drift_0)",
        overrides: { timestamp: 0 },
        error: "Drift report has no usable timestamp.",
      },
      {
        name: "a missing timestamp",
        overrides: { timestamp: undefined },
        error: "Drift report has no usable timestamp.",
      },
      {
        name: "a non-integer timestamp",
        overrides: { timestamp: 1.5 },
        error: "Drift report has no usable timestamp.",
      },
      {
        name: "a NaN timestamp",
        overrides: { timestamp: Number.NaN },
        error: "Drift report has no usable timestamp.",
      },
      {
        name: "an infinite timestamp",
        overrides: { timestamp: Number.POSITIVE_INFINITY },
        error: "Drift report has no usable timestamp.",
      },
      {
        name: "a drifts value that is not an array",
        overrides: { drifts: { 0: drift("invoice_1") } },
        error: "Drift report drift list is not an array.",
      },
      {
        name: "a missing drifts value",
        overrides: { drifts: undefined },
        error: "Drift report drift list is not an array.",
      },
      {
        name: "a negative drift count",
        overrides: { driftCount: -1 },
        error: "Drift report drift count is not a whole number.",
      },
      {
        name: "a fractional drift count",
        overrides: { driftCount: 1.5 },
        error: "Drift report drift count is not a whole number.",
      },
      {
        name: "a drift count lower than the reported list",
        overrides: { driftCount: 1 },
        error: "Drift report drift count does not match the reported drift list.",
      },
      {
        name: "a drift count higher than the reported list",
        overrides: { driftCount: 3 },
        error: "Drift report drift count does not match the reported drift list.",
      },
    ];

    it.each(cases)("returns 422 for $name", async ({ overrides, error }) => {
      getLatestReport.mockReturnValue(report(overrides));

      const { next, status, body } = await invoke();

      expect(status).toBe(422);
      expect(body).toEqual({ error });
      expect(triggerBoundedBackfill).not.toHaveBeenCalled();
      expect(next).not.toHaveBeenCalled();
    });

    it("records the rejection in the log so the refusal is diagnosable", async () => {
      getLatestReport.mockReturnValue(report({ timestamp: 0 }));

      await invoke();

      expect(warn).toHaveBeenCalledTimes(1);
      expect(warn.mock.calls[0][0]).toContain("[TriggerBackfill]");
      expect(warn.mock.calls[0][0]).toContain("Drift report has no usable timestamp.");
    });

    it("accepts the smallest valid timestamp boundary", async () => {
      getLatestReport.mockReturnValue(report({ timestamp: 1, driftCount: 0, drifts: [] }));
      triggerBoundedBackfill.mockResolvedValue(summary({ successCount: 0 }));

      const { res, next } = await invoke();

      expect(res.status).not.toHaveBeenCalled();
      expect(triggerBoundedBackfill).toHaveBeenCalledTimes(1);
      expect(next).not.toHaveBeenCalled();
    });
  });

  // -------------------------------------------------------------------------
  // B5 — refusal: the service returned something that cannot be relayed
  // -------------------------------------------------------------------------
  describe("B5 — unusable backfill summary is never answered with 200", () => {
    const cases: Array<{ name: string; result: unknown; error: string }> = [
      {
        name: "an undefined result",
        result: undefined,
        error: "Backfill service returned no result.",
      },
      {
        name: "a null result",
        result: null,
        error: "Backfill service returned no result.",
      },
      {
        name: "a non-object result",
        result: "done",
        error: "Backfill service returned no result.",
      },
      {
        name: "a missing success count",
        result: { failCount: 0, errors: [] },
        error: "Backfill service returned a non-numeric record count.",
      },
      {
        name: "a negative success count",
        result: { successCount: -1, failCount: 0, errors: [] },
        error: "Backfill service returned a non-numeric record count.",
      },
      {
        name: "a fractional success count",
        result: { successCount: 1.5, failCount: 0, errors: [] },
        error: "Backfill service returned a non-numeric record count.",
      },
      {
        name: "a NaN success count",
        result: { successCount: Number.NaN, failCount: 0, errors: [] },
        error: "Backfill service returned a non-numeric record count.",
      },
      {
        name: "an infinite fail count",
        result: { successCount: 0, failCount: Number.POSITIVE_INFINITY, errors: [] },
        error: "Backfill service returned a non-numeric record count.",
      },
      {
        name: "a missing error list",
        result: { successCount: 1, failCount: 1 },
        error: "Backfill service returned a malformed error list.",
      },
      {
        name: "an error list that is not an array",
        result: { successCount: 1, failCount: 1, errors: "boom" },
        error: "Backfill service returned a malformed error list.",
      },
      {
        name: "an error list holding non-string entries",
        result: { successCount: 1, failCount: 1, errors: [{ reason: "boom" }] },
        error: "Backfill service returned a malformed error list.",
      },
      {
        name: "more records processed than the report contains",
        result: { successCount: 3, failCount: 0, errors: [] },
        error: "Backfill service reported more records processed than the report contains.",
      },
      {
        name: "success and failure counts that together exceed the report",
        result: { successCount: 2, failCount: 2, errors: [] },
        error: "Backfill service reported more records processed than the report contains.",
      },
    ];

    it.each(cases)("returns 502 for $name", async ({ result, error }) => {
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockResolvedValue(result as BackfillResult);

      const { res, next, status, body } = await invoke();

      expect(status).toBe(502);
      expect(body).toEqual({ error });
      // 200 would be indistinguishable from "nothing needed backfilling".
      expect(res.status).toHaveBeenCalledTimes(1);
      expect(next).not.toHaveBeenCalled();
    });

    it("does not echo the rejected payload to the caller", async () => {
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockResolvedValue({
        successCount: 1,
        failCount: 1,
        errors: [{ token: "sk-live-do-not-leak" }],
      } as unknown as BackfillResult);

      const { body } = await invoke();

      expect(JSON.stringify(body)).not.toContain("sk-live-do-not-leak");
      expect(body).toEqual({ error: "Backfill service returned a malformed error list." });
    });

    it("logs the report identity and reason but not the rejected payload", async () => {
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockResolvedValue({
        successCount: 9,
        failCount: 0,
        errors: [],
        note: "sk-live-do-not-leak",
      } as unknown as BackfillResult);

      await invoke();

      expect(err).toHaveBeenCalledTimes(1);
      const [prefix, context] = err.mock.calls[0];
      expect(prefix).toContain("[TriggerBackfill]");
      expect(context).toEqual({
        report_timestamp: TIMESTAMP,
        drift_count: 2,
        reason: "Backfill service reported more records processed than the report contains.",
      });
      expect(JSON.stringify(context)).not.toContain("sk-live-do-not-leak");
    });

    it("accepts a summary that accounts for exactly the whole report", async () => {
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockResolvedValue(summary({ successCount: 1, failCount: 1 }));

      const { res } = await invoke();

      expect(res.status).not.toHaveBeenCalled();
    });
  });

  // -------------------------------------------------------------------------
  // B4 — concurrency
  // -------------------------------------------------------------------------
  describe("B4 — concurrent execution", () => {
    it("admits one backfill and refuses the overlapping one with 409", async () => {
      let release: () => void = () => undefined;
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockReturnValue(gate.then(() => summary()));

      const firstRes = makeRes();
      const firstNext = jest.fn();
      const first = triggerBackfill(
        makeReq(),
        firstRes,
        firstNext as unknown as NextFunction,
      );

      // `triggerBackfill` runs synchronously up to the first await, so the lock
      // is already held here — no timer, no scheduler dependence.
      const second = await invoke();

      expect(second.status).toBe(409);
      expect(second.body).toEqual({
        error: "A reconciliation backfill is already in progress.",
      });
      expect(second.next).not.toHaveBeenCalled();
      expect(triggerBoundedBackfill).toHaveBeenCalledTimes(1);

      release();
      await first;

      expect(firstNext).not.toHaveBeenCalled();
      expect(firstRes.json).toHaveBeenCalledTimes(1);
      expect(firstRes.json.mock.calls[0][0]).toEqual({
        successCount: 2,
        failCount: 0,
        errors: [],
      });
    });

    it("does not double-replay when several requests race the same report", async () => {
      getLatestReport.mockReturnValue(report());
      let release: () => void = () => undefined;
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      triggerBoundedBackfill.mockReturnValue(gate.then(() => summary()));

      const racers = [0, 1, 2, 3, 4].map(() => {
        const res = makeRes();
        const next = jest.fn();
        return {
          done: triggerBackfill(makeReq(), res, next as unknown as NextFunction),
          res,
          next,
        };
      });
      release();
      await Promise.all(racers.map((r) => r.done));

      // Exactly one replay reached the service: the progress row it read-modify-
      // writes is not transactional, so a second writer would re-walk the slice.
      expect(triggerBoundedBackfill).toHaveBeenCalledTimes(1);
      const conflicts = racers.filter((r) => r.res.status.mock.calls[0]?.[0] === 409);
      expect(conflicts).toHaveLength(4);
      for (const loser of conflicts) {
        expect(loser.res.json.mock.calls[0][0]).toEqual({
          error: "A reconciliation backfill is already in progress.",
        });
        expect(loser.next).not.toHaveBeenCalled();
      }
    });

    it("releases the lock so a later request can still run", async () => {
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockResolvedValue(summary());

      await invoke();
      const afterFirst = await invoke();

      expect(afterFirst.status).toBeUndefined();
      expect(triggerBoundedBackfill).toHaveBeenCalledTimes(2);
    });

    it("releases the lock when the replay fails, so a retry is not blocked", async () => {
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockRejectedValueOnce(new Error("database is locked"));
      triggerBoundedBackfill.mockResolvedValueOnce(summary());

      const failed = await invoke();
      const retried = await invoke();

      expect(failed.next).toHaveBeenCalledTimes(1);
      expect(retried.status).toBeUndefined();
      expect(retried.body).toEqual({ successCount: 2, failCount: 0, errors: [] });
      expect(triggerBoundedBackfill).toHaveBeenCalledTimes(2);
    });

    it("does not let a request rejected before the lock release someone else's lock", async () => {
      let release: () => void = () => undefined;
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockReturnValue(gate.then(() => summary()));

      const holderRes = makeRes();
      const holderDone = triggerBackfill(
        makeReq(),
        holderRes,
        jest.fn() as unknown as NextFunction,
      );

      // A refusal on an earlier boundary runs the `finally` too; it must not
      // clear a lock it never took, or the next request would double-replay.
      getLatestReport.mockReturnValue(null);
      const noReport = await invoke();
      expect(noReport.status).toBe(400);

      getLatestReport.mockReturnValue(report());
      const blocked = await invoke();

      expect(blocked.status).toBe(409);
      expect(triggerBoundedBackfill).toHaveBeenCalledTimes(1);

      release();
      await holderDone;
    });
  });

  // -------------------------------------------------------------------------
  // B6 — failures reach the shared error handler intact
  // -------------------------------------------------------------------------
  describe("B6 — error forwarding", () => {
    it("preserves the status and code a validation error already carries", async () => {
      const validation = Object.assign(new Error("Requested batch size exceeds maximum of 100"), {
        code: "MAX_BATCH_SIZE_EXCEEDED",
        status: 422,
      });
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockRejectedValue(validation);

      const { res, next } = await invoke();

      expect(next).toHaveBeenCalledTimes(1);
      expect(next.mock.calls[0][0]).toBe(validation);
      expect(res.json).not.toHaveBeenCalled();
    });

    it("bridges `statusCode` (BackfillError) to the `status` the error handler reads", async () => {
      // `BackfillError` publishes `statusCode`; `errorHandler` reads `status`.
      // Without the bridge the client would see an opaque 500 for a 422.
      const backfillError = Object.assign(new Error("batchSize must be greater than 0"), {
        code: "INVALID_BATCH_SIZE",
        statusCode: 400,
      });
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockRejectedValue(backfillError);

      const { next } = await invoke();

      const forwarded = next.mock.calls[0][0];
      expect(forwarded.status).toBe(400);
      const rendered = renderThroughErrorHandler(forwarded);
      expect(rendered.status).toBe(400);
      expect(rendered.body.error.code).toBe("INVALID_BATCH_SIZE");
    });

    it("renders a replay failure through the shared error handler as a diagnosable 500", async () => {
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockRejectedValue(new Error("SQLITE_BUSY: database is locked"));

      const { next } = await invoke();

      const rendered = renderThroughErrorHandler(next.mock.calls[0][0]);
      expect(rendered.status).toBe(500);
      expect(rendered.body.error.code).toBe("BACKFILL_FAILED");
      expect(rendered.body.error.message).toBe("SQLITE_BUSY: database is locked");
    });

    it("labels a replay failure that carries no code of its own", async () => {
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockRejectedValue(new Error("connection reset"));

      const { next } = await invoke();

      expect(renderThroughErrorHandler(next.mock.calls[0][0]).body.error.code).toBe(
        "BACKFILL_FAILED",
      );
    });

    it("keeps an explicit status even when the error also has a statusCode", async () => {
      const err = Object.assign(new Error("conflict"), {
        code: "RUN_NOT_RESUMABLE",
        status: 409,
        statusCode: 400,
      });
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockRejectedValue(err);

      const { next } = await invoke();

      expect(next.mock.calls[0][0].status).toBe(409);
    });

    it.each([
      { name: "a string", thrown: "boom" },
      { name: "null", thrown: null },
      { name: "undefined", thrown: undefined },
      { name: "a number", thrown: 500 },
      { name: "a plain object", thrown: { message: "boom" } },
    ])("promotes a non-Error throw ($name) so Express cannot read it as success", async ({
      thrown,
    }) => {
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockImplementation(() => {
        throw thrown;
      });

      const { res, next } = await invoke();

      // `next(null)`/`next(undefined)` means "no error" in Express, which would
      // leave the request hanging until the 404 handler ran.
      expect(next).toHaveBeenCalledTimes(1);
      const forwarded = next.mock.calls[0][0];
      expect(forwarded).toBeInstanceOf(Error);
      expect(forwarded.status).toBe(500);
      expect(forwarded.code).toBe("BACKFILL_FAILED");
      expect(res.json).not.toHaveBeenCalled();
    });

    it("replaces a non-finite status with 500 instead of writing an invalid response", async () => {
      const err = Object.assign(new Error("boom"), { status: Number.NaN });
      getLatestReport.mockReturnValue(report());
      triggerBoundedBackfill.mockRejectedValue(err);

      const { next } = await invoke();

      expect(renderThroughErrorHandler(next.mock.calls[0][0]).status).toBe(500);
    });

    it("forwards a failure raised while reading the latest report", async () => {
      getLatestReport.mockImplementation(() => {
        throw new Error("report store unavailable");
      });

      const { res, next, body } = await invoke();

      expect(next).toHaveBeenCalledTimes(1);
      expect(renderThroughErrorHandler(next.mock.calls[0][0]).body.error.message).toBe(
        "report store unavailable",
      );
      expect(res.json).not.toHaveBeenCalled();
      expect(body).toBeUndefined();
    });

    it("logs instead of forwarding when the response was already sent", async () => {
      getLatestReport.mockReturnValue(report());
      const res = makeRes();
      res.headersSent = true;
      res.json.mockImplementation(() => {
        throw new Error("socket hang up");
      });
      triggerBoundedBackfill.mockResolvedValue(summary());
      const next = jest.fn();

      await triggerBackfill(makeReq(), res, next as unknown as NextFunction);

      // Forwarding here would make Express attempt a second write.
      expect(next).not.toHaveBeenCalled();
      expect(err).toHaveBeenCalledTimes(1);
      expect(err.mock.calls[0][0]).toContain("[TriggerBackfill]");
    });
  });

  // -------------------------------------------------------------------------
  // Determinism / regression
  // -------------------------------------------------------------------------
  describe("determinism", () => {
    it("produces the same verdict and body for identical inputs", async () => {
      const latest = report();
      getLatestReport.mockReturnValue(latest);
      triggerBoundedBackfill.mockResolvedValue(summary());

      const runs = [];
      for (let i = 0; i < 5; i += 1) {
        runs.push(await invoke());
      }

      for (const run of runs) {
        expect(run.status).toBeUndefined();
        expect(run.body).toEqual({ successCount: 2, failCount: 0, errors: [] });
        expect(run.next).not.toHaveBeenCalled();
      }
      for (const call of triggerBoundedBackfill.mock.calls) {
        expect(call[0]).toBe(latest);
      }
    });

    it("produces the same 400 verdict on every call once no report exists", async () => {
      getLatestReport.mockReturnValue(null);

      for (let i = 0; i < 3; i += 1) {
        const run = await invoke();
        expect(run.status).toBe(400);
        expect(run.body).toEqual({ error: "No drift report available. Run reconciliation first." });
      }
      expect(triggerBoundedBackfill).not.toHaveBeenCalled();
    });

    it("never writes a body on more than one channel for a single request", async () => {
      getLatestReport.mockReturnValue(report({ driftCount: 9 }));
      triggerBoundedBackfill.mockResolvedValue(summary());

      const { res, next } = await invoke();

      // Either a status+body pair, or a forwarded error — never both.
      expect(res.status).toHaveBeenCalledTimes(1);
      expect(res.json).toHaveBeenCalledTimes(1);
      expect(next).not.toHaveBeenCalled();
    });
  });
});
