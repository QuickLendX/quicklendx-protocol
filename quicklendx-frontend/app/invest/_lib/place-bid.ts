/**
 * place-bid.ts: validation, the request payload and the ranking preview for the
 * place-bid form. Pure functions only, so every rule can be tested without
 * rendering anything.
 *
 * # Units
 *
 * Amounts are whole numbers in the same smallest units the rest of the page
 * already shows (the invoice amount and the ranked bids are printed as the raw
 * i128 strings the API returns) and the same units that go on the wire in
 * `bid_amount` and `expected_return`. There is deliberately no decimal
 * conversion: the API carries no decimals metadata, and the backend (6) and
 * `math.ts` (7) disagree on how many to assume, so converting here would put a
 * guess between what the investor types and what is sent.
 *
 * # Where each rule comes from
 *
 * - Payload shape: `createBidBodySchema` (backend/src/validators/bids.ts):
 *   `bid_amount` and `expected_return` are digit strings, `expiration_timestamp`
 *   a positive integer (unix seconds).
 * - Bid at least 1, and expected return at least the bid: `BidStore.createBid`.
 * - Bid at most the invoice amount: the contract's `validate_bid`
 *   (`InvalidAmount`). The HTTP API does not check it, so this is the one rule
 *   that is enforced here and not by the server.
 * - Ranking: the `ORDER BY` in `BidStore.getBidsForInvoice`.
 */

import type { Bid } from "./view-model";

export interface CreateBidPayload {
  invoice_id: string;
  bid_amount: string;
  expected_return: string;
  expiration_timestamp: number;
}

/** What the form holds. `expiresAt` is a `datetime-local` value (local time). */
export interface PlaceBidFormValues {
  amount: string;
  expectedReturn: string;
  expiresAt: string;
}

export interface PlaceBidContext {
  invoiceId: string;
  /** Invoice face amount, an i128 string. */
  invoiceAmount: string;
  /** Current unix time in seconds. Injected so tests are deterministic. */
  nowSeconds: number;
}

export type FieldErrors = Partial<Record<keyof PlaceBidFormValues, string>>;

export type PlaceBidValidation =
  | {
      ok: true;
      payload: CreateBidPayload;
      /** The parsed amounts, for the limit check and the preview. */
      bid: bigint;
      expected: bigint;
    }
  | { ok: false; errors: FieldErrors; formError?: string };

/** An expiry closer than this to now is as good as already expired. */
export const MIN_EXPIRY_LEAD_SECONDS = 60;
/** The contract's default bid lifetime: 24 hours. */
export const DEFAULT_EXPIRY_SECONDS = 86_400;

/** Largest value an i128 can hold; anything above cannot be a valid amount. */
const I128_MAX = BigInt("170141183460469231731687303715884105727");

/** Same pattern as `createBidBodySchema.invoice_id`. */
const INVOICE_ID_PATTERN = /^(0x[a-fA-F0-9]+|inv_[0-9A-HJKMNP-TV-Z]{26})$/i;

/** Parse digits-only input to a BigInt; `null` for anything else. */
export function parseWholeNumber(input: string): bigint | null {
  const trimmed = input.trim();
  return /^[0-9]+$/.test(trimmed) ? BigInt(trimmed) : null;
}

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/** Unix seconds -> `datetime-local` value (`YYYY-MM-DDTHH:mm`, local time). */
export function toDatetimeLocal(seconds: number): string {
  const d = new Date(seconds * 1000);
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}` +
    `T${pad(d.getHours())}:${pad(d.getMinutes())}`
  );
}

/** `datetime-local` value -> unix seconds, or `null` if it is not a date. */
export function fromDatetimeLocal(value: string): number | null {
  if (!value.trim()) return null;
  const ms = new Date(value).getTime();
  return Number.isFinite(ms) ? Math.floor(ms / 1000) : null;
}

function formatForMessage(value: string): string {
  try {
    return BigInt(value).toLocaleString();
  } catch {
    return value;
  }
}

/**
 * Validate the form and, when it is valid, build the exact request body.
 *
 * The payload has exactly the four fields the API requires, and amounts are the
 * normalised digit strings (so `"007"` is sent as `"7"`).
 */
export function validatePlaceBid(
  values: PlaceBidFormValues,
  ctx: PlaceBidContext
): PlaceBidValidation {
  const errors: FieldErrors = {};
  let formError: string | undefined;

  if (!INVOICE_ID_PATTERN.test(ctx.invoiceId)) {
    formError = "This invoice has an ID the bid API does not accept.";
  }

  // ── Bid amount ────────────────────────────────────────────────────────────
  let bid: bigint | null = null;
  if (!values.amount.trim()) {
    errors.amount = "Enter a bid amount.";
  } else {
    bid = parseWholeNumber(values.amount);
    if (bid === null) {
      errors.amount = "Use whole numbers only (no decimals, commas or spaces).";
    } else if (bid < BigInt(1)) {
      errors.amount = "A bid must be at least 1.";
    } else if (bid > I128_MAX) {
      errors.amount = "That number is too large.";
    } else {
      const invoiceAmount = parseWholeNumber(ctx.invoiceAmount);
      if (invoiceAmount !== null && bid > invoiceAmount) {
        errors.amount = `A bid cannot exceed the invoice amount (${formatForMessage(ctx.invoiceAmount)}).`;
      }
    }
  }
  const bidOk = bid !== null && errors.amount === undefined;

  // ── Expected return ───────────────────────────────────────────────────────
  let expected: bigint | null = null;
  if (!values.expectedReturn.trim()) {
    errors.expectedReturn = "Enter the expected return.";
  } else {
    expected = parseWholeNumber(values.expectedReturn);
    if (expected === null) {
      errors.expectedReturn =
        "Use whole numbers only (no decimals, commas or spaces).";
    } else if (expected > I128_MAX) {
      errors.expectedReturn = "That number is too large.";
    } else if (bidOk && expected < (bid as bigint)) {
      errors.expectedReturn =
        "Expected return must be at least the bid amount.";
    }
  }
  const expectedOk = expected !== null && errors.expectedReturn === undefined;

  // ── Expiry ────────────────────────────────────────────────────────────────
  let expiry: number | null = null;
  if (!values.expiresAt.trim()) {
    errors.expiresAt = "Choose when the bid expires.";
  } else {
    expiry = fromDatetimeLocal(values.expiresAt);
    if (expiry === null) {
      errors.expiresAt = "Enter a valid date and time.";
    } else if (expiry < ctx.nowSeconds + MIN_EXPIRY_LEAD_SECONDS) {
      errors.expiresAt = "The expiry must be at least a minute from now.";
    }
  }

  if (
    Object.keys(errors).length > 0 ||
    formError ||
    !bidOk ||
    !expectedOk ||
    expiry === null
  ) {
    return { ok: false, errors, formError };
  }

  const bidAmount = bid as bigint;
  const expectedReturn = expected as bigint;
  return {
    ok: true,
    bid: bidAmount,
    expected: expectedReturn,
    payload: {
      invoice_id: ctx.invoiceId,
      bid_amount: bidAmount.toString(),
      expected_return: expectedReturn.toString(),
      expiration_timestamp: expiry,
    },
  };
}

// ---------------------------------------------------------------------------
// Ranking
// ---------------------------------------------------------------------------

/** The fields ranking looks at. */
export type RankableBid = Pick<
  Bid,
  "bid_amount" | "expected_return" | "timestamp"
> & { bid_id?: string };

function safeBig(value: string): bigint | null {
  try {
    return BigInt(value);
  } catch {
    return null;
  }
}

/**
 * Order two bids the way the server does (`BidStore.getBidsForInvoice`):
 * profit (`expected_return - bid_amount`) descending, then expected return
 * descending, then bid amount descending, then newest first, then `bid_id`
 * ascending. A negative result means `a` ranks ahead of `b`.
 */
export function compareBids(a: RankableBid, b: RankableBid): number {
  const aBid = safeBig(a.bid_amount) ?? BigInt(0);
  const bBid = safeBig(b.bid_amount) ?? BigInt(0);
  const aRet = safeBig(a.expected_return) ?? BigInt(0);
  const bRet = safeBig(b.expected_return) ?? BigInt(0);

  const profit = bRet - bBid - (aRet - aBid);
  if (profit !== BigInt(0)) return profit > BigInt(0) ? 1 : -1;
  if (aRet !== bRet) return bRet > aRet ? 1 : -1;
  if (aBid !== bBid) return bBid > aBid ? 1 : -1;
  if (a.timestamp !== b.timestamp) return b.timestamp - a.timestamp;
  return (a.bid_id ?? "").localeCompare(b.bid_id ?? "");
}

/** A copy of `bids` in server ranking order. */
export function rankBids<T extends RankableBid>(bids: readonly T[]): T[] {
  return [...bids].sort(compareBids);
}

export interface RankPreview {
  /** Position the bid would take, 1 = best. */
  rank: number;
  /** How many bids would be on the invoice including this one. */
  total: number;
}

/**
 * Where a bid would land among the invoice's current bids.
 *
 * The new bid is the newest, so on an exact tie in profit, return and amount it
 * ranks ahead (the server breaks that tie by timestamp descending). Only `Placed`
 * bids count, the same set the ranked endpoint returns.
 */
export function previewRank(
  existing: readonly Bid[],
  candidate: { bid_amount: bigint; expected_return: bigint }
): RankPreview {
  const candidateProfit = candidate.expected_return - candidate.bid_amount;
  let ahead = 0;
  let placed = 0;

  for (const bid of existing) {
    if (bid.status !== "Placed") continue;
    const b = safeBig(bid.bid_amount);
    const r = safeBig(bid.expected_return);
    if (b === null || r === null) continue;
    placed += 1;

    const profit = r - b;
    const beats =
      profit > candidateProfit ||
      (profit === candidateProfit &&
        (r > candidate.expected_return ||
          (r === candidate.expected_return && b > candidate.bid_amount)));
    if (beats) ahead += 1;
  }

  return { rank: ahead + 1, total: placed + 1 };
}

/**
 * Put a freshly placed bid into a list that may not contain it yet. The ranked
 * endpoint can lag the write that just succeeded, so a refresh that comes back
 * without the new bid must not make it vanish from the screen.
 */
export function mergePlacedBid(serverBids: readonly Bid[], placed: Bid): Bid[] {
  if (serverBids.some((b) => b.bid_id === placed.bid_id)) {
    return rankBids(serverBids);
  }
  return rankBids([...serverBids, placed]);
}
