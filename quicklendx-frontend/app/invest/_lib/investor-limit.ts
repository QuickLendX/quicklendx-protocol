/**
 * investor-limit.ts: client-side pre-check of the per-investor exposure cap.
 *
 * The backend enforces the cap in `exposureService.check` and refuses a bid that
 * would exceed it with `429 EXPOSURE_CAP_EXCEEDED`. The refusal is correct, but
 * it costs a round trip and shows up as an error after the investor has filled in
 * the form. When the numbers are already known, the form can say so up front and
 * keep the submit button off.
 *
 * Everything here mirrors the server so the two cannot disagree at the edge:
 *
 *   allowed  <=>  cap is 0 (cap disabled)  OR  current + attempted <= cap
 *
 * All amounts are micro-USD as BigInt strings, the units the backend uses for
 * `currentExposureUsd`, `attemptedUsd` and `capUsd`.
 *
 * The server converts a bid to USD with a per-currency rate. Only the currencies
 * pegged 1:1 to USD are handled here (the same `USDC`, `USDT`, `USD` the server
 * pegs at 1.0). For any other currency the answer is `unknown` and the server
 * stays the only judge, because copying a moving rate table into the client
 * would just give two places to go out of date.
 */

export interface InvestorLimit {
  /** Cap in micro-USD. `"0"` means the cap is disabled. */
  capUsd: string;
  /** The investor's current exposure in micro-USD. */
  currentExposureUsd: string;
}

export type LimitCheck =
  | { status: "ok" }
  | {
      status: "exceeded";
      capUsd: string;
      currentExposureUsd: string;
      attemptedUsd: string;
      /** Headroom left under the cap before this bid, never negative. */
      remainingUsd: string;
    }
  /** No limit known, or the currency cannot be converted here. */
  | { status: "unknown" };

/** Currencies the backend pegs at exactly 1 USD. */
const PAR_CURRENCIES: ReadonlySet<string> = new Set(["USDC", "USDT", "USD"]);

function toBigInt(value: unknown): bigint | null {
  if (typeof value === "bigint") return value;
  if (typeof value === "string" && /^[0-9]+$/.test(value)) return BigInt(value);
  return null;
}

/**
 * Would a bid of `bidAmount` (the same micro-units that go on the wire) push the
 * investor past their cap?
 */
export function checkInvestorLimit(
  bidAmount: bigint,
  currency: string,
  limit?: InvestorLimit | null
): LimitCheck {
  if (!limit) return { status: "unknown" };
  if (!PAR_CURRENCIES.has(currency.trim().toUpperCase())) {
    return { status: "unknown" };
  }

  const cap = toBigInt(limit.capUsd);
  const current = toBigInt(limit.currentExposureUsd);
  if (cap === null || current === null) return { status: "unknown" };

  // A cap of 0 means "no cap", exactly as the server reads it.
  if (cap === BigInt(0)) return { status: "ok" };

  if (current + bidAmount <= cap) return { status: "ok" };

  const headroom = cap - current;
  return {
    status: "exceeded",
    capUsd: cap.toString(),
    currentExposureUsd: current.toString(),
    attemptedUsd: bidAmount.toString(),
    remainingUsd: (headroom > BigInt(0) ? headroom : BigInt(0)).toString(),
  };
}

/**
 * Read the limit out of the details of a `429 EXPOSURE_CAP_EXCEEDED` response, so
 * the next attempt can be refused locally without another request.
 */
export function parseLimitFromDetails(details: unknown): InvestorLimit | null {
  if (!details || typeof details !== "object") return null;
  const d = details as Record<string, unknown>;
  const cap = toBigInt(d.capUsd);
  const current = toBigInt(d.currentExposureUsd);
  if (cap === null || current === null) return null;
  return { capUsd: cap.toString(), currentExposureUsd: current.toString() };
}
