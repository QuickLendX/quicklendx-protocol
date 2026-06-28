import { ReliableRpcClient, CircuitState } from "../services/rpcClient";
import { FaultyFetch } from "./helpers/faultInjector";

describe("RPC Client & Circuit Breaker - Fault Injection Tests", () => {
  let faultyFetch: FaultyFetch;
  let client: ReliableRpcClient;

  beforeEach(() => {
    faultyFetch = new FaultyFetch();
    faultyFetch.setup();
    client = new ReliableRpcClient({
      retries: 1,
      initialDelayMs: 1,
      maxDelayMs: 2,
      failureThreshold: 2,
      resetTimeoutMs: 50, // short timeout for fast transition testing
    });
  });

  afterEach(() => {
    faultyFetch.restore();
  });

  it("executes successfully when fetch returns 200 (CLOSED state)", async () => {
    faultyFetch.queueFailure({ status: 200, body: JSON.stringify({ jsonrpc: "2.0", result: "success-data" }) });

    const res = await client.call("getLedger");
    expect(res).toBe("success-data");
    expect(client.getState()).toBe(CircuitState.CLOSED);
  });

  it("handles fetch returning 502 then 200 via retry", async () => {
    // Attempt 0 fails with 502, Attempt 1 (retry) succeeds with 200
    faultyFetch.queueFailure({ status: 502 });
    faultyFetch.queueFailure({ status: 200, body: JSON.stringify({ jsonrpc: "2.0", result: "retry-success" }) });

    const res = await client.call("getLedger");
    expect(res).toBe("retry-success");
    expect(client.getState()).toBe(CircuitState.CLOSED);
  });

  it("transitions CLOSED -> OPEN after exceeding failure threshold", async () => {
    // 1st failed call: both attempt 0 and attempt 1 fail
    faultyFetch.queueFailure({ status: 500 });
    faultyFetch.queueFailure({ status: 500 });
    await expect(client.call("getLedger")).rejects.toThrow("RPC HTTP Error");
    expect(client.getState()).toBe(CircuitState.CLOSED); // 1 failure, threshold is 2

    // 2nd failed call: both attempt 0 and attempt 1 fail
    faultyFetch.queueFailure({ status: 500 });
    faultyFetch.queueFailure({ status: 500 });
    await expect(client.call("getLedger")).rejects.toThrow("RPC HTTP Error");
    
    // Circuit must now be OPEN
    expect(client.getState()).toBe(CircuitState.OPEN);

    // 3rd call: fails immediately with circuit breaker error without calling fetch
    await expect(client.call("getLedger")).rejects.toThrow("Circuit breaker is OPEN");
  });

  it("transitions OPEN -> HALF_OPEN -> CLOSED on success after reset timeout", async () => {
    // Trip the circuit to OPEN
    faultyFetch.queueFailure({ status: 500 });
    faultyFetch.queueFailure({ status: 500 });
    await expect(client.call("getLedger")).rejects.toThrow();
    faultyFetch.queueFailure({ status: 500 });
    faultyFetch.queueFailure({ status: 500 });
    await expect(client.call("getLedger")).rejects.toThrow();
    expect(client.getState()).toBe(CircuitState.OPEN);

    // Wait for reset timeout (50ms)
    await new Promise((resolve) => setTimeout(resolve, 60));

    // Next request should transition state to HALF_OPEN and be sent.
    // If it succeeds, the state should transition back to CLOSED.
    faultyFetch.queueFailure({ status: 200, body: JSON.stringify({ jsonrpc: "2.0", result: "half-open-success" }) });

    const res = await client.call("getLedger");
    expect(res).toBe("half-open-success");
    expect(client.getState()).toBe(CircuitState.CLOSED);
  });

  it("transitions OPEN -> HALF_OPEN -> OPEN on failure after reset timeout", async () => {
    // Trip the circuit to OPEN
    faultyFetch.queueFailure({ status: 500 });
    faultyFetch.queueFailure({ status: 500 });
    await expect(client.call("getLedger")).rejects.toThrow();
    faultyFetch.queueFailure({ status: 500 });
    faultyFetch.queueFailure({ status: 500 });
    await expect(client.call("getLedger")).rejects.toThrow();
    expect(client.getState()).toBe(CircuitState.OPEN);

    // Wait for reset timeout
    await new Promise((resolve) => setTimeout(resolve, 60));

    // Next request transitions to HALF_OPEN but fails, sending it back to OPEN immediately
    faultyFetch.queueFailure({ status: 500 });
    faultyFetch.queueFailure({ status: 500 });
    await expect(client.call("getLedger")).rejects.toThrow();

    expect(client.getState()).toBe(CircuitState.OPEN);
  });

  it("handles a raw network error in FaultyFetch and propagates it", async () => {
    faultyFetch.queueFailure({ error: new TypeError("Failed to fetch due to connection reset") });
    faultyFetch.queueFailure({ error: new TypeError("Failed to fetch due to connection reset") });

    await expect(client.call("getLedger")).rejects.toThrow("Failed to fetch");
  });

  it("falls back to original fetch when no failure is queued", async () => {
    // When no failures are queued, FaultyFetch delegates to the original fetch.
    // The original fetch should try to contact the real STELLAR_RPC_URL and fail (or succeed) in Jest context.
    // We just assert that it executes the real fetch logic (which throws an HTTP/network/protocol error).
    await expect(client.call("getLedger")).rejects.toThrow();
  });
});
