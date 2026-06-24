import { FreshnessMetadata } from "../types/contract";
import { getDatabase } from "../lib/database";

const AVG_LEDGER_CLOSE_SECS = 5;
const DEBOUNCE_MS = 100;

export class FreshnessService {
  private static instance: FreshnessService;

  private mockNowMs: number | null = null;
  private mockLastIndexedLedger: number | null = null;
  private mockChainTipLedger: number | null = null;

  private persistedLastIndexedLedger: number | null = null;
  private persistedLastUpdatedAt: string | null = null;
  private pendingPersistTimer: NodeJS.Timeout | null = null;
  private pendingPersistPayload: { cursor: string; timestamp: string } | null = null;
  private pendingPersistPromise: Promise<void> | null = null;

  private constructor() {}

  public static getInstance(): FreshnessService {
    if (!FreshnessService.instance) {
      FreshnessService.instance = new FreshnessService();
    }
    return FreshnessService.instance;
  }

  public setMockNowMs(ms: number | null): void {
    this.mockNowMs = ms;
  }

  public setMockLastIndexedLedger(seq: number | null): void {
    this.mockLastIndexedLedger = seq;
  }

  public setMockChainTipLedger(seq: number | null): void {
    this.mockChainTipLedger = seq;
  }

  public resetForTests(): void {
    this.mockNowMs = null;
    this.mockLastIndexedLedger = null;
    this.mockChainTipLedger = null;
    this.persistedLastIndexedLedger = null;
    this.persistedLastUpdatedAt = null;
    if (this.pendingPersistTimer) {
      clearTimeout(this.pendingPersistTimer);
      this.pendingPersistTimer = null;
    }
    this.pendingPersistPayload = null;
    this.pendingPersistPromise = null;
  }

  public async initialize(): Promise<void> {
    const db = getDatabase();
    try {
      const row = db.prepare("SELECT cursor, timestamp FROM freshness_state WHERE id = 1").get();
      if (row?.cursor && row?.timestamp) {
        this.persistedLastIndexedLedger = parseCursor(row.cursor)?.[0] ?? null;
        this.persistedLastUpdatedAt = row.timestamp;
      }
    } catch {
      this.persistedLastIndexedLedger = null;
      this.persistedLastUpdatedAt = null;
    }
  }

  public updateFreshness(cursor: string, timestamp: string): void {
    const parsed = parseCursor(cursor);
    const ledger = parsed?.[0];
    if (ledger !== undefined && !Number.isNaN(ledger)) {
      this.persistedLastIndexedLedger = ledger;
    }
    this.persistedLastUpdatedAt = timestamp;
    this.pendingPersistPayload = { cursor, timestamp };

    if (this.pendingPersistTimer) {
      clearTimeout(this.pendingPersistTimer);
    }

    this.pendingPersistTimer = setTimeout(() => {
      const payload = this.pendingPersistPayload;
      if (!payload) {
        this.pendingPersistTimer = null;
        return;
      }
      this.pendingPersistPromise = this.persistValue(payload.cursor, payload.timestamp)
        .catch((err) => {
          console.error("[freshnessService] unhandled async timer persistence failed:", err?.message || err);
        })
        .finally(() => {
          this.pendingPersistPromise = null;
        });
      this.pendingPersistTimer = null;
    }, DEBOUNCE_MS);
  }

  public async flush(): Promise<void> {
    if (this.pendingPersistTimer) {
      clearTimeout(this.pendingPersistTimer);
      this.pendingPersistTimer = null;
    }

    if (this.pendingPersistPayload) {
      const { cursor, timestamp } = this.pendingPersistPayload;
      this.pendingPersistPayload = null;
      await this.persistValue(cursor, timestamp);
    }

    if (this.pendingPersistPromise) {
      await this.pendingPersistPromise;
      this.pendingPersistPromise = null;
    }
  }

  private async persistValue(cursor: string, timestamp: string): Promise<void> {
    const db = getDatabase();
    try {
      db.prepare(
        `INSERT INTO freshness_state (id, cursor, timestamp)
         VALUES (1, ?, ?)
         ON CONFLICT(id) DO UPDATE SET cursor = excluded.cursor, timestamp = excluded.timestamp`
      ).run(cursor, timestamp);
    } catch (err: any) {
      console.error("[freshnessService] failed to persist freshness state:", err?.message ?? err);
      throw err;
    }
  }

  public getFreshness(offset = 0): FreshnessMetadata {
    const nowMs = this.mockNowMs ?? Date.now();
    const lastIndexedLedger = this.mockLastIndexedLedger ?? this.persistedLastIndexedLedger ?? this.defaultLastIndexedLedger();
    const chainTipLedger = this.mockChainTipLedger ?? this.defaultChainTipLedger(nowMs);

    const lagLedgers = Math.max(0, chainTipLedger - lastIndexedLedger);
    const indexLagSeconds = lagLedgers * AVG_LEDGER_CLOSE_SECS;

    const lastUpdatedAt = this.persistedLastUpdatedAt ?? new Date(nowMs - indexLagSeconds * 1000).toISOString().replace(/\.\d{3}Z$/, "Z");

    const cursor = buildCursor(lastIndexedLedger, offset);

    return { lastIndexedLedger, indexLagSeconds, lastUpdatedAt, cursor };
  }

  private defaultLastIndexedLedger(): number {
    return 100000 + Math.floor((Date.now() % 3_600_000) / 5000);
  }

  private defaultChainTipLedger(nowMs: number): number {
    return 100000 + Math.floor((nowMs % 3_600_000) / 5000);
  }
}

export function buildCursor(ledgerSeq: number, offset: number): string {
  return `${ledgerSeq}_${offset}`;
}

export function parseCursor(cursor: string): [number, number] | null {
  const parts = cursor.split("_");
  if (parts.length !== 2) return null;
  const seq = parseInt(parts[0], 10);
  const off = parseInt(parts[1], 10);
  if (!Number.isFinite(seq) || !Number.isFinite(off)) return null;
  return [seq, off];
}

export const freshnessService = FreshnessService.getInstance();
