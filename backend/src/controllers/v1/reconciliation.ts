import { Request, Response, NextFunction } from "express";
import { ReconciliationWorker } from "../../services/reconciliationWorker";
import { BackfillResult, DriftReport } from "../../types/reconciliation";

const D = 50;
const M = 100;
const V = "v1";

const enc = (i: string) => Buffer.from(V + ":" + i).toString("base64url");

const dec = (c: string) => {
  const d = Buffer.from(c, "base64url").toString();
  const s = d.indexOf(":");
  if (s < 0 || d.slice(0, s) !== V || !d.slice(s + 1)) throw new Error("bad cursor");
  return d.slice(s + 1);
};

const lim = (v: any): number | null => {
  if (v === void 0) return D;
  if (typeof v === "string" && /^\d+$/.test(v.trim())) {
    const n = +v;
    return n > 0 && n <= M ? n : null;
  }
  return null;
};

const cmp = (a: any, b: any) => {
  const x = String(a.id ?? "");
  const y = String(b.id ?? "");
  if (x < y) return -1;
  if (x > y) return 1;
  return (
    new Date(a.createdAt ?? a.created_at ?? 0).getTime() -
    new Date(b.createdAt ?? b.created_at ?? 0).getTime()
  );
};

export const getDriftReports = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const l = lim(req.query.limit);
    if (l === null) return res.status(400).json({ error: "Invalid limit" });
    let a: string | undefined;
    if (req.query.cursor !== undefined) {
      if (typeof req.query.cursor !== "string")
        return res.status(400).json({ error: "Invalid cursor" });
      try {
        a = dec(req.query.cursor);
      } catch {
        return res.status(400).json({ error: "Invalid cursor" });
      }
    }
    const s = [...ReconciliationWorker.getAllReports()].sort(cmp);
    const f = a ? s.filter((r) => String((r as any).id ?? "") > a) : s;
    const m = f.length > l;
    const p = m ? f.slice(0, l) : f;
    res.json({
      data: p,
      pagination: {
        limit: l,
        nextCursor: m ? enc(String((p[p.length - 1] as any).id ?? "")) : null,
        hasMore: m,
      },
    });
  } catch (e) {
    next(e);
  }
};

export const runReconciliation = async (req: Request, res: Response, next: NextFunction) => {
  try {
    if (ReconciliationWorker.isReconciliationRunning()) {
      return res.status(409).json({
        error: {
          code: "CONFLICT",
          message: "Reconciliation already in progress",
        },
      });
    }
    
    const report = await ReconciliationWorker.runReconciliation();
    
    if ('error' in report && report.error) {
      return res.status(502).json({
        error: {
          code: "BAD_GATEWAY",
          message: "Reconciliation failed due to downstream error",
          details: report.error,
        },
      });
    }
    
    res.json(report);
  } catch (e: any) {
    if (e.message === "Reconciliation already in progress") {
      return res.status(409).json({
        error: {
          code: "CONFLICT",
          message: "Reconciliation already in progress",
        },
      });
    }
    next(e);
  }
};

// --- POST /reconciliation/backfill --------------------------------------------
//
// `triggerBackfill` replays the latest drift report through the backfill
// service. That service keys its persisted progress on a run id derived from
// the report and read-modify-writes that row as it walks the drift list, so
// every boundary below is fail-closed: a request that cannot be replayed
// safely is refused *before* any state is written rather than being allowed
// to half-apply. The response bodies stay in this file's flat
// `{ error: <string> }` shape, which callers of the pre-existing 400 already
// depend on.
//
//  B1  A backfill never starts without a report -> 400, service not called.
//      Reconciliation has to run first; nothing to replay otherwise.
//  B2  The report must be structurally sound -> 422. The service derives its
//      run id from `timestamp` (`drift_<timestamp>`) and iterates `drifts`, so
//      a report with an unusable timestamp could collide with another run's
//      progress row and resume at the wrong offset — skipping records while
//      reporting success — and a `driftCount` that disagrees with `drifts`
//      would make the summary contradict the report it came from.
//  B3  A report the worker stored *because reconciliation failed* is not
//      backfillable -> 409. It records no comparison, so replaying it would
//      answer 200 with a meaningless all-zero summary and leave a bogus
//      progress row behind.
//  B4  At most one backfill per process is in flight -> 409. The progress row
//      is not written transactionally, so two concurrent replays of the same
//      report would process the same drift slice twice. This mirrors the
//      in-progress guard on ReconciliationWorker.runReconciliation.
//  B5  A response is written only once the service returns a summary that
//      matches the report -> 502. A 200 with an unusable body is
//      indistinguishable from "there was nothing to backfill", and a body that
//      cannot be serialised would fail after the headers were already sent.
//  B6  Upstream failures reach the shared error handler with status and code
//      intact. `BackfillError` publishes `statusCode` while the handler reads
//      `status`, so without bridging the two every validation failure (for
//      example MAX_BATCH_SIZE_EXCEEDED) would surface as an opaque
//      500 INTERNAL_ERROR. A non-Error throw is also promoted to an Error,
//      because `next(null)` reads as "no error" in Express and would fall
//      through to the 404 handler instead of reporting the failure.

const NO_REPORT = "No drift report available. Run reconciliation first.";
const BACKFILL_IN_PROGRESS = "A reconciliation backfill is already in progress.";
const FAILURE_REPORT =
  "Latest drift report is a reconciliation failure. Re-run reconciliation before backfilling.";

/** Shape the shared error handler reads, plus `BackfillError`'s `statusCode`. */
type StatusedError = Error & { status?: number; code?: string; statusCode?: number };

// Held only for the duration of one replay; released in `finally`.
let backfillInFlight = false;

const nonNegInt = (v: unknown): v is number =>
  typeof v === "number" && Number.isSafeInteger(v) && v >= 0;

const posInt = (v: unknown): v is number =>
  typeof v === "number" && Number.isSafeInteger(v) && v > 0;

/** B3: true when the worker filed this report because reconciliation failed. */
const isFailureReport = (r: DriftReport): boolean => {
  const e = (r as { error?: unknown } | null)?.error;
  return typeof e === "string" && e.trim().length > 0;
};

/** B2: why `r` is unsafe to replay, or null when it is safe. */
const reportDefect = (r: DriftReport): string | null => {
  if (!r || typeof r !== "object") return "Drift report is not a report.";
  if (!posInt(r.timestamp)) return "Drift report has no usable timestamp.";
  if (!Array.isArray(r.drifts)) return "Drift report drift list is not an array.";
  if (!nonNegInt(r.driftCount)) return "Drift report drift count is not a whole number.";
  if (r.driftCount !== r.drifts.length)
    return "Drift report drift count does not match the reported drift list.";
  return null;
};

/** B5: why `result` cannot be relayed, or null when it can be. */
const resultDefect = (result: BackfillResult, driftTotal: number): string | null => {
  if (!result || typeof result !== "object") return "Backfill service returned no result.";
  if (!nonNegInt(result.successCount) || !nonNegInt(result.failCount))
    return "Backfill service returned a non-numeric record count.";
  if (!Array.isArray(result.errors) || result.errors.some((e) => typeof e !== "string"))
    return "Backfill service returned a malformed error list.";
  // Each drift is replayed at most once, so a larger total means the same slice
  // was walked twice — the corruption B4 exists to prevent.
  if (result.successCount + result.failCount > driftTotal)
    return "Backfill service reported more records processed than the report contains.";
  return null;
};

/** B6: normalise any thrown value into something the error handler can render. */
const forwardable = (e: unknown): StatusedError => {
  const err: StatusedError = e instanceof Error ? e : new Error("Drift backfill failed.");
  if (err.status === undefined && err.statusCode !== undefined) err.status = err.statusCode;
  if (typeof err.status !== "number" || !Number.isFinite(err.status)) err.status = 500;
  if (typeof err.code !== "string" || !err.code) err.code = "BACKFILL_FAILED";
  return err;
};

export const triggerBackfill = async (req: Request, res: Response, next: NextFunction) => {
  // Only the request that actually took the lock may release it, so a request
  // rejected before that point cannot clear a concurrent backfill's lock.
  let holdsLock = false;
  try {
    const r = ReconciliationWorker.getLatestReport();
    if (!r) return res.status(400).json({ error: NO_REPORT });

    if (backfillInFlight) return res.status(409).json({ error: BACKFILL_IN_PROGRESS });
    backfillInFlight = true;
    holdsLock = true;

    if (isFailureReport(r)) return res.status(409).json({ error: FAILURE_REPORT });

    const defect = reportDefect(r);
    if (defect) {
      console.warn(`[TriggerBackfill] Rejected drift report: ${defect}`);
      return res.status(422).json({ error: defect });
    }

    const result = await ReconciliationWorker.triggerBoundedBackfill(r);

    const invalid = resultDefect(result, r.drifts.length);
    if (invalid) {
      console.error("[TriggerBackfill] Error: unusable backfill summary", {
        report_timestamp: r.timestamp,
        drift_count: r.drifts.length,
        reason: invalid,
      });
      return res.status(502).json({ error: invalid });
    }

    res.json(result);
  } catch (e) {
    const err = forwardable(e);
    // A write that fails after the headers are on the wire cannot be reported
    // through the error handler; forwarding it would attempt a second write.
    if (res.headersSent) {
      console.error("[TriggerBackfill] Error: backfill failed after the response was sent", err);
      return;
    }
    next(err);
  } finally {
    if (holdsLock) backfillInFlight = false;
  }
};