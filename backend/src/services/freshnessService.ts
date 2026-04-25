import { FreshnessMetadata } from "../types/contract";

/**
 * Seconds per Stellar ledger close (protocol constant used for lag estimation).
 * Intentionally a constant — no internal node URLs or topology are exposed.
 */
const AVG_LEDGER_CLOSE_SECS = 5;

export class FreshnessService {
  private static instance: FreshnessService;

  // Injected in tests to produce deterministic output.
  private mockNowMs: number | null = null;
  private mockLastIndexedLedger: number | null = null;
  private mockChainTipLedger: number | null = null;

  private constructor() {}

  public static getInstance(): FreshnessService {
    if (!FreshnessService.instance) {
      FreshnessService.instance = new FreshnessService();
    }
    return FreshnessService.instance;
  }

  // ── Test helpers ────────────────────────────────────────────────────────────

  public setMockNowMs(ms: number | null): void {
    this.mockNowMs = ms;
  }

  public setMockLastIndexedLedger(seq: number | null): void {
    this.mockLastIndexedLedger = seq;
  }

  public setMockChainTipLedger(seq: number | null): void {
    this.mockChainTipLedger = seq;
  }

  // ── Core ────────────────────────────────────────────────────────────────────

  /**
   * Build freshness metadata for the current indexer state.
   *
   * @param offset - pagination offset to embed in the cursor (default 0)
   */
  public getFreshness(offset = 0): FreshnessMetadata {
    const nowMs = this.mockNowMs ?? Date.now();
    const lastIndexedLedger = this.mockLastIndexedLedger ?? this.defaultLastIndexedLedger();
    const chainTipLedger = this.mockChainTipLedger ?? this.defaultChainTipLedger(nowMs);

    const lagLedgers = Math.max(0, chainTipLedger - lastIndexedLedger);
    const indexLagSeconds = lagLedgers * AVG_LEDGER_CLOSE_SECS;

    // lastUpdatedAt: wall-clock time minus the lag
    const lastUpdatedAtMs = nowMs - indexLagSeconds * 1000;
    const lastUpdatedAt = new Date(lastUpdatedAtMs).toISOString().replace(/\.\d{3}Z$/, "Z");

    const cursor = buildCursor(lastIndexedLedger, offset);

    return { lastIndexedLedger, indexLagSeconds, lastUpdatedAt, cursor };
  }

  // ── Private defaults (no internal topology exposed) ─────────────────────────

  private defaultLastIndexedLedger(): number {
    // Simulates a slowly advancing indexer. In production this comes from the DB.
    return 100000 + Math.floor((Date.now() % 3_600_000) / 5000);
  }

  private defaultChainTipLedger(nowMs: number): number {
    // Simulates the chain tip advancing every ~5 s.
    return 100000 + Math.floor((nowMs % 3_600_000) / 5000);
  }
}

/**
 * Encode a cursor as `"<ledger_seq>_<offset>"`.
 * Opaque to clients — they must not parse it.
 */
export function buildCursor(ledgerSeq: number, offset: number): string {
  return `${ledgerSeq}_${offset}`;
}

/**
 * Decode a cursor produced by {@link buildCursor}.
 * Returns `[ledgerSeq, offset]` or `null` if malformed.
 */
export function parseCursor(cursor: string): [number, number] | null {
  const parts = cursor.split("_");
  if (parts.length !== 2) return null;
  const seq = parseInt(parts[0], 10);
  const off = parseInt(parts[1], 10);
  if (!Number.isFinite(seq) || !Number.isFinite(off)) return null;
  return [seq, off];
}

export const freshnessService = FreshnessService.getInstance();
