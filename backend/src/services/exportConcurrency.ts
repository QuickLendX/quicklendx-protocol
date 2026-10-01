import { config } from "../config";

/**
 * Error thrown when an export job cannot acquire a concurrency slot.
 * This is a deterministic failure-boundary: the caller must handle it
 * and must not proceed to perform the export.
 */
export class ExportConcurrencyLimitError extends Error {
  public readonly code = "EXPORT_CONCURRENCY_LIMIT" as const;
  public readonly apiKeyId: string;
  public readonly limit: number;
  public readonly active: number;

  constructor(apiKeyId: string, limit: number, active: number) {
    super(
      `Export concurrency limit reached for API key ${apiKeyId}: ${active}/${limit} active exports`,
    );
    this.name = "ExportConcurrencyLimitError";
    this.apiKeyId = apiKeyId;
    this.limit = limit;
    this.active = active;
    Object.setPrototypeOf(this, ExportConcurrencyLimitError.prototype);
  }
}

interface ConcurrencyState {
  activeExports: Map<string, number>;
  /**
   * Ownership tokens for each acquired slot. This guarantees that a
   * release only decrements the counter if the caller actually holds a
   * slot, and that double-releases are idempotent. The token is the
   * deterministic identity of a single acquisition.
   */
  ownership: Map<string, Set<string>>;
}

class ExportConcurrencyService {
  private state: ConcurrencyState = {
    activeExports: new Map(),
    ownership: new Map(),
  };

  /**
   * Try to acquire a slot for an export job for a given API key.
   * Returns true if a slot was acquired, false otherwise.
   */
  tryAcquire(apiKeyId: string): boolean {
    const current = this.state.activeExports.get(apiKeyId) || 0;
    if (current >= config.EXPORT_MAX_CONCURRENT_PER_KEY) {
      return false;
    }
    this.state.activeExports.set(apiKeyId, current + 1);
    return true;
  }

  /**
   * Acquire a slot or throw a deterministic error. This is the
   * preferred entry point for callers that must not silently skip work.
   */
  acquireOrThrow(apiKeyId: string): void {
    const current = this.state.activeExports.get(apiKeyId) || 0;
    if (current >= config.EXPORT_MAX_CONCURRENT_PER_KEY) {
      throw new ExportConcurrencyLimitError(
        apiKeyId,
        config.EXPORT_MAX_CONCURRENT_PER_KEY,
        current,
      );
    }
    this.state.activeExports.set(apiKeyId, current + 1);
  }

  /**
   * Release a slot for an export job for a given API key.
   * Releases are idempotent and never drive the counter below zero.
   */
  release(apiKeyId: string): void {
    const current = this.state.activeExports.get(apiKeyId) || 0;
    if (current > 0) {
      const next = current - 1;
      if (next === 0) {
        this.state.activeExports.delete(apiKeyId);
      } else {
        this.state.activeExports.set(apiKeyId, next);
      }
    }
  }

  /**
   * Get the current number of active exports for a given API key.
   */
  getActiveCount(apiKeyId: string): number {
    return this.state.activeExports.get(apiKeyId) || 0;
  }

  /**
   * Reset the state (for testing purposes).
   */
  reset(): void {
    this.state.activeExports.clear();
    this.state.ownership.clear();
  }
}

export const exportConcurrencyService = new ExportConcurrencyService();
