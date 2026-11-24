/**
 * Tests for how ApiClient treats write requests (#3176).
 *
 *  - `retry: false` sends a request exactly once (a bid must not be resent after a
 *    failure whose outcome is unknown).
 *  - De-duplication of concurrent requests takes the body into account.
 *  - Fields the backend puts next to `message` and `code` in the error envelope
 *    (`POST /bids` 429 EXPOSURE_CAP_EXCEEDED) survive into the thrown error.
 *
 * Run with: npx jest app/lib/__tests__/api-client.writes.test.ts
 */

const mockAxiosInstance = {
  interceptors: {
    request: { use: jest.fn() },
    response: { use: jest.fn() },
  },
  request: jest.fn(),
};

jest.mock("axios", () => {
  const actual = jest.requireActual("axios");
  return {
    ...actual,
    create: jest.fn(() => mockAxiosInstance),
    isAxiosError: actual.isAxiosError,
  };
});

import { apiClient } from "../api-client";
import { AppError } from "../errors";

const responseOnRejected: (err: any) => Promise<never> =
  mockAxiosInstance.interceptors.response.use.mock.calls[0][1];

function axiosError(
  status: number,
  data: unknown,
  headers: Record<string, string> = {}
) {
  return {
    isAxiosError: true,
    config: { method: "post", url: "/bids", timeout: 10000 },
    request: {},
    response: { status, data, headers },
    message: "axios error",
  };
}

async function catchError(error: unknown): Promise<AppError> {
  try {
    await responseOnRejected(error);
  } catch (e) {
    return e as AppError;
  }
  throw new Error("expected the interceptor to reject");
}

describe("ApiClient writes", () => {
  beforeEach(() => {
    mockAxiosInstance.request.mockReset();
    apiClient.clearQueue();
    jest.useRealTimers();
  });

  describe("retry", () => {
    it("retry: false sends the request once and surfaces the first failure", async () => {
      mockAxiosInstance.request.mockRejectedValue(new Error("timeout"));

      await expect(
        apiClient.post("/bids", { a: 1 }, { retry: false })
      ).rejects.toThrow("timeout");

      expect(mockAxiosInstance.request).toHaveBeenCalledTimes(1);
    });

    it("retry: false does not wait for a backoff before failing", async () => {
      mockAxiosInstance.request.mockRejectedValue(new Error("down"));
      const started = Date.now();
      await expect(
        apiClient.post("/bids", {}, { retry: false })
      ).rejects.toThrow("down");
      expect(Date.now() - started).toBeLessThan(500);
    });

    it("does not pass the retry option on to axios", async () => {
      mockAxiosInstance.request.mockResolvedValue({ data: { ok: true } });
      await apiClient.post(
        "/bids",
        { a: 1 },
        { retry: false, headers: { "X-Test": "1" } }
      );

      const sent = mockAxiosInstance.request.mock.calls[0][0];
      expect(sent).not.toHaveProperty("retry");
      expect(sent).toMatchObject({
        method: "POST",
        url: "/bids",
        data: { a: 1 },
        headers: { "X-Test": "1" },
      });
    });

    it("keeps the default of three attempts when retry is not given", async () => {
      jest.useFakeTimers();
      mockAxiosInstance.request.mockRejectedValue(new Error("flaky"));

      const result = apiClient.get("/invoices");
      const assertion = expect(result).rejects.toThrow("flaky");
      await jest.advanceTimersByTimeAsync(10_000);
      await assertion;

      expect(mockAxiosInstance.request).toHaveBeenCalledTimes(3);
    });

    it("retry: 2 makes two attempts", async () => {
      jest.useFakeTimers();
      mockAxiosInstance.request.mockRejectedValue(new Error("flaky"));

      const result = apiClient.get("/invoices", { retry: 2 });
      const assertion = expect(result).rejects.toThrow("flaky");
      await jest.advanceTimersByTimeAsync(10_000);
      await assertion;

      expect(mockAxiosInstance.request).toHaveBeenCalledTimes(2);
    });

    it("returns the data on success without retrying", async () => {
      mockAxiosInstance.request.mockResolvedValue({
        data: { data: { id: 1 } },
      });
      await expect(
        apiClient.post("/bids", {}, { retry: false })
      ).resolves.toEqual({
        data: { id: 1 },
      });
      expect(mockAxiosInstance.request).toHaveBeenCalledTimes(1);
    });
  });

  describe("de-duplicating concurrent requests", () => {
    it("does not merge two POSTs to the same URL with different bodies", async () => {
      mockAxiosInstance.request.mockImplementation(
        (config: { data: { n: number } }) =>
          Promise.resolve({ data: { echoed: config.data.n } })
      );

      const [first, second] = await Promise.all([
        apiClient.post("/bids", { n: 1 }, { retry: false }),
        apiClient.post("/bids", { n: 2 }, { retry: false }),
      ]);

      expect(mockAxiosInstance.request).toHaveBeenCalledTimes(2);
      // Each caller gets its own request's result, not the first caller's.
      expect(first).toEqual({ echoed: 1 });
      expect(second).toEqual({ echoed: 2 });
    });

    it("still merges two identical concurrent requests", async () => {
      mockAxiosInstance.request.mockResolvedValue({ data: { ok: true } });

      await Promise.all([
        apiClient.post("/bids", { n: 1 }, { retry: false }),
        apiClient.post("/bids", { n: 1 }, { retry: false }),
      ]);

      expect(mockAxiosInstance.request).toHaveBeenCalledTimes(1);
    });

    it("still merges identical GETs", async () => {
      mockAxiosInstance.request.mockResolvedValue({ data: [] });
      await Promise.all([
        apiClient.get("/invoices"),
        apiClient.get("/invoices"),
      ]);
      expect(mockAxiosInstance.request).toHaveBeenCalledTimes(1);
    });

    it("does not merge GETs with different params", async () => {
      mockAxiosInstance.request.mockResolvedValue({ data: [] });
      await Promise.all([
        apiClient.get("/invoices", { params: { page: 1 } }),
        apiClient.get("/invoices", { params: { page: 2 } }),
      ]);
      expect(mockAxiosInstance.request).toHaveBeenCalledTimes(2);
    });

    it("treats a body that cannot be serialised as unique rather than throwing", async () => {
      const circular: Record<string, unknown> = {};
      circular.self = circular;
      mockAxiosInstance.request.mockResolvedValue({ data: {} });

      await expect(
        Promise.all([
          apiClient.post("/x", circular, { retry: false }),
          apiClient.post("/x", circular, { retry: false }),
        ])
      ).resolves.toBeDefined();
      expect(mockAxiosInstance.request).toHaveBeenCalledTimes(2);
    });

    it("forgets a finished request, so the same call can be made again", async () => {
      mockAxiosInstance.request.mockResolvedValue({ data: {} });
      await apiClient.post("/bids", { n: 1 }, { retry: false });
      await apiClient.post("/bids", { n: 1 }, { retry: false });
      expect(mockAxiosInstance.request).toHaveBeenCalledTimes(2);
    });
  });

  describe("error envelope", () => {
    it("keeps the cap numbers a 429 EXPOSURE_CAP_EXCEEDED carries next to message and code", async () => {
      const error = await catchError(
        axiosError(429, {
          error: {
            message: "Investor exposure cap would be exceeded",
            code: "EXPOSURE_CAP_EXCEEDED",
            currentExposureUsd: "1100000000000000",
            attemptedUsd: "500000000000000",
            capUsd: "1500000000000000",
            investor: "GINVESTOR",
          },
        })
      );

      expect(error.code).toBe("EXPOSURE_CAP_EXCEEDED");
      expect(error.message).toBe("Investor exposure cap would be exceeded");
      expect(error.context.additionalData?.details).toEqual({
        currentExposureUsd: "1100000000000000",
        attemptedUsd: "500000000000000",
        capUsd: "1500000000000000",
        investor: "GINVESTOR",
      });
    });

    it("carries Retry-After on a 429 and marks it as a rate limit by the retryAfter key", async () => {
      const error = await catchError(
        axiosError(
          429,
          { error: { message: "slow", code: "RATE_LIMITED" } },
          { "retry-after": "30" }
        )
      );
      expect(error.context.additionalData?.retryAfter).toBe("30");
      expect(error.context.additionalData).not.toHaveProperty("details");
    });

    it("prefers an explicit details field over extra fields", async () => {
      const error = await catchError(
        axiosError(400, {
          error: {
            message: "Validation failed",
            code: "VALIDATION_ERROR",
            details: [{ field: "bid_amount", message: "bad" }],
            somethingElse: "ignored when details is present",
          },
        })
      );
      expect(error.context.additionalData?.details).toEqual([
        { field: "bid_amount", message: "bad" },
      ]);
    });

    it("adds no details when the envelope has only message and code", async () => {
      const error = await catchError(
        axiosError(400, { error: { message: "nope", code: "INVALID_BID" } })
      );
      expect(error.context.additionalData).not.toHaveProperty("details");
    });

    it("still copes with a body that is not an envelope at all", async () => {
      const error = await catchError(axiosError(429, "<html>too many</html>"));
      expect(error.message).toBe("Too many requests. Please try again later.");
      expect(error.context.additionalData).not.toHaveProperty("details");
    });
  });
});
