/**
 * Cursor-based pagination utility.
 *
 * Cursor format: base64url( JSON({ id, sort_val, scope? }) )
 * Sort key: (sort_val DESC, id ASC) - deterministic, stable under concurrent inserts.
 *
 * Security properties:
 *  - Limit is clamped to [1, MAX_LIMI]; no unbounded scans.
 *  - Cursor is opaque (base64url-encoded JSON); malformed cursors are rejected with 400.
 *  - Cursor fields are validated against expected types to prevent injection.
 *  - Cursor is optionally bound to a scope string so a cursor cannot be replayed
 *    against a different filtered view ("scope-safe").
 *  - No information about total count is leaked (no `total` field)
 */

export const DEFAULT_LIMIT = 20;
export const MAX_LIMIT = 100;

export interface CursorPayload {
  id: string;
  sort_val: number;
  scope?: string;
}

export interface PaginationParams {
  limit: number;
  cursor: CursorPayload | null;
}

export interface PageResult<T> {
  data: T[];
  next_cursor: string | null;
  has_more: boolean;
}

/** Encode a cursor to an opaque base64url string. */
export function encodeCursor(payload: CursorPayload): string {
  const body: Record<string, unknown> = {
    id: payload.id,
    sort_val: payload.sort_val,
  };
  if (payload.scope !== undefined) {
    body.scope = payload.scope;
  }
  return Buffer.from(JSON.stringify(body)).toString("base64url");
}

/** Decode and validate a cursor string. Returns null on invalid input. */
export function decodeCursor(raw: string): CursorPayload | null {
  try {
    const json = Buffer.from(raw, "base64url").toString("utf8");
    const parsed: unknown = JSON.parse(json);
    if (typeof parsed !== "object" || parsed === null) {
      return null;
    }
    const obj = parsed as Record<string, unknown>;
    if (
      typeof obj.id !== "string" ||
      typeof obj.sort_val !== "number" ||
      !isFinite(obj.sort_val)
    ) {
      return null;
    }
    const payload: CursorPayload = { id: obj.id, sort_val: obj.sort_val };
    if (obj.scope !== undefined) {
      if (typeof obj.scope !== "string") return null;
      payload.scope = obj.scope;
    }
    return payload;
  } catch {
    return null;
  }
}

/**
 * Parse and validate pagination query params.
 * Throws a typed error on invalid cursor so callers can return 400.
 */
export function parsePaginationParams(query: {
  limit?: unknown;
  cursor?: unknown;
}): PaginationParams {
  // Clamp limit
  let limit = DEFAULT_LIMIT;
  if (query.limit !== undefined) {
    const parsed = Number(query.limit);
    if (!Number.isInteger(parsed) || parsed < 1) {
      throw new PaginationError("limit must be a positive integer");
    }
    limit = Math.min(parsed, MAX_LIMIT);
  }

  // Decode cursor
  let cursor: CursorPayload | null = null;
  if (query.cursor !== undefined && query.cursor !== "") {
    if (typeof query.cursor !== "string") {
      throw new PaginationError("cursor must be a string");
    }
    cursor = decodeCursor(query.cursor);
    if (cursor === null) {
      throw new PaginationError("invalid cursor");
    }
  }

  return { limit, cursor };
}

export class PaginationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "PaginationError";
  }
}

/**
 * Apply cursor-based pagination to an in-memory array.
 *
 * Items must have `id: string` and a numeric sort field.
 * Sort order: sort_field DESC, id ASC (stable tiebreaker).
 *
 * @param items     Full dataset (already filtered, not yet sorted/sliced)
 * @param sortField  Key of the numeric sort field on each item
 * @param params    Parsed pagination params
 * @param scope     Optional scope identifier; when provided, the cursor must
 *                  carry the same scope or the call will throw `PaginationError`.
 */
export function applyPagination<T extends { id: string }>(
  items: T[],
  sortField: keyof T,
  params: PaginationParams,
  scope?: string
): PageResult<T> {
  // Reject invalid sort field values up-front so ordering is deterministic.
  for (const item of items) {
    const v = item[sortField] as unknown;
    if (typeof v !== "number" || !isFinite(v)) {
      throw new PaginationError(
        `Sort field '${String(sortField)}' must be a finite number`
      );
    }
  }

  // Sort: sort_field DESC, id ASC
  const sorted = [.items].sort((a, b) => {
    const av = a[sortField] as unknown as number;
    const bv = b[sortField] as unknown as number;
    if (bv !== av) return bv - av;
    return a.id < b.id ? -1 : a.id > b.id ? 1 : 0;
  });

  // Apply cursor: skip items that come before or at the cursor position
  let startIdx = 0;
  if (params.cursor) {
    const { id: cursorId, sort_val: cursorVal, scope: cursorScope } = params.cursor;
    if (scope !== undefined && cursorScope !== scope) {
      throw new PaginationError("cursor scope mismatch");
    }

    startIdx = sorted.findIndex((item) => {
      const v = item[sortField] as unknown as number;
      if (v < cursorVal) return true;
      if (v === cursorVal && item.id > cursorId) return true;
      return false;
    });
    if (startIdx === -1) {
      // Cursor is past the end
      return { data: [], next_cursor: null, has_more: false };
    }
  }

  const page = sorted.slice(startIdx, startIdx + params.limit);
  const has_more = startIdx + params.limit < sorted.length;

  let next_cursor: string | null = null;
  if (has_more && page.length > 0) {
    const last = page[page.length - 1];
    next_cursor = encodeCursor({
      id: last.id,
      sort_val: last[sortField] as unknown as number,
      scope,
    });
  }

  return { data: page, next_cursor, has_more };
}
