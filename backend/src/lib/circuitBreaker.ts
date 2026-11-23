export enum CircuitState {
  CLOSED = "CLOSED",
  OPEN = "OPEN",
  HALF_OPEN = "HALF_OPEN",
}

export interface CircuitBreakerOptions {
  retries?: number;
  initialDelayMs?: number;
  maxDelayMs?: number;
  failureThreshold?: number;
  resetTimeoutMs?: number;
  maxConcurrency?: number;
}

const DEFAULT_OPTIONS: Required<CircuitBreakerOptions> = {
  retries: 3,
  initialDelayMs: 100,
  maxDelayMs: 10000,
  failureThreshold: 5,
  resetTimeoutMs: 30000,
  maxConcurrency: 10,
};

export class CircuitBreaker {
  private state: CircuitState = CircuitState.CLOSED;
  private failureCount: number = 0;
  private lastFailureTime: number = 0;
  private activeRequests: number = 0;
  private options: Required<CircuitBreakerOptions>;

  constructor(options: CircuitBreakerOptions = {}) {
    this.options = { ...DEFAULT_OPTIONS, ...options };
  }

  /**
   * Execute an async action with retries, jitter, and circuit breaker protection.
   */
  async execute<T>(
    action: () => Promise<T>,
    shouldRetry: (error: any) => boolean = () => true
  ): Promise<T> {
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
      return await this.executeWithRetries<T>(action, shouldRetry, 0);
    } finally {
      this.activeRequests--;
    }
  }

  private async executeWithRetries<T>(
    action: () => Promise<T>,
    shouldRetry: (error: any) => boolean,
    attempt: number
  ): Promise<T> {
    try {
      const result = await action();
      this.onSuccess();
      return result;
    } catch (error: any) {
      const retry = attempt < this.options.retries && shouldRetry(error);
      if (retry) {
        const delay = this.calculateBackoff(attempt);
        await new Promise((resolve) => setTimeout(resolve, delay));
        return this.executeWithRetries<T>(action, shouldRetry, attempt + 1);
      }
      this.onFailure();
      throw error;
    }
  }

  private calculateBackoff(attempt: number): number {
    const base = this.options.initialDelayMs * Math.pow(2, attempt);
    // Add jitter to prevent retry storms: [base, base * 1.5]
    const jitter = Math.random() * (base * 0.5);
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
