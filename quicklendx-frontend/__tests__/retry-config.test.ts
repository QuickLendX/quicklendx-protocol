// jest.mock is hoisted before imports, so axios.create is mocked before api-client.ts loads
const mockAxiosInstance = {
  interceptors: {
    request: { use: jest.fn() },
    response: { use: jest.fn() },
  },
  request: jest.fn(),
};

jest.mock("axios", () => ({
  __esModule: true,
  default: { create: jest.fn(() => mockAxiosInstance) },
  create: jest.fn(() => mockAxiosInstance),
}));

import { ErrorRecovery } from "../app/lib/errors";
import { ApiClient } from "../app/lib/api-client";

beforeEach(() => {
  jest.clearAllMocks();
  mockAxiosInstance.interceptors.request.use = jest.fn();
  mockAxiosInstance.interceptors.response.use = jest.fn();
});

// ─── ErrorRecovery.retryOperation ────────────────────────────────────────────

describe("ErrorRecovery.retryOperation", () => {
  it("succeeds on first attempt without retrying", async () => {
    const op = jest.fn().mockResolvedValue("ok");
    await expect(ErrorRecovery.retryOperation(op, 3, 1)).resolves.toBe("ok");
    expect(op).toHaveBeenCalledTimes(1);
  });

  it("retries up to maxRetries times then throws", async () => {
    const op = jest.fn().mockRejectedValue(new Error("fail"));
    await expect(ErrorRecovery.retryOperation(op, 3, 1)).rejects.toThrow("fail");
    expect(op).toHaveBeenCalledTimes(3);
  });

  it("maxRetries: 0 calls operation exactly once and does not retry", async () => {
    const op = jest.fn().mockRejectedValue(new Error("no retry"));
    await expect(ErrorRecovery.retryOperation(op, 0, 1)).rejects.toThrow("no retry");
    expect(op).toHaveBeenCalledTimes(1);
  });

  it("caps wait time at maxDelayMs", async () => {
    const delays: number[] = [];
    const origSetTimeout = global.setTimeout;
    // Intercept setTimeout calls made by retryOperation
    global.setTimeout = ((fn: any, ms?: number, ...args: any[]) => {
      delays.push(ms ?? 0);
      return origSetTimeout(fn, 0, ...args); // execute immediately
    }) as any;

    try {
      const op = jest.fn().mockRejectedValue(new Error("fail"));
      await expect(ErrorRecovery.retryOperation(op, 3, 1000, 1500)).rejects.toThrow();
    } finally {
      global.setTimeout = origSetTimeout;
    }

    // attempt 1: min(1000 * 2^0, 1500) = 1000
    // attempt 2: min(1000 * 2^1, 1500) = 1500
    expect(delays[0]).toBe(1000);
    expect(delays[1]).toBe(1500);
  });
});

// ─── ApiClient retryConfig ────────────────────────────────────────────────────

describe("ApiClient retryConfig", () => {
  it("uses DEFAULT_RETRY values when no retryConfig provided", () => {
    const client = new ApiClient();
    expect((client as any).retryConfig).toEqual({
      maxRetries: 3,
      initialDelayMs: 1000,
      maxDelayMs: 30000,
    });
  });

  it("merges consumer retryConfig over defaults", () => {
    const client = new ApiClient({ retryConfig: { maxRetries: 5, initialDelayMs: 500 } });
    expect((client as any).retryConfig).toEqual({
      maxRetries: 5,
      initialDelayMs: 500,
      maxDelayMs: 30000,
    });
  });

  it("maxRetries: 0 disables retries — retryOperation called with maxRetries=0", async () => {
    mockAxiosInstance.request.mockRejectedValue(new Error("network error"));

    const client = new ApiClient({ retryConfig: { maxRetries: 0 } });
    const retrySpy = jest.spyOn(ErrorRecovery, "retryOperation");

    await expect(client.get("/test")).rejects.toThrow();

    expect(retrySpy).toHaveBeenCalledWith(expect.any(Function), 0, 1000, 30000);
    expect(mockAxiosInstance.request).toHaveBeenCalledTimes(1);

    retrySpy.mockRestore();
  });
});
