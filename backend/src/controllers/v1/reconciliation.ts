import { Request, Response, NextFunction } from "express";
import { ReconciliationWorker } from "../../services/reconciliationWorker";

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
    res.json(await ReconciliationWorker.runReconciliation());
  } catch (e) {
    next(e);
  }
};

export const triggerBackfill = async (req: Request, res: Response, next: NextFunction) => {
  try {
    const r = ReconciliationWorker.getLatestReport();
    if (!r)
      return res
        .status(400)
        .json({ error: "No drift report available. Run reconciliation first." });
    res.json(await ReconciliationWorker.triggerBoundedBackfill(r));
  } catch (e) {
    next(e);
  }
};
