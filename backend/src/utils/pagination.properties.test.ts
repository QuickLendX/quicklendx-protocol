import fc from "fast-check";
import {
  applyPagination,
  decodeCursor,
  DEFAULT_LIMIT,
  encodeCursor,
  MAX_LIMIT,
  PageResult,
  paginationScope,
  parsePaginationParams,
  PaginationError,
  CursorPayload,
} from "./pagination";

interface SortItem {
  id: string;
  score: number;
}

const finiteNumberArb = fc
  .double({ noNaN: true, noDefaultInfinity: true })
  .filter((value) => !Object.is(value, -0));

const cursorPayloadArb = fc
  .record({
    id: fc.string({ minLength: 1 }),
    sort_val: finiteNumberArb,
    scope: fc.oneof(fc.constant(undefined), fc.string()),
  })
  .map(({ id, sort_val, scope }): CursorPayload =>
    scope === undefined ? { id, sort_val } : { id, sort_val, scope }
  );

const itemArb = fc
  .array(
    fc.tuple(fc.string({ minLength: 0, maxLength: 12 }), fc.integer({ min: -20, max: 20 })),
    { minLength: 0, maxLength: 30 }
  )
  .map((entries): SortItem[] =>
    entries.map(([id, score], index) => ({ id: `${index}:${id}`, score }))
  );

const safeStringArb = fc.stringMatching(/^[A-Za-z0-9_-]*$/);

function sortedItems(items: SortItem[]): SortItem[] {
  return [...items].sort((left, right) => {
    if (left.score !== right.score) return right.score - left.score;
    return left.id < right.id ? -1 : left.id > right.id ? 1 : 0;
  });
}

function collectPages(
  items: SortItem[],
  limit: number,
  scope?: string
): PageResult<SortItem>[] {
  const pages: PageResult<SortItem>[] = [];
  let cursor: string | undefined;

  while (true) {
    if (pages.length > items.length + 1) {
      throw new Error("pagination did not terminate");
    }
    const params = parsePaginationParams({ limit, cursor });
    const page = applyPagination(items, "score", params, scope);
    pages.push(page);
    if (!page.has_more || page.next_cursor === null) {
      return pages;
    }
    cursor = page.next_cursor;
  }
}

describe("Pagination property-based tests", () => {
  describe("Cursor round-trip", () => {
    it("decodes every encoded payload without loss", () => {
      fc.assert(
        fc.property(cursorPayloadArb, (payload) => {
          expect(decodeCursor(encodeCursor(payload))).toEqual(payload);
        })
      );
    });

    it("encodes finite numeric extremes without loss", () => {
      fc.assert(
        fc.property(
          fc.oneof(
            fc.constant(Number.MAX_SAFE_INTEGER),
            fc.constant(Number.MIN_SAFE_INTEGER),
            fc.constant(Number.MAX_VALUE),
            fc.constant(Number.MIN_VALUE)
          ),
          (sort_val) => {
            const payload: CursorPayload = { id: "extreme", sort_val };
            expect(decodeCursor(encodeCursor(payload))).toEqual(payload);
          }
        )
      );
    });

    it("preserves ids containing unicode and JSON metacharacters", () => {
      fc.assert(
        fc.property(fc.string({ minLength: 1 }), (id) => {
          const payload: CursorPayload = { id, sort_val: 42 };
          expect(decodeCursor(encodeCursor(payload))?.id).toBe(id);
        })
      );
    });

    it("encodes deterministically", () => {
      fc.assert(
        fc.property(cursorPayloadArb, (payload) => {
          expect(encodeCursor(payload)).toBe(encodeCursor(payload));
        })
      );
    });

    it("ignores extra fields when decoding", () => {
      const encoded = Buffer.from(
        JSON.stringify({ id: "test-id", sort_val: 42, extra: "ignored" })
      ).toString("base64url");
      expect(decodeCursor(encoded)).toEqual({ id: "test-id", sort_val: 42 });
    });

    it("never throws while encoding valid payloads", () => {
      fc.assert(
        fc.property(cursorPayloadArb, (payload) => {
          expect(() => encodeCursor(payload)).not.toThrow();
        })
      );
    });
  });

  describe("Malformed cursors", () => {
    it("rejects invalid field types, missing fields, and non-JSON content", () => {
      const invalid = [
        Buffer.from(JSON.stringify({ id: 123, sort_val: "string" })).toString("base64url"),
        Buffer.from(JSON.stringify({ id: "abc" })).toString("base64url"),
        Buffer.from(JSON.stringify({ sort_val: 123 })).toString("base64url"),
        Buffer.from(JSON.stringify({ id: null, sort_val: 123 })).toString("base64url"),
        Buffer.from(JSON.stringify({ id: "abc", sort_val: NaN })).toString("base64url"),
        Buffer.from(JSON.stringify({ id: "abc", sort_val: Infinity })).toString("base64url"),
        Buffer.from(JSON.stringify({ id: "abc", sort_val: 1, scope: 42 })).toString(
          "base64url"
        ),
        Buffer.from("not json").toString("base64url"),
        "",
        " ",
        "\t",
        "\n",
      ];

      invalid.forEach((cursor) => {
        expect(decodeCursor(cursor)).toBeNull();
      });
    });

    it("never throws on arbitrary strings", () => {
      fc.assert(
        fc.property(fc.string(), (raw) => {
          const decoded = decodeCursor(raw);
          if (decoded !== null) {
            expect(typeof decoded.id).toBe("string");
            expect(typeof decoded.sort_val).toBe("number");
            expect(Number.isFinite(decoded.sort_val)).toBe(true);
          }
        })
      );
    });

    it("returns null or a valid payload for truncated cursors", () => {
      fc.assert(
        fc.property(cursorPayloadArb, (payload) => {
          const encoded = encodeCursor(payload);
          const truncated = encoded.slice(0, Math.max(1, encoded.length - 1));
          const decoded = decodeCursor(truncated);
          if (decoded !== null) {
            expect(typeof decoded.id).toBe("string");
            expect(Number.isFinite(decoded.sort_val)).toBe(true);
          }
        })
      );
    });
  });

  describe("Limit parsing", () => {
    it("uses DEFAULT_LIMIT when limit is omitted", () => {
      expect(parsePaginationParams({}).limit).toBe(DEFAULT_LIMIT);
      expect(parsePaginationParams({ cursor: "" }).limit).toBe(DEFAULT_LIMIT);
    });

    it("preserves limits in the valid range", () => {
      fc.assert(
        fc.property(fc.integer({ min: 1, max: MAX_LIMIT }), (limit) => {
          expect(parsePaginationParams({ limit }).limit).toBe(limit);
        })
      );
    });

    it("clamps limits above MAX_LIMIT", () => {
      fc.assert(
        fc.property(
          fc.integer({ min: MAX_LIMIT + 1, max: Number.MAX_SAFE_INTEGER }),
          (limit) => {
            expect(parsePaginationParams({ limit }).limit).toBe(MAX_LIMIT);
          }
        )
      );
    });

    it("accepts numeric strings as limits", () => {
      fc.assert(
        fc.property(
          fc.integer({ min: 1, max: MAX_LIMIT }).map(String),
          (limit) => {
            expect(parsePaginationParams({ limit }).limit).toBe(Number(limit));
          }
        )
      );
    });

    it("rejects limits below one and non-integer limits", () => {
      fc.assert(
        fc.property(
          fc.oneof(
            fc.integer({ max: 0 }),
            fc
              .double({ noNaN: true, noDefaultInfinity: true })
              .filter((value) => !Number.isInteger(value)),
            fc.constantFrom("", "abc", "1.5", "0", "-1", "NaN", null, NaN, Infinity, {}, [])
          ),
          (limit) => {
            expect(() => parsePaginationParams({ limit })).toThrow(PaginationError);
          }
        )
      );
    });
  });

  describe("Cursor parameter parsing", () => {
    it("decodes cursors passed to parsePaginationParams", () => {
      fc.assert(
        fc.property(cursorPayloadArb, (payload) => {
          const params = parsePaginationParams({ cursor: encodeCursor(payload) });
          expect(params.cursor).toEqual(payload);
          expect(params.limit).toBe(DEFAULT_LIMIT);
        })
      );
    });

    it("treats an empty cursor as no cursor", () => {
      expect(parsePaginationParams({ cursor: "" }).cursor).toBeNull();
      expect(parsePaginationParams({}).cursor).toBeNull();
    });

    it("rejects null and malformed cursors", () => {
      const invalid = [
        null,
        "not a cursor",
        "!!!",
        Buffer.from("not json").toString("base64url"),
      ];
      invalid.forEach((cursor) => {
        expect(() => parsePaginationParams({ cursor })).toThrow(PaginationError);
      });
    });

    it("rejects every non-string cursor", () => {
      fc.assert(
        fc.property(
          fc.oneof(fc.integer(), fc.boolean(), fc.constant(null), fc.array(fc.anything())),
          (cursor) => {
            expect(() => parsePaginationParams({ cursor })).toThrow(PaginationError);
          }
        )
      );
    });
  });

  describe("Scope handling", () => {
    it("canonicalizes filter scopes deterministically", () => {
      fc.assert(
        fc.property(
          fc.array(fc.tuple(safeStringArb, safeStringArb)),
          (entries) => {
            const filters = Object.fromEntries(entries);
            const expected = Object.entries(filters)
              .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
              .map(
                ([key, value]) =>
                  `${encodeURIComponent(key)}=${encodeURIComponent(value)}`
              )
              .join("&");
            expect(paginationScope(filters)).toBe(expected);
          }
        )
      );
    });

    it("omits undefined filters and encodes reserved characters", () => {
      expect(paginationScope({ b: "two", a: "one", missing: undefined })).toBe(
        "a=one&b=two"
      );
      expect(paginationScope({ "a b": "x/y" })).toBe("a%20b=x%2Fy");
    });

    it("throws when a scoped cursor is used with another scope", () => {
      const items: SortItem[] = [
        { id: "a", score: 10 },
        { id: "b", score: 9 },
      ];
      const first = applyPagination(
        items,
        "score",
        parsePaginationParams({ limit: 1 }),
        "tenant-a"
      );
      const params = parsePaginationParams({ limit: 1, cursor: first.next_cursor });
      expect(() => applyPagination(items, "score", params, "tenant-b")).toThrow(
        PaginationError
      );
    });
  });

  describe("Page iteration", () => {
    it("returns items in stable sort_val DESC, id ASC order", () => {
      fc.assert(
        fc.property(itemArb, (items) => {
          const pages = collectPages(items, 3);
          expect(pages.flatMap((page) => page.data)).toEqual(sortedItems(items));
        })
      );
    });

    it("produces identical pages regardless of input order", () => {
      fc.assert(
        fc.property(itemArb, (items) => {
          expect(collectPages(items, 3)).toEqual(collectPages([...items].reverse(), 3));
        })
      );
    });

    it("never repeats an item across pages", () => {
      fc.assert(
        fc.property(itemArb, fc.integer({ min: 1, max: 10 }), (items, limit) => {
          const ids = collectPages(items, limit).flatMap((page) =>
            page.data.map((item) => item.id)
          );
          expect(new Set(ids).size).toBe(ids.length);
        })
      );
    });

    it("keeps pages within the limit and marks only the final page as complete", () => {
      fc.assert(
        fc.property(itemArb, fc.integer({ min: 1, max: 10 }), (items, limit) => {
          const pages = collectPages(items, limit);
          pages.forEach((page, index) => {
            expect(page.data.length).toBeLessThanOrEqual(limit);
            expect(page.has_more).toBe(index < pages.length - 1);
            if (!page.has_more) expect(page.next_cursor).toBeNull();
          });
        })
      );
    });

    it("round-trips next_cursor to the last item on each page", () => {
      fc.assert(
        fc.property(itemArb, safeStringArb, (items, scope) => {
          const pages = collectPages(items, 3, scope);
          pages.forEach((page, index) => {
            if (page.next_cursor === null) return;
            const last = page.data[page.data.length - 1];
            expect(decodeCursor(page.next_cursor)).toEqual({
              id: last.id,
              sort_val: last.score,
              scope,
            });
            expect(pages[index + 1].data[0].id).not.toBe(last.id);
          });
        })
      );
    });

    it("returns an empty page for a cursor past the end", () => {
      const items: SortItem[] = [
        { id: "a", score: 10 },
        { id: "b", score: 9 },
      ];
      const cursor = encodeCursor({ id: "z", sort_val: -1000 });
      const params = parsePaginationParams({ limit: 10, cursor });
      expect(applyPagination(items, "score", params)).toEqual({
        data: [],
        next_cursor: null,
        has_more: false,
      });
    });

    it("rejects non-finite sort fields", () => {
      expect(() =>
        applyPagination(
          [{ id: "a", score: NaN }],
          "score",
          parsePaginationParams({ limit: 10 })
        )
      ).toThrow(PaginationError);
    });

    it("does not mutate the input array", () => {
      fc.assert(
        fc.property(itemArb, (items) => {
          const before = items.map((item) => ({ ...item }));
          collectPages(items, 3);
          expect(items).toEqual(before);
        })
      );
    });
  });
});
