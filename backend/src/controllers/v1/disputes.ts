import { Request, Response, NextFunction } from "express";
import { Dispute } from "../../types/contract";
import { applyCacheHeaders, CC_NO_STORE } from "../../middleware/cache-headers";
import { parsePaginationParams, PaginationError, applyPagination } from "../../utils/pagination";
import { derivedTableStore } from "../../services/replayService";

export const getDisputes = async (
  req: Request,
  res: Response,
  next: NextFunction
) => {
  try {
    const params = parsePaginationParams(req.query);
    const { id: invoice_id } = req.params;

    let filtered = await listIndexedDisputes();
    if (invoice_id) {
      filtered = filtered.filter((d) => d.invoice_id === invoice_id);
    }

    const page = applyPagination(filtered, "created_at", params);
    const body = { data: page.data, next_cursor: page.next_cursor, has_more: page.has_more };
    applyCacheHeaders(req, res, { cacheControl: CC_NO_STORE, body });
    res.json(body);
  } catch (error) {
    if (error instanceof PaginationError) {
      return res.status(400).json({ error: { message: error.message, code: "INVALID_PAGINATION" } });
    }
    next(error);
  }
};

async function listIndexedDisputes(): Promise<Dispute[]> {
  if (!derivedTableStore.listDisputes) {
    return [];
  }
  return (await derivedTableStore.listDisputes()) as Dispute[];
}
