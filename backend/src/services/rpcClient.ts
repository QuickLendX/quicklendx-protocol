import { config } from "../config";
import { URL } from "url";
import { getCorrelationId } from "../lib/requestContext";

export interface RpcOptions extends CircuitBreakerOptions {
  timeoutMs?: number;
}

const DEFAULT_OPTIONS: Required<RpcOptions> = {
  retries: 3,
  initialDelayMs: 100,
  maxDelayMs: 10000,
  timeoutMs: 5000,
  failureThreshold: 5,
  resetTimeoutMs: 30000,
  maxConcurrency: 10,
};

export class ReliableRpcClient {
  private options: Required<RpcOptions>;
  private allowedHosts: Set<string>;
  private circuitBreaker: CircuitBreaker;

  constructor(options: RpcOptions = {}) {
    this.options = { ...DEFAULT_OPTIONS, ...options };
    this.allowedHosts = new Set(
      config.RPC_ALLOWED_HOSTS.split(",").map((h) => h.trim())
    );
    this.circuitBreaker = new CircuitBreaker({
      retries: this.options.retries,
      initialDelayMs: this.options.initialDelayMs,
      maxDelayMs: this.options.maxDelayMs,
      failureThreshold: this.options.failureThreshold,
      resetTimeoutMs: this.options.resetTimeoutMs,
      maxConcurrency: this.options.maxConcurrency,
    });
  }

  /**
   * Execute a JSON-RPC call with retries, jitter, and circuit breaker protection.
   */
  async call<T>(method: string, params: any[] | Record<string, any> = []): Promise<T> {
    this.validateHost();

    return this.circuitBreaker.execute(
      () => this.performRequest<T>(method, params),
      (error) => this.shouldRetry(error)
    );
  }

  private validateHost() {
    const url = new URL(config.STELLAR_RPC_URL);
    if (!this.allowedHosts.has(url.hostname)) {
      throw new Error(`SSRF Prevention: Host ${url.hostname} is not in the allow-list.`);
    }
  }

  private async performRequest<T>(method: string, params: any): Promise<T> {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.options.timeoutMs);

    // Forward the originating request id (from async-local-storage) to the
    // upstream Soroban RPC so the call can be correlated end-to-end. Absent a
    // request context (e.g. background workers) the header is simply omitted.
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
    };
    const requestId = getCorrelationId();
    if (requestId) {
      headers["X-Request-Id"] = requestId;
    }

    try {
      const response = await fetch(config.STELLAR_RPC_URL, {
        method: "POST",
        headers,
        body: JSON.stringify({
          jsonrpc: "2.0",
          id: Date.now(),
          method,
          params,
        }),
        signal: controller.signal,
      });

      if (!response.ok) {
        throw new Error(`RPC HTTP Error: ${response.status} ${response.statusText}`);
      }

      const json: any = await response.json();
      if (json.error) {
        throw new Error(`RPC Protocol Error: ${json.error.message} (code: ${json.error.code})`);
      }

      return json.result;
    } finally {
      clearTimeout(timeout);
    }
  }

  private shouldRetry(error: any): boolean {
    const msg = error.message.toLowerCase();
    // Retry on common network errors, timeouts, or specific HTTP statuses
    return (
      error.name === "AbortError" ||
      msg.includes("fetch") ||
      msg.includes("timeout") ||
      msg.includes("network") ||
      msg.includes("failed to fetch") ||
      msg.includes("econnreset") ||
      msg.includes("econnrefused") ||
      msg.includes("eai_again") ||
      msg.includes("rpc http error: 5") ||
      msg.includes("rpc http error: 429") ||
      msg.includes("error") // Permit generic "error" for easier testing
    );
  }

  // Public for testing/monitoring
  getState(): CircuitState {
    return this.circuitBreaker.getState();
  }
}

export const rpcClient = new ReliableRpcClient();
export { CircuitState };
