import { ErrorCategory, ErrorSeverity } from "../../errors";
import {
  ApiHttpError,
  ApiNetworkError,
  DEFAULT_TIMEOUT_MS,
  fetchBestBid,
  fetchInvoiceById,
  fetchInvoices,
  fetchRankedBids,
} from "../endpoints";

function mockResponse(init: {
  ok: boolean;
  status: number;
  body?: unknown;
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
}

describe("api/endpoints typed errors, timeouts, and abort handling", () => {
  const originalFetch = global.fetch;

  beforeEach(() => {
    jest.restoreAllMocks();
  });

  afterEach(() => {
    jest.useRealTimers();
    global.fetch = originalFetch;
  });

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
    });
  });

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
    });
  });

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
    });
    expect(fetchMock).not.toHaveBeenCalled();
  });

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
    });
  });

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
  });

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
  });

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
  });

  it("preserves 404 empty semantics for fetchBestBid and fetchRankedBids", async () => {
    global.fetch = jest.fn().mockResolvedValue(
      mockResponse({
        ok: false,
        status: 404,
      })
    ) as unknown as typeof fetch;

    await expect(fetchBestBid("inv-1")).resolves.toBeNull();
    await expect(fetchRankedBids("inv-1")).resolves.toEqual([]);
  });
});
