import * as dns from "node:dns";
import { IngestionStore, IndexedEvent } from "../../services/ingestion";

/**
 * FaultyIngestionStore wraps an existing IngestionStore and allows
 * injecting database errors, such as complete or partial commit failures.
 */
export class FaultyIngestionStore implements IngestionStore {
  private target: IngestionStore;
  private shouldFailCommit = false;
  private failCommitError = new Error("Database transaction failed");
  private partialCommitCount = 0;

  private shouldFailGetCursor = false;
  private failGetCursorError = new Error("Database read failed");

  private shouldFailRollback = false;
  private failRollbackError = new Error("Database rollback failed");

  constructor(target: IngestionStore) {
    this.target = target;
  }

  setShouldFailCommit(fail: boolean, error?: Error) {
    this.shouldFailCommit = fail;
    if (error) this.failCommitError = error;
  }

  setPartialCommitCount(count: number) {
    this.partialCommitCount = count;
  }

  setShouldFailGetCursor(fail: boolean, error?: Error) {
    this.shouldFailGetCursor = fail;
    if (error) this.failGetCursorError = error;
  }

  setShouldFailRollback(fail: boolean, error?: Error) {
    this.shouldFailRollback = fail;
    if (error) this.failRollbackError = error;
  }

  async getCursor(): Promise<number | null> {
    if (this.shouldFailGetCursor) {
      throw this.failGetCursorError;
    }
    return this.target.getCursor();
  }

  async commitBatch(events: IndexedEvent[], newCursor: number): Promise<void> {
    if (this.shouldFailCommit) {
      if (this.partialCommitCount > 0) {
        // Simulating a partial commit: write some events but do not advance the cursor.
        const currentCursor = await this.target.getCursor();
        const partialEvents = events.slice(0, this.partialCommitCount);
        // Note: we pass the old cursor directly (even if null) so the cursor doesn't advance.
        await this.target.commitBatch(partialEvents, currentCursor as any);
      }
      throw this.failCommitError;
    }
    await this.target.commitBatch(events, newCursor);
  }

  async rollbackTo(cursor: number): Promise<void> {
    if (this.shouldFailRollback) {
      throw this.failRollbackError;
    }
    await this.target.rollbackTo(cursor);
  }
}

/**
 * FaultyFetch overrides the global fetch function to simulate
 * transient network failures, latency, or custom HTTP responses.
 */
export class FaultyFetch {
  private originalFetch = globalThis.fetch;
  private nextFailures: Array<{ error?: Error; status?: number; body?: string }> = [];

  setup() {
    globalThis.fetch = async (input, init) => {
      if (this.nextFailures.length > 0) {
        const failure = this.nextFailures.shift()!;
        if (failure.error) {
          throw failure.error;
        }
        const status = failure.status ?? 200;
        const body = failure.body ?? "{}";

        // Duck-typing Response to avoid environment discrepancies
        const mockResponse = {
          ok: status >= 200 && status < 300,
          status,
          statusText: status === 200 ? "OK" : "Error",
          json: async () => JSON.parse(body),
          text: async () => body,
        } as unknown as Response;

        return mockResponse;
      }
      return this.originalFetch(input, init);
    };
  }

  queueFailure(options: { error?: Error; status?: number; body?: string }) {
    this.nextFailures.push(options);
  }

  restore() {
    globalThis.fetch = this.originalFetch;
    this.nextFailures = [];
  }
}

const mutableDns = require("node:dns");
export const originalDnsLookup = mutableDns.lookup;

/**
 * FaultyDns overrides dns.lookup to mock DNS resolution failures
 * or return specific IPs (e.g., private/blocked addresses) for webhook testing.
 */
export class FaultyDns {
  private originalLookup = mutableDns.lookup;
  private resolveToIps: string[] = [];
  private shouldFail = false;
  private isOverridden = false;

  setup(resolveToIps: string[] = [], shouldFail = false) {
    this.resolveToIps = resolveToIps;
    this.shouldFail = shouldFail;

    if (!this.isOverridden) {
      this.originalLookup = mutableDns.lookup;
      this.isOverridden = true;
    }

    mutableDns.lookup = (hostname: string, options: any, callback: any) => {
      let cb = callback;
      let opts = options;
      if (typeof options === "function") {
        cb = options;
        opts = {};
      }

      if (this.shouldFail) {
        const err = new Error("ENOTFOUND: DNS lookup failed");
        (err as any).code = "ENOTFOUND";
        return cb(err, "", 0);
      }

      if (this.resolveToIps.length > 0) {
        const addresses = this.resolveToIps.map((ip) => ({
          address: ip,
          family: ip.includes(":") ? 6 : 4,
        }));
        if (opts && opts.all) {
          return cb(null, addresses);
        } else {
          return cb(null, addresses[0].address, addresses[0].family);
        }
      }

      return this.originalLookup(hostname, opts, cb);
    };
  }

  restore() {
    if (this.isOverridden) {
      mutableDns.lookup = this.originalLookup;
      this.isOverridden = false;
    }
    this.resolveToIps = [];
    this.shouldFail = false;
  }
}
