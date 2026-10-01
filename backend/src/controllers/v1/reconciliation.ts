import { Request, Response, NextFunction } from "express";
import { ReconciliationWorker } from "../../services/reconciliationWorker";
import { BackfillResult, DriftReport } from "../../types/reconciliation";

const DEFAULT_LIMIT = 50;
const MAX_LIMIT = 100;
const CURSOR_VERSION = "v1";

function encodeCursor(timestamp: number): string {
  return Buffer.from(`${CURSOR_VERSION}:${timestamp}`).toString("base64url");
}

function decodeCursor(cursor: string): number {
  const decoded = Buffer.from(cursor, "base64url").toString();
  const separator = decoded.indexOf(":");
  const timestamp = Number(decoded.slice(separator + 1));
  if (
    separator < 0 ||
    decoded.slice(0, separator) !== CURSOR_VERSION ||
    !Number.isFinite(timestamp)
  ) {
    throw new Error("Invalid cursor");
  }
  return timestamp;
}

function parseLimit(value: unknown): number | null {
  if (value === undefined) return DEFAULT_LIMIT;
  if (typeof value !== "string" || !/^\d+$/.test(value.trim())) return null;

  const limit = Number(value);
  return Number.isInteger(limit) && limit > 0 && limit <= MAX_LIMIT
    ? limit
    : null;
}

function compareReports(a: unknown, b: unknown): number {
  const left = a as Record<string, unknown>;
  const right = b as Record<string, unknown>;
  return Number(left.timestamp) - Number(right.timestamp);
}

export const getDriftReports = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const limit = parseLimit(req.query.limit);
    if (limit === null) return res.status(400).json({ error: "Invalid limit" });

    let afterTimestamp: number | undefined;
    if (req.query.cursor !== undefined) {
      if (typeof req.query.cursor !== "string") {
        return res.status(400).json({ error: "Invalid cursor" });
      }
      try {
        afterTimestamp = decodeCursor(req.query.cursor);
      } catch {
        return res.status(400).json({ error: "Invalid cursor" });
      }
    }

    const reports = [...ReconciliationWorker.getAllReports()].sort(compareReports);
    const remaining = afterTimestamp !== undefined
      ? reports.filter((report) => report.timestamp > afterTimestamp!)
      : reports;
    const hasMore = remaining.length > limit;
    const data = hasMore ? remaining.slice(0, limit) : remaining;
    const lastTimestamp = data.length > 0 ? data[data.length - 1].timestamp : 0;

    res.json({
      data,
      pagination: {
        limit,
        nextCursor: hasMore ? encodeCursor(lastTimestamp) : null,
        hasMore,
      },
    });
  } catch (error) {
    next(error);
  }
};

export const runReconciliation = async (
  _req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    res.json(await ReconciliationWorker.runReconciliation());
  } catch (error) {
    next(error);
  }
};

export const triggerBackfill = async (
  _req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const report = ReconciliationWorker.getLatestReport();
    if (!report) {
      return res
        .status(400)
        .json({ error: "No drift report available. Run reconciliation first." });
    }
    res.json(await ReconciliationWorker.triggerBoundedBackfill(report));
  } catch (error) {
    next(error);
  }
};
