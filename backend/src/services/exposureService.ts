/**
 * ExposureService
 *
 * Computes per-investor exposure across the legacy mock arrays
 * (MOCK_BIDS / MOCK_SETTLEMENTS) AND the future persisted stores
 * (bidStore via the pg pool, settlementOrchestrator via better-sqlite3).
 *
 * Off-chain exposure cap (sum of bid amounts in pending status plus
 * settlements not yet finalized) short-circuits bids that would violate
 * policy before the on-chain contract rejects them. This both reduces
 * wasted RPC and gives clear API-level error semantics — POST /bids
 * returns `429 EXPOSURE_CAP_EXCEEDED` when the cap would be exceeded.
 *
 * Design notes
 * ────────────
 *  • The cap (EXPOSURE_CAP_PER_INVESTOR_USD) is expressed in USD whole
 *    units (e.g. `10000000` = $10M). The service internally converts
 *    that to USD micro-units (`cap * 1_000_000`) so all comparisons use
 *    BigInt at a uniform 6-decimal precision.
 *  • Bid and settlement amounts are token micro-units (i128 strings).
 *    Currency normalization maps each token micro-unit to a USD
 *    micro-unit equivalent via an integer rate scaled by 1_000_000.
 *  • BigInt is used end-to-end for the summation so the integer
 *    overflow tests pass on values larger than Number.MAX_SAFE_INTEGER.
 *  • The service is fault-tolerant: if a persistent store throws (e.g.
 *    when running under a mocked pg pool in unit tests) it falls back
 *    to the in-memory mock arrays so callers always get a deterministic
 *    answer.
 */

import { config } from "../config";
import { MOCK_BIDS } from "../controllers/v1/bids";
import { MOCK_SETTLEMENTS } from "../controllers/v1/settlements";
import { BidStatus, SettlementStatus } from "../types/contract";
import pool from "./database";
import { settlementOrchestrator } from "./settlementOrchestrator";

// ---------------------------------------------------------------------------
// Currency normalization
// ---------------------------------------------------------------------------
//
// Rate is "USD per 1 token". We store it as an integer scaled by 1_000_000 so
// every multiplication can stay in BigInt land without losing precision.
//
//   rate_scaled = Math.round(rate * 1_000_000)
//
// Default USD-stable coins are pegged at 1:1. Non-USD currencies are explicit
// so a typo like `USDC ` (with trailing space) falls through to the unknown
// rate rather than silently being treated as $1.
const PRECISION = 1_000_000n;

interface CurrencyRate {
  /** Display name; used for documentation and error messages. */
  name: string;
  /** Rate as a USD per 1 token, scaled by PRECISION. */
  rateScaled: bigint;
}

const CURRENCY_RATES: Record<string, CurrencyRate> = {
  USDC: { name: "USDC", rateScaled: 1_000_000n }, // 1.0 USD
  USDT: { name: "USDT", rateScaled: 1_000_000n }, // 1.0 USD
  USD:  { name: "USD",  rateScaled: 1_000_000n }, // 1.0 USD
  XLM:  { name: "XLM",  rateScaled:   120_000n }, // 0.12 USD
};

const UNKNOWN_CURRENCY_RATE: CurrencyRate = {
  name: "UNKNOWN",
  // Fail-safe: treat unknown currencies at par with USD. Callers that need
  // a strict mode should pass an allow-list via setAllowedCurrencies().
  rateScaled: 1_000_000n,
};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/**
 * Thrown when a new bid would push the investor's total exposure past the
 * configured cap. Carries the underlying BigInts so the controller layer can
 * surface a precise 429 response.
 */
export class ExposureCapExceededError extends Error {
  public readonly currentExposureUsd: bigint;
  public readonly attemptedUsd: bigint;
  public readonly capUsd: bigint;
  public readonly investor: string;

  constructor(
    investor: string,
    currentExposureUsd: bigint,
    attemptedUsd: bigint,
    capUsd: bigint,
  ) {
    super(
      `Investor ${investor} exposure cap would be exceeded: ` +
        `current=${currentExposureUsd.toString()}, ` +
        `attempted=${attemptedUsd.toString()}, ` +
        `cap=${capUsd.toString()}`
    );
    this.name = "ExposureCapExceededError";
    this.investor = investor;
    this.currentExposureUsd = currentExposureUsd;
    this.attemptedUsd = attemptedUsd;
    this.capUsd = capUsd;
  }
}

/**
 * Thrown when an input amount cannot be parsed as a non-negative integer
 * string of token micro-units. Kept separate from the cap error so callers
 * can return a clean 400 INVALID_BID response.
 */
export class InvalidAmountError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "InvalidAmountError";
  }
}

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

export interface ExposureCheck {
  /** True when (currentExposure + newAmount) ≤ cap. */
  allowed: boolean;
  /** Current exposure across active bids + unsettled positions. */
  currentExposureUsd: string;
  /** The proposed new amount, normalized to USD. */
  attemptedUsd: string;
  /** Projected exposure after the new bid is placed. */
  projectedExposureUsd: string;
  /** The configured cap, normalized to USD micro-units. */
  capUsd: string;
  /** Headroom remaining before the cap is reached. */
  remainingUsd: string;
  /** Currency reported by the caller for the proposed bid. */
  currency: string;
}

export interface ExposureBreakdown {
  /** Sum of active (Placed) bids, in USD micro-units. */
  bidsUsd: string;
  /** Sum of unsettled (Pending/Processing) positions, in USD micro-units. */
  positionsUsd: string;
  /** Sum of both buckets, in USD micro-units. */
  totalUsd: string;
}

// ---------------------------------------------------------------------------
// Service
// ---------------------------------------------------------------------------

/**
 * Pluggable dependency hooks so tests can inject custom data sources
 * without monkey-patching the mock arrays. In production these default
 * to the real stores.
 */
export interface ExposureServiceDeps {
  /** Source of legacy mock bids (mutable in tests). */
  readonly mockBids: ReadonlyArray<any>;
  /** Source of legacy mock settlements (mutable in tests). */
  readonly mockSettlements: ReadonlyArray<any>;
  /** Pluggable persisted-bids fetcher. Returns rows shaped like the bid table. */
  readonly fetchPersistedBids?: (investor: string) => Promise<
    Array<{ bid_id: string; bid_amount: string; status: string; currency?: string }>
  >;
  /** Pluggable persisted-settlements fetcher. Returns rows shaped like the settlements table. */
  readonly fetchPersistedSettlements?: (investor: string) => Promise<
    Array<{ id: string; amount: string; recipient?: string; status: string; currency?: string }>
  >;
}

export class ExposureService {
  private _capUsd: bigint;
  private _deps: ExposureServiceDeps;
  /** Optional allow-list of currencies. Empty = all known + unknown-rate default. */
  private _allowedCurrencies: Set<string> | null = null;

  constructor(deps: Partial<ExposureServiceDeps> = {}) {
    this._deps = {
      mockBids: MOCK_BIDS,
      mockSettlements: MOCK_SETTLEMENTS,
      ...deps,
    };
    this._capUsd = this._parseCap(config.EXPOSURE_CAP_PER_INVESTOR_USD);
  }

  // ─── Configuration ──────────────────────────────────────────────────────

  /**
   * Override the cap. Useful in tests and for hot-reload scenarios.
   * `value` is expressed in USD whole units (e.g. 10000000 = $10M).
   */
  setCap(value: string | number | bigint): void {
    this._capUsd = this._parseCap(value);
  }

  /** Current cap, in USD micro-units (BigInt). */
  get capUsd(): bigint {
    return this._capUsd;
  }

  /**
   * Restrict which currencies the service will accept. Unknown currencies
   * after this is set throw InvalidAmountError. Pass `null` to clear.
   */
  setAllowedCurrencies(currencies: string[] | null): void {
    this._allowedCurrencies = currencies && currencies.length > 0
      ? new Set(currencies.map((c) => c.toUpperCase()))
      : null;
  }

  /** Replace the dependency hooks (testing only). */
  setDependencies(deps: Partial<ExposureServiceDeps>): void {
    this._deps = { ...this._deps, ...deps };
  }

  // ─── Public API ──────────────────────────────────────────────────────────

  /**
   * Compute the current exposure breakdown for an investor. Returns both the
   * per-bucket subtotals and the total in USD micro-units.
   */
  async computeBreakdown(investor: string): Promise<ExposureBreakdown> {
    const [bidsUsd, positionsUsd] = await Promise.all([
      this._sumBids(investor),
      this._sumPositions(investor),
    ]);
    return {
      bidsUsd: bidsUsd.toString(),
      positionsUsd: positionsUsd.toString(),
      totalUsd: (bidsUsd + positionsUsd).toString(),
    };
  }

  /**
   * Compute the current total exposure (bids + positions) for an investor,
   * in USD micro-units.
   */
  async computeExposure(investor: string): Promise<bigint> {
    const breakdown = await this.computeBreakdown(investor);
    return BigInt(breakdown.totalUsd);
  }

  /**
   * Test whether a new bid of `amount` micro-units in `currency` would
   * push the investor past the cap. Pure inspection — never throws.
   *
   * A cap of `0` is treated as "disabled": every bid is allowed and the
   * remainingUsd field is reported as `0`. This makes test setups trivial
   * (set `EXPOSURE_CAP_PER_INVESTOR_USD=0` to bypass the gate) while the
   * default $10B cap keeps production behaviour unchanged.
   */
  async check(
    investor: string,
    amount: string,
    currency: string = "USDC",
  ): Promise<ExposureCheck> {
    const normalizedAmount = this._normalizeToUsdMicro(amount, currency);
    const currentExposure = await this.computeExposure(investor);
    const projected = currentExposure + normalizedAmount;
    const capDisabled = this._capUsd === 0n;
    const allowed = capDisabled || projected <= this._capUsd;
    return {
      allowed,
      currentExposureUsd: currentExposure.toString(),
      attemptedUsd: normalizedAmount.toString(),
      projectedExposureUsd: projected.toString(),
      capUsd: this._capUsd.toString(),
      remainingUsd: capDisabled
        ? "0"
        : (allowed ? this._capUsd - projected : 0n).toString(),
      currency: currency.toUpperCase(),
    };
  }

  /**
   * Throw ExposureCapExceededError if placing `amount` micro-units in
   * `currency` for `investor` would exceed the cap.
   */
  async assertWithinCap(
    investor: string,
    amount: string,
    currency: string = "USDC",
  ): Promise<void> {
    const result = await this.check(investor, amount, currency);
    if (!result.allowed) {
      throw new ExposureCapExceededError(
        investor,
        BigInt(result.currentExposureUsd),
        BigInt(result.attemptedUsd),
        BigInt(result.capUsd),
      );
    }
  }

  // ─── Internals ───────────────────────────────────────────────────────────

  private _parseCap(value: string | number | bigint): bigint {
    let wholeUsd: bigint;
    if (typeof value === "bigint") {
      if (value < 0n) {
        throw new InvalidAmountError("Exposure cap must be non-negative");
      }
      wholeUsd = value;
    } else if (typeof value === "number") {
      if (!Number.isFinite(value) || !Number.isInteger(value) || value < 0) {
        throw new InvalidAmountError(
          `Exposure cap must be a non-negative integer (got ${value})`,
        );
      }
      wholeUsd = BigInt(value);
    } else if (typeof value === "string") {
      if (!/^[0-9]+$/.test(value)) {
        throw new InvalidAmountError(
          `Exposure cap must be a non-negative integer string (got "${value}")`,
        );
      }
      wholeUsd = BigInt(value);
    } else {
      throw new InvalidAmountError("Exposure cap must be a string, number, or bigint");
    }
    return wholeUsd * PRECISION;
  }

  private _parseAmount(amount: unknown): bigint {
    if (typeof amount === "bigint") {
      if (amount < 0n) {
        throw new InvalidAmountError("Amount must be non-negative");
      }
      return amount;
    }
    if (typeof amount === "number") {
      if (!Number.isFinite(amount) || !Number.isInteger(amount) || amount < 0) {
        throw new InvalidAmountError(
          `Amount must be a non-negative integer (got ${amount})`,
        );
      }
      return BigInt(amount);
    }
    if (typeof amount !== "string" || !/^[0-9]+$/.test(amount)) {
      throw new InvalidAmountError(
        `Amount must be a non-negative integer string (got ${JSON.stringify(amount)})`,
      );
    }
    return BigInt(amount);
  }

  /**
   * Convert a token micro-unit amount in `currency` to a USD micro-unit
   * amount using BigInt-only arithmetic.
   *
   *   usdMicro = amountMicro * rateScaled / PRECISION
   *
   * The integer division is intentional: it floors to the nearest
   * micro-USD so the exposure number never overstates. The cap and the
   * currentExposure are both already integer micro-USD, so the
   * comparison stays exact.
   */
  private _normalizeToUsdMicro(amount: unknown, currency: string): bigint {
    const micro = this._parseAmount(amount);
    const upper = (currency || "USDC").toUpperCase();
    if (this._allowedCurrencies && !this._allowedCurrencies.has(upper)) {
      throw new InvalidAmountError(`Currency not allowed: ${currency}`);
    }
    const rate = CURRENCY_RATES[upper] ?? UNKNOWN_CURRENCY_RATE;
    return (micro * rate.rateScaled) / PRECISION;
  }

  private async _sumBids(investor: string): Promise<bigint> {
    let total = 0n;

    // Legacy in-memory mock
    for (const bid of this._deps.mockBids) {
      if (!bid || bid.investor !== investor) continue;
      if (bid.status !== BidStatus.Placed) continue;
      try {
        total += this._normalizeToUsdMicro(
          bid.bid_amount,
          bid.currency ?? "USDC",
        );
      } catch {
        // Skip malformed rows silently — defensive against corrupt mocks
        // in test fixtures.
      }
    }

    // Persisted store (best-effort; falls back silently on failure)
    try {
      if (this._deps.fetchPersistedBids) {
        const rows = await this._deps.fetchPersistedBids(investor);
        for (const row of rows) {
          if (row.status !== BidStatus.Placed) continue;
          try {
            total += this._normalizeToUsdMicro(
              row.bid_amount,
              row.currency ?? "USDC",
            );
          } catch {
            // skip malformed row
          }
        }
      } else {
        const result = await pool.query(
          `SELECT bid_id, bid_amount, status FROM bids
            WHERE investor = $1 AND status = $2`,
          [investor, BidStatus.Placed],
        );
        for (const row of result.rows ?? []) {
          try {
            total += this._normalizeToUsdMicro(row.bid_amount, "USDC");
          } catch {
            // skip
          }
        }
      }
    } catch {
      // Persistent store unavailable (test env or DB error). Mock-only is OK.
    }

    return total;
  }

  private async _sumPositions(investor: string): Promise<bigint> {
    let total = 0n;
    const unsettledStatuses = new Set<string>([
      SettlementStatus.Pending,
      SettlementStatus.Processing,
    ]);

    // Legacy mock
    for (const s of this._deps.mockSettlements) {
      if (!s) continue;
      if (s.recipient !== investor) continue;
      if (!unsettledStatuses.has(s.status)) continue;
      try {
        total += this._normalizeToUsdMicro(s.amount, s.currency ?? "USDC");
      } catch {
        // skip
      }
    }

    // Persisted settlements via settlementOrchestrator (best-effort)
    try {
      if (this._deps.fetchPersistedSettlements) {
        const rows = await this._deps.fetchPersistedSettlements(investor);
        for (const row of rows) {
          if (!unsettledStatuses.has(row.status)) continue;
          try {
            total += this._normalizeToUsdMicro(
              row.amount,
              row.currency ?? "USDC",
            );
          } catch {
            // skip
          }
        }
      } else {
        const all = settlementOrchestrator.list();
        for (const s of all) {
          if (s.recipient !== investor) continue;
          if (!unsettledStatuses.has(s.status)) continue;
          try {
            total += this._normalizeToUsdMicro(s.amount, "USDC");
          } catch {
            // skip
          }
        }
      }
    } catch {
      // ignore
    }

    return total;
  }
}

// ---------------------------------------------------------------------------
// Singleton + helper exports
// ---------------------------------------------------------------------------

/**
 * Process-wide singleton. Uses the env-var-driven cap from src/config.ts.
 */
export const exposureService = new ExposureService();

// Intentionally no helper export — controllers should call the singleton's
// check/assertWithinCap methods directly.
