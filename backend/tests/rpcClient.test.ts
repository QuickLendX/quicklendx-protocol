import { jest, describe, it, expect, beforeEach, afterAll } from "@jest/globals";
import { ReliableRpcClient, CircuitState } from "../src/services/rpcClient";
import { config } from "../src/config";

const originalFetch = global.fetch;
const mockFetch = jest.fn() as any;
(global as any).fetch = mockFetch;
(globalThis as any).fetch = mockFetch;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

describe("ReliableRpcClient", () => {
  let client: ReliableRpcClient;

  beforeEach(() => {
    jest.resetAllMocks();
    jest.useRealTimers();
    (config as any).STELLAR_RPC_URL = "https://soroban-testnet.stellar.org";
    (config as any).RPC_ALLOWED_HOSTS = "soroban-testnet.stellar.org,localhost";
    
    client = new ReliableRpcClient({
      retries: 2,
      initialDelayMs: 10,
      failureThreshold: 2,
      resetTimeoutMs: 100,
      maxConcurrency: 2,
      timeoutMs: 500,
    });
  });

  afterAll(() => {
    (global as any).fetch = originalFetch;
    (globalThis as any).fetch = originalFetch;
  });

  it("should return result on successful call", async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ result: "success" }),
    });

    const result = await client.call("getHealth");
    expect(result).toBe("success");
    expect(mockFetch).toHaveBeenCalledTimes(1);
    expect(client.getState()).toBe(CircuitState.CLOSED);
  });

  it("should retry on network failure and succeed eventually", async () => {
    mockFetch
      .mockRejectedValueOnce(new Error("Network Error"))
      .mockResolvedValueOnce({
        ok: true,
        json: async () => ({ result: "retry-success" }),
      });

    const result = await client.call("getHealth");
    expect(result).toBe("retry-success");
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });

  it("should fail after maximum retries", async () => {
    mockFetch.mockRejectedValue(new Error("Failed to fetch"));

    await expect(client.call("getHealth")).rejects.toThrow("Failed to fetch");
    expect(mockFetch).toHaveBeenCalledTimes(3); 
  });

  it("should open circuit breaker after failure threshold", async () => {
    mockFetch.mockRejectedValue(new Error("Network Error"));

    await expect(client.call("m1")).rejects.toThrow();
    await expect(client.call("m2")).rejects.toThrow();
    
    expect(client.getState()).toBe(CircuitState.OPEN);
    
    await expect(client.call("m3")).rejects.toThrow("Circuit breaker is OPEN");
    expect(mockFetch).toHaveBeenCalledTimes(6); 
  });

  it("should allow a single probe in HALF_OPEN state", async () => {
    mockFetch.mockRejectedValue(new Error("Network Error"));
    
    await expect(client.call("fail1")).rejects.toThrow();
    await expect(client.call("fail2")).rejects.toThrow();
    expect(client.getState()).toBe(CircuitState.OPEN);

    await sleep(150);
    
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: async () => ({ result: "probe-success" }),
    });

    const result = await client.call("probe");
    expect(result).toBe("probe-success");
    expect(client.getState()).toBe(CircuitState.CLOSED);
  });

  it("should re-open circuit if probe fails in HALF_OPEN", async () => {
    mockFetch.mockRejectedValue(new Error("Network Error"));
    await expect(client.call("f1")).rejects.toThrow();
    await expect(client.call("f2")).rejects.toThrow();
    expect(client.getState()).toBe(CircuitState.OPEN);

    await sleep(150);
    
    mockFetch.mockRejectedValueOnce(new Error("Probe failed: Network Error"));
    await expect(client.call("probe")).rejects.toThrow("Probe failed");
    expect(client.getState()).toBe(CircuitState.OPEN);
  });

  it("should enforce max concurrency", async () => {
    let resolveFirst: any;
    const firstCallPromise = new Promise((resolve) => { resolveFirst = resolve; });
    mockFetch.mockReturnValue(firstCallPromise);

    const p1 = client.call("c1");
    const p2 = client.call("c2");

    await expect(client.call("c3")).rejects.toThrow("max concurrency reached");
    
    resolveFirst({ ok: true, json: async () => ({ result: "ok" }) });
    await Promise.all([p1, p2]);
  });

  it("should prevent SSRF by checking host allow-list", async () => {
    (config as any).STELLAR_RPC_URL = "https://malicious-host.com/rpc";
    
    await expect(client.call("test")).rejects.toThrow("SSRF Prevention");
    expect(mockFetch).not.toHaveBeenCalled();
  });

  it("should handle 5xx errors by retrying", async () => {
    mockFetch
      .mockResolvedValueOnce({ ok: false, status: 503, statusText: "Service Unavailable" })
      .mockImplementationOnce(() => Promise.resolve({ 
        ok: true, 
        json: async () => ({ result: "recovered-value" }) 
      }));

    const result = await client.call("test503");
    expect(result).toBe("recovered-value");
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });

  it("should handle RPC protocol errors (non-retriable)", async () => {
    mockFetch.mockImplementationOnce(() => Promise.resolve({
      ok: true,
      json: async () => ({ error: { message: "Method not found", code: -32601 } }),
    }));

    await expect(client.call("invalid")).rejects.toThrow(/RPC Protocol Error/);
    expect(mockFetch).toHaveBeenCalledTimes(1);
  });

  it("should cover all states and initialization", async () => {
    const customClient = new ReliableRpcClient({ retries: 0 });
    expect(customClient.getState()).toBe(CircuitState.CLOSED);
  });

  it("should work with the default exported instance", async () => {
    const { rpcClient: defaultClient } = await import("../src/services/rpcClient");
    mockFetch.mockImplementationOnce(() => Promise.resolve({
      ok: true,
      json: async () => ({ result: "default-ok" }),
    }));
    const result = await defaultClient.call("test");
    expect(result).toBe("default-ok");
  });
});
