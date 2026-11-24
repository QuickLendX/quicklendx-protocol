import {
  ApiHttpError,
  isNotFoundError,
  fetchInvoices,
  fetchInvoiceById,
  fetchBestBid,
  fetchRankedBids,
  listInvoices,
  getInvoice,
} from "../endpoints";

const originalFetch = global.fetch;

function stubFetch(response: {
  ok: boolean;
  status: number;
  body?: unknown;
  jsonError?: boolean;
}): jest.Mock {
  const fetchMock = jest.fn().mockResolvedValue({
    ok: response.ok,
    status: response.status,
    json: response.jsonError
      ? async () => {
          throw new Error("not json");
        }
      : async () => response.body,
  });
  (global as any).fetch = fetchMock;
  return fetchMock;
}

const invoiceBody = {
  id: "0xabc123",
  business: "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K",
  amount: "50000000000",
  currency: "USDC",
  due_date: 1778803200,
  status: "Verified",
  description: "Freight invoice",
  category: "Services",
  tags: ["freight"],
  created_at: 1776211200,
  updated_at: 1776297600,
};

async function captureError(promise: Promise<unknown>): Promise<unknown> {
  try {
    await promise;
  } catch (err) {
    return err;
  }
  throw new Error("Expected promise to reject, but it resolved");
}

afterEach(() => {
  global.fetch = originalFetch;
  jest.restoreAllMocks();
});

describe("ApiHttpError", () => {
  it("carries message, status and code and is an Error", () => {
    const err = new ApiHttpError("Invoice not found", 404, "INVOICE_NOT_FOUND");
    expect(err).toBeInstanceOf(Error);
    expect(err).toBeInstanceOf(ApiHttpError);
    expect(err.name).toBe("ApiHttpError");
    expect(err.message).toBe("Invoice not found");
    expect(err.status).toBe(404);
    expect(err.code).toBe("INVOICE_NOT_FOUND");
  });

  it("allows omitting the code", () => {
    const err = new ApiHttpError("Server error", 500);
    expect(err.status).toBe(500);
    expect(err.code).toBeUndefined();
  });
});

describe("fetchInvoices error mapping", () => {
  it("throws ApiHttpError with status 500 for non-2xx and is not a 404", async () => {
    stubFetch({ ok: false, status: 500 });
    const err = await captureError(fetchInvoices());
    expect(err).toBeInstanceOf(ApiHttpError);
    expect((err as ApiHttpError).status).toBe(500);
    expect((err as Error).message).toBe("Failed to fetch invoices: HTTP 500");
    expect(isNotFoundError(err)).toBe(false);
  });

  it("throws ApiHttpError with status 404 that isNotFoundError recognizes", async () => {
    stubFetch({ ok: false, status: 404 });
    const err = await captureError(fetchInvoices());
    expect(err).toBeInstanceOf(ApiHttpError);
    expect((err as ApiHttpError).status).toBe(404);
    expect(isNotFoundError(err)).toBe(true);
  });

  it("still parses a successful page and forwards status/cursor/limit", async () => {
    const fetchMock = stubFetch({
      ok: true,
      status: 200,
      body: { data: [invoiceBody], next_cursor: "c2", has_more: true },
    });
    const page = await fetchInvoices({
      status: "Funded",
      cursor: "c1",
      limit: 20,
    });
    expect(page.data).toHaveLength(1);
    expect(page.data[0].id).toBe("0xabc123");
    expect(page.next_cursor).toBe("c2");
    expect(page.has_more).toBe(true);
    const url = fetchMock.mock.calls[0][0] as string;
    expect(url).toContain("status=Funded");
    expect(url).toContain("cursor=c1");
    expect(url).toContain("limit=20");
  });
});

describe("fetchInvoiceById error mapping", () => {
  it("throws ApiHttpError with status 404 for a missing invoice", async () => {
    stubFetch({ ok: false, status: 404 });
    const err = await captureError(fetchInvoiceById("0xmissing"));
    expect(err).toBeInstanceOf(ApiHttpError);
    expect((err as ApiHttpError).status).toBe(404);
    expect((err as Error).message).toContain("0xmissing");
    expect(isNotFoundError(err)).toBe(true);
  });

  it("throws ApiHttpError with status 500 for server errors", async () => {
    stubFetch({ ok: false, status: 500 });
    const err = await captureError(fetchInvoiceById("0xabc123"));
    expect(err).toBeInstanceOf(ApiHttpError);
    expect((err as ApiHttpError).status).toBe(500);
    expect(isNotFoundError(err)).toBe(false);
  });
});

describe("re-exported listInvoices / getInvoice error mapping", () => {
  it("listInvoices surfaces the API error body message and code on 404", async () => {
    stubFetch({
      ok: false,
      status: 404,
      body: {
        error: { message: "Invoice not found", code: "INVOICE_NOT_FOUND" },
      },
    });
    const err = await captureError(listInvoices({ business: "GABC" }));
    expect(err).toBeInstanceOf(ApiHttpError);
    expect((err as ApiHttpError).status).toBe(404);
    expect((err as ApiHttpError).message).toBe("Invoice not found");
    expect((err as ApiHttpError).code).toBe("INVOICE_NOT_FOUND");
    expect(isNotFoundError(err)).toBe(true);
  });

  it("listInvoices falls back to 'HTTP 500' when the error body is not JSON", async () => {
    stubFetch({ ok: false, status: 500, jsonError: true });
    const err = await captureError(listInvoices());
    expect(err).toBeInstanceOf(ApiHttpError);
    expect((err as ApiHttpError).status).toBe(500);
    expect((err as ApiHttpError).message).toBe("HTTP 500");
    expect((err as ApiHttpError).code).toBeUndefined();
    expect(isNotFoundError(err)).toBe(false);
  });

  it("getInvoice throws ApiHttpError with status 404", async () => {
    stubFetch({ ok: false, status: 404 });
    const err = await captureError(getInvoice("0xmissing"));
    expect(err).toBeInstanceOf(ApiHttpError);
    expect((err as ApiHttpError).status).toBe(404);
    expect(isNotFoundError(err)).toBe(true);
  });

  it("listInvoices forwards business/status/limit/cursor and parses the page", async () => {
    const fetchMock = stubFetch({
      ok: true,
      status: 200,
      body: { data: [invoiceBody], next_cursor: null, has_more: false },
    });
    const page = await listInvoices({
      business: "GABC",
      status: "Funded",
      limit: 20,
      cursor: "c1",
    });
    expect(page.data).toHaveLength(1);
    expect(page.next_cursor).toBeNull();
    expect(page.has_more).toBe(false);
    const url = fetchMock.mock.calls[0][0] as string;
    expect(url).toContain("business=GABC");
    expect(url).toContain("status=Funded");
    expect(url).toContain("limit=20");
    expect(url).toContain("cursor=c1");
  });
});

describe("isNotFoundError duck typing", () => {
  it.each([
    ["status", { status: 404 }],
    ["statusCode", { statusCode: 404 }],
    ["response.status", { response: { status: 404 } }],
    ["code NOT_FOUND", { code: "NOT_FOUND" }],
    ["code INVOICE_NOT_FOUND", { code: "INVOICE_NOT_FOUND" }],
    ["message containing 404", new Error("Request failed with 404")],
  ])("recognizes 404 via %s", (_label, err) => {
    expect(isNotFoundError(err)).toBe(true);
  });

  it.each([
    ["null", null],
    ["undefined", undefined],
    ["a string", "404"],
    ["an empty object", {}],
    ["a generic error", new Error("boom")],
    ["a 500 ApiHttpError", new ApiHttpError("Server error", 500)],
    ["a 400 status object", { status: 400 }],
  ])("returns false for %s", (_label, err) => {
    expect(isNotFoundError(err)).toBe(false);
  });
});

describe("bid fetchers keep their 404 semantics for invest", () => {
  it("fetchBestBid resolves null on 404 instead of throwing", async () => {
    stubFetch({ ok: false, status: 404 });
    await expect(fetchBestBid("0xabc123")).resolves.toBeNull();
  });

  it("fetchRankedBids resolves [] on 404 instead of throwing", async () => {
    stubFetch({ ok: false, status: 404 });
    await expect(fetchRankedBids("0xabc123")).resolves.toEqual([]);
  });

  it("fetchBestBid still throws on non-404 failures", async () => {
    stubFetch({ ok: false, status: 500 });
    await expect(fetchBestBid("0xabc123")).rejects.toThrow("HTTP 500");
  });
});
import { ErrorCategory, ErrorSeverity } from "../../errors";
  ApiNetworkError,
  DEFAULT_TIMEOUT_MS,
function mockResponse(init: {
  headers?: Record<string, string>;
}): Response {
  const rawText =
    init.body === undefined
      ? ""
      : typeof init.body === "string"
        ? init.body
        : JSON.stringify(init.body);
  const headerMap = new Map<string, string>(
    Object.entries(init.headers ?? {}).map(([k, v]) => [k.toLowerCase(), v])
  );
  return {
    ok: init.ok,
    status: init.status,
    headers: {
      get: (name: string) => headerMap.get(name.toLowerCase()) ?? null,
    },
    text: jest.fn().mockResolvedValue(rawText),
    json: jest.fn().mockResolvedValue(init.body),
  } as unknown as Response;
describe("api/endpoints typed errors, timeouts, and abort handling", () => {
  const originalFetch = global.fetch;
  beforeEach(() => {
    jest.restoreAllMocks();
  afterEach(() => {
    jest.useRealTimers();
    global.fetch = originalFetch;
  it("aborts after default 10s timeout and throws ApiNetworkError", async () => {
    jest.useFakeTimers();
    global.fetch = jest.fn((_url, init?: RequestInit) => {
      return new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => {
          const abortErr = new Error("The operation was aborted");
          abortErr.name = "AbortError";
          reject(abortErr);
        });
      });
    }) as unknown as typeof fetch;
    const promise = fetchInvoices({ status: "Verified" });
    jest.advanceTimersByTime(DEFAULT_TIMEOUT_MS);
    await expect(promise).rejects.toBeInstanceOf(ApiNetworkError);
    await expect(promise).rejects.toMatchObject({
      name: "ApiNetworkError",
      isTimeout: true,
      isAborted: false,
      code: "REQUEST_TIMEOUT",
      category: ErrorCategory.NETWORK,
      retryable: true,
  it("aborts when caller AbortSignal fires and throws ApiNetworkError with isAborted", async () => {
    const controller = new AbortController();
    global.fetch = jest.fn((_url, init?: RequestInit) => {
      return new Promise<Response>((_resolve, reject) => {
        init?.signal?.addEventListener("abort", () => {
          const abortErr = new Error("Aborted");
          abortErr.name = "AbortError";
          reject(abortErr);
        });
      });
    }) as unknown as typeof fetch;
    const promise = fetchInvoiceById("inv-1", { signal: controller.signal });
    controller.abort();
    await expect(promise).rejects.toMatchObject({
      name: "ApiNetworkError",
      isAborted: true,
      isTimeout: false,
      code: "REQUEST_ABORTED",
  it("rejects immediately when passed an already-aborted signal", async () => {
    const controller = new AbortController();
    controller.abort();
    const fetchMock = jest.fn();
    global.fetch = fetchMock as unknown as typeof fetch;
    await expect(
      fetchInvoices({ signal: controller.signal })
    ).rejects.toMatchObject({
      name: "ApiNetworkError",
      isAborted: true,
      code: "REQUEST_ABORTED",
    expect(fetchMock).not.toHaveBeenCalled();
  it("wraps fetch network failures in ApiNetworkError", async () => {
    global.fetch = jest
      .fn()
      .mockRejectedValue(
        new TypeError("Failed to fetch")
      ) as unknown as typeof fetch;
    await expect(fetchInvoices()).rejects.toMatchObject({
      name: "ApiNetworkError",
      message: "Failed to fetch",
      isTimeout: false,
      isAborted: false,
      code: "NETWORK_ERROR",
      category: ErrorCategory.NETWORK,
  it("throws ApiHttpError with status and parsed JSON body on HTTP 404", async () => {
    global.fetch = jest.fn().mockResolvedValue(
      mockResponse({
        ok: false,
        status: 404,
        body: { error: "Invoice not found" },
      })
    ) as unknown as typeof fetch;
    let caught: unknown;
    try {
      await fetchInvoiceById("missing-id");
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(ApiHttpError);
    const httpErr = caught as ApiHttpError;
    expect(httpErr.status).toBe(404);
    expect(httpErr.body).toEqual({ error: "Invoice not found" });
    expect(httpErr.category).toBe(ErrorCategory.BUSINESS_LOGIC);
    expect(httpErr.retryable).toBe(false);
    expect(httpErr.code).toBe("HTTP_404");
  it("honours Retry-After header text on HTTP 429", async () => {
    global.fetch = jest.fn().mockResolvedValue(
      mockResponse({
        ok: false,
        status: 429,
        body: { error: "Rate limit exceeded" },
        headers: { "Retry-After": "30" },
      })
    ) as unknown as typeof fetch;
    let caught: unknown;
    try {
      await fetchInvoices();
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(ApiHttpError);
    const httpErr = caught as ApiHttpError;
    expect(httpErr.status).toBe(429);
    expect(httpErr.retryAfter).toBe("30");
    expect(httpErr.message).toContain("Retry after 30");
    expect(httpErr.retryable).toBe(true);
    expect(httpErr.code).toBe("HTTP_429");
  it("throws ApiHttpError with plain-text body on HTTP 500", async () => {
    global.fetch = jest.fn().mockResolvedValue(
      mockResponse({
        ok: false,
        status: 500,
        body: "Internal Server Error",
      })
    ) as unknown as typeof fetch;
    let caught: unknown;
    try {
      await fetchRankedBids("inv-1");
    } catch (err) {
      caught = err;
    }
    expect(caught).toBeInstanceOf(ApiHttpError);
    const httpErr = caught as ApiHttpError;
    expect(httpErr.status).toBe(500);
    expect(httpErr.body).toBe("Internal Server Error");
    expect(httpErr.category).toBe(ErrorCategory.SYSTEM);
    expect(httpErr.severity).toBe(ErrorSeverity.CRITICAL);
    expect(httpErr.retryable).toBe(true);
  it("preserves 404 empty semantics for fetchBestBid and fetchRankedBids", async () => {
    global.fetch = jest.fn().mockResolvedValue(
      mockResponse({
        ok: false,
        status: 404,
      })
    ) as unknown as typeof fetch;
    await expect(fetchBestBid("inv-1")).resolves.toBeNull();
    await expect(fetchRankedBids("inv-1")).resolves.toEqual([]);
