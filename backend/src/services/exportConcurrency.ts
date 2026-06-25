import { config } from "../config";

interface ConcurrencyState {
  activeExports: Map<string, number>;
}

class ExportConcurrencyService {
  private state: ConcurrencyState = {
    activeExports: new Map(),
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
   * Release a slot for an export job for a given API key.
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
  }
}

export const exportConcurrencyService = new ExportConcurrencyService();