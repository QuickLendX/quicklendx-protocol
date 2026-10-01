import { Request, Response, NextFunction } from "express";
import { ReconciliationWorker } from "../../services/reconciliationWorker";
import { BackfillResult, DriftReport } from "../../types/reconciliation";

const DEFAULT_LIMIT = 50;
const MAX_LIMIT = 100;
const CURSOR_PREFIX = "v1";

function encodeCursor(value: string): string {
  return Buffer.from(`${CURSOR_PREFIX}:${value}`).toString("base64url");
}

function decodeCursor(raw: string): string {
  const decoded = Buffer.from(raw, "base64url").toString("utf8");
  const separatorIndex = decoded.indexOf(":");
  if (separatorIndex < 0 || decoded.slice(0, separatorIndex) !== CURSOR_PREFIX) {
    throw new Error("bad cursor");
  }
  return decoded.slice(separatorIndex + 1);
}

function normalizeLimit(value: unknown): number | null {
  if (value === undefined) return DEFAULT_LIMIT;
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  if (!/^\d+$/.test(trimmed)) return null;
  const parsed = Number(trimmed);
  if (!Number.isInteger(parsed) || parsed <= 0 || parsed > MAX_LIMIT) return null;
  return parsed;
}

function compareReports(a: any, b: any): number {
  const left = String(a.id ?? "");
  const right = String(b.id ?? "");
  if (left < right) return -1;
  if (left > right) return 1;

  const leftTime = new Date(a.createdAt ?? a.created_at ?? 0).getTime();
  const rightTime = new Date(b.createdAt ?? b.created_at ?? 0).getTime();
  return leftTime - rightTime;
}

export const getDriftReports = async (
  req: Request,
  res: Response,
  next: NextFunction,
): Promise<void> => {
  try {
    const limit = normalizeLimit(req.query.limit);
    if (limit === null) {
      res.status(400).json({ error: "Invalid limit" });
      return;
    }

    let afterId: string | undefined;
    if (req.query.cursor !== undefined) {
      if (typeof req.query.cursor !== "string") {
        res.status(400).json({ error: "Invalid cursor" });
        return;
      }

      try {
        afterId = decodeCursor(req.query.cursor);
      } catch {
        res.status(400).json({ error: "Invalid cursor" });
        return;
      }
    }

    const reports = [...ReconciliationWorker.getAllReports()].sort(compareReports);
    const filtered = afterId
      ? reports.filter((report) => String((report as any).id ?? report.timestamp ?? "") > afterId)
      : reports;
    const page = filtered.slice(0, limit);
    const hasMore = filtered.length > limit;

    res.json({
      data: page,
      pagination: {
        limit,
        nextCursor: hasMore && page.length > 0 ? encodeCursor(String((page[page.length - 1] as any).id ?? "")) : null,
        hasMore,
      },
    });
  } catch (error) {
    next(error);
  }
};

export const runReconciliation = async (
  req: Request,
  res: Response,
  next: NextFunction,
): Promise<void> => {
  try {
    const result = await ReconciliationWorker.runReconciliation();
    res.json(result);
  } catch (error) {
    next(error);
  }
};

export const triggerBackfill = async (
  req: Request,
  res: Response,
  next: NextFunction,
): Promise<void> => {
  try {
    const report = ReconciliationWorker.getLatestReport();
    if (!report) {
      res.status(400).json({ error: "No drift report available. Run reconciliation first." });
      return;
    }

    const result = await ReconciliationWorker.triggerBoundedBackfill(report);
    res.json(result);
  } catch (error) {
    next(error);
  }
};
const D = 50;
const M = 100;
const V = "v1";

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
