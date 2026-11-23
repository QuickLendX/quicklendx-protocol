import { Request } from "express";
import { isNotModified } from "../src/middleware/cache-headers";

describe("conditional cache validation", () => {
  const etag = '"body-hash"';
  const lastModified = new Date("2026-09-29T08:00:00.000Z");

  const requestWithHeaders = (headers: Record<string, string>) =>
    ({ headers }) as unknown as Request;

  it("matches an ETag from a comma-separated If-None-Match header", () => {
    expect(
      isNotModified(
        requestWithHeaders({ "if-none-match": '"older", "body-hash"' }),
        etag,
        lastModified
      )
    ).toBe(true);
  });

  it("does not match a stale ETag or an invalid date", () => {
    expect(
      isNotModified(
        requestWithHeaders({ "if-none-match": '"older"' }),
        etag,
        lastModified
      )
    ).toBe(false);
    expect(
      isNotModified(
        requestWithHeaders({ "if-modified-since": "not a date" }),
        etag,
        lastModified
      )
    ).toBe(false);
  });

  it("matches a wildcard and an unmodified timestamp", () => {
    expect(
      isNotModified(requestWithHeaders({ "if-none-match": "*" }), etag, null)
    ).toBe(true);
    expect(
      isNotModified(
        requestWithHeaders({ "if-modified-since": lastModified.toUTCString() }),
        etag,
        lastModified
      )
    ).toBe(true);
  });
});
