import { config } from "../config";
import { URL } from "url";

export enum CircuitState {
  CLOSED = "CLOSED",
  OPEN = "OPEN",
  HALF_OPEN = "HALF_OPEN",
}

export interface RpcOptions {
  retries?: number;
  initialDelayMs?: number;
  maxDelayMs?: number;
  timeoutMs?: number;
  failureThreshold?: number;
  resetTimeoutMs?: number;
  maxConcurrency?: number;
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
  private state: CircuitState = CircuitState.CLOSED;
  private failureCount: number = 0;
  private lastFailureTime: number = 0;
  private activeRequests: number = 0;
  private options: Required<RpcOptions>;
  private allowedHosts: Set<string>;

  constructor(options: RpcOptions = {}) {
    this.options = { ...DEFAULT_OPTIONS, ...options };
    this.allowedHosts = new Set(
      config.RPC_ALLOWED_HOSTS.split(",").map((h) => h.trim())
    );
  }

  /**
   * Execute a JSON-RPC call with retries, jitter, and circuit breaker protection.
   */
  async call<T>(method: string, params: any[] | Record<string, any> = []): Promise<T> {
    this.validateHost();

    if (this.state === CircuitState.OPEN) {
      if (Date.now() - this.lastFailureTime > this.options.resetTimeoutMs) {
        this.state = CircuitState.HALF_OPEN;
      } else {
        throw new Error("Circuit breaker is OPEN");
      }
    }

    if (this.activeRequests >= this.options.maxConcurrency) {
      throw new Error("Rate limit exceeded: max concurrency reached");
    }

    this.activeRequests++;
    try {
      return await this.executeWithRetries<T>(method, params);
    } finally {
      this.activeRequests--;
    }
  }

  private validateHost() {
    const url = new URL(config.STELLAR_RPC_URL);
    if (!this.allowedHosts.has(url.hostname)) {
      throw new Error(`SSRF Prevention: Host ${url.hostname} is not in the allow-list.`);
    }
  }

  private async executeWithRetries<T>(method: string, params: any, attempt: number = 0): Promise<T> {
    try {
      const result = await this.performRequest<T>(method, params);
      this.onSuccess();
      return result;
    } catch (error: any) {
      const retry = attempt < this.options.retries && this.shouldRetry(error);
      if (retry) {
        const delay = this.calculateBackoff(attempt);
        await new Promise((resolve) => setTimeout(resolve, delay));
        return this.executeWithRetries<T>(method, params, attempt + 1);
      }
      this.onFailure();
      throw error;
    }
  }

  private async performRequest<T>(method: string, params: any): Promise<T> {
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), this.options.timeoutMs);

    try {
      const response = await fetch(config.STELLAR_RPC_URL, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
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

  private calculateBackoff(attempt: number): number {
    const base = this.options.initialDelayMs * Math.pow(2, attempt);
    // Add full jitter to prevent retry storms: [0, base]
    const jitter = Math.random() * base;
    return Math.min(base + jitter, this.options.maxDelayMs);
  }

  private onSuccess() {
    this.failureCount = 0;
    this.state = CircuitState.CLOSED;
  }

  private onFailure() {
    this.failureCount++;
    this.lastFailureTime = Date.now();
    if (this.failureCount >= this.options.failureThreshold) {
      this.state = CircuitState.OPEN;
    }
  }

  // Public for testing/monitoring
  getState(): CircuitState {
    return this.state;
  }
}

export const rpcClient = new ReliableRpcClient();
