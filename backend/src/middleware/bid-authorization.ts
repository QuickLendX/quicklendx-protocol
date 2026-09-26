import { ApiKey } from "../models/api-key";

/**
 * Bid / auction-selection authorization boundary.
 *
 * Bid and auction-selection reads are tenant-scoped: a bid row carries an
 * `investor` identity, and the investor's order flow (amounts, expected
 * returns, timing) is commercially sensitive. Before this module existed the
 * bid read routes performed no authentication at all, so any anonymous client
 * could enumerate every bid on any invoice — including competing investors'
 * sizes and returns — and could additionally pass an arbitrary `?investor=`
 * value to target a specific tenant's book.
 *
 * The rules implemented here are:
 *
 *   1. Authentication is mandatory for every bid read (401 when absent).
 *   2. The caller must hold `read:bids` (403 when absent) — enforced by
 *      `requireScopes` at the route layer.
 *   3. A non-privileged caller may only read its own rows. Passing another
 *      investor's identity is refused with 403 rather than silently respected
 *      (identity spoofing), and omitting the filter is *not* a wildcard: it is
 *      implicitly bound to the caller's own identity so the endpoint can no
 *      longer be used to dump the full bid book of an invoice.
 *   4. Privileged keys (`admin:*`, `admin:keys`, `read:*`) may read across
 *      tenants, which is what those scopes exist for (support / operations).
 */

/** Scope required to read bid and auction-selection data. */
export const BID_READ_SCOPES = ["read:bids"];

/** Scope required to submit a bid. */
export const BID_WRITE_SCOPES = ["write:bids"];

/** Scopes that lift the per-tenant restriction on bid reads. */
export const PRIVILEGED_BID_READ_SCOPES = ["admin:*", "admin:keys", "read:*"];

export interface BidReadIdentityOk {
  ok: true;
  /** Authenticated principal that owns the request (`apiKey.created_by`). */
  caller: string;
  /** True when the caller may read bids across tenants. */
  privileged: boolean;
  /**
   * Investor the query must be constrained to. `undefined` only for a
   * privileged caller that did not ask for a specific investor.
   */
  investorFilter?: string;
}

export interface BidReadIdentityErr {
  ok: false;
  status: 401 | 403;
  code: "UNAUTHORIZED" | "FORBIDDEN";
  message: string;
}

export type BidReadIdentity = BidReadIdentityOk | BidReadIdentityErr;

/**
 * Whether the key is allowed to read bid rows owned by other investors.
 */
export function isPrivilegedBidReader(apiKey?: ApiKey | null): boolean {
  if (!apiKey) return false;
  const scopes = apiKey.scopes ?? [];
  return PRIVILEGED_BID_READ_SCOPES.some((scope) => scopes.includes(scope));
}

/**
 * Resolve the effective investor filter for a bid read request.
 *
 * This is deliberately a pure function so the boundary can be unit tested
 * without spinning up Express or a database.
 */
export function resolveBidReadIdentity(
  apiKey: ApiKey | undefined | null,
  requestedInvestor?: string | null
): BidReadIdentity {
  if (!apiKey || !apiKey.created_by) {
    return {
      ok: false,
      status: 401,
      code: "UNAUTHORIZED",
      message: "Authentication required to read bids",
    };
  }

  const caller = apiKey.created_by;
  const privileged = isPrivilegedBidReader(apiKey);
  const requested =
    typeof requestedInvestor === "string" && requestedInvestor.trim() !== ""
      ? requestedInvestor.trim()
      : undefined;

  if (requested && requested !== caller && !privileged) {
    return {
      ok: false,
      status: 403,
      code: "FORBIDDEN",
      message: "Cannot query bids for another investor",
    };
  }

  return {
    ok: true,
    caller,
    privileged,
    // Non-privileged callers are always scoped to themselves, even when the
    // filter is omitted. Without this an authenticated non-privileged caller
    // could still read every tenant's bid book for an invoice.
    investorFilter: privileged ? requested : caller,
  };
}
