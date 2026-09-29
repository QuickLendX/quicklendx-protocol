import { Request, Response, NextFunction } from "express";
import { ReconciliationWorker } from "../../services/reconciliationWorker";

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