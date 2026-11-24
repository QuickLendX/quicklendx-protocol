"use client";

/**
 * PlaceBidForm: lets a connected investor place a bid on an invoice (#3176).
 *
 * The form owns everything up to and including the request: validation, the
 * investor-limit pre-check, the preview, and mapping the server's refusals to
 * something actionable. It does not own the bid list; on success it hands the new
 * bid to `onBidPlaced` and the page updates and refreshes the ranking.
 *
 * What stops a submit, and what the investor sees instead:
 *   - no connected wallet            -> inputs and button disabled, with a notice
 *   - invoice not open for bids      -> disabled, with the reason
 *   - an active bid already exists   -> disabled (the server allows one per invoice)
 *   - indexed data too stale (>120s) -> disabled, the freshness rule used elsewhere
 *   - invalid or inconsistent fields -> per-field messages
 *   - the bid exceeds a known limit  -> an alert; submit stays off
 *   - a rate-limit wait in progress  -> disabled until it ends
 *
 * The request is sent once, never retried automatically (see `placeBid`).
 */

import React, { useEffect, useMemo, useRef, useState } from "react";
import { computeImpliedReturn } from "@/app/business/invoices/_lib/math";
import { isFreshEnoughForAction } from "@/app/components/FreshnessBanner";
import { placeBid } from "@/app/lib/api/endpoints";
import { useWallet } from "@/app/lib/wallet/WalletProvider";
import { checkInvestorLimit, type InvestorLimit } from "../_lib/investor-limit";
import {
  DEFAULT_EXPIRY_SECONDS,
  parseWholeNumber,
  previewRank,
  toDatetimeLocal,
  validatePlaceBid,
  type CreateBidPayload,
  type FieldErrors,
  type PlaceBidFormValues,
} from "../_lib/place-bid";
import {
  describePlaceBidError,
  formatUsdMicro,
  type PlaceBidFailure,
} from "../_lib/place-bid-errors";
import {
  truncateAddress,
  type Bid,
  type FreshnessMetadata,
  type InvoiceStatus,
} from "../_lib/view-model";

export interface PlaceBidFormProps {
  invoiceId: string;
  /** Invoice face amount (i128 string), the ceiling for a bid. */
  invoiceAmount: string;
  currency: string;
  invoiceStatus: InvoiceStatus;
  /** The invoice's current ranked bids, for the rank preview and the duplicate check. */
  rankedBids: Bid[];
  freshness?: FreshnessMetadata | null;
  /**
   * The investor's exposure limit, when the page knows it. The form also learns
   * it from a `429 EXPOSURE_CAP_EXCEEDED` and reports it through `onLimitLearned`.
   */
  investorLimit?: InvestorLimit | null;
  onLimitLearned?: (limit: InvestorLimit) => void;
  /** Called with the server's bid after a successful POST. */
  onBidPlaced: (bid: Bid) => void;
  /**
   * Called when the outcome of the request is unknown (timeout, dropped
   * connection, 5xx). The page should refresh the ranked bids so the investor can
   * see whether the bid landed.
   */
  onAmbiguousFailure?: () => void;
  onCancel?: () => void;
  /** Injected so tests can stand in for the network. Defaults to the real call. */
  submitBid?: (payload: CreateBidPayload) => Promise<Bid>;
  /** Injected clock (ms), so the expiry rules are deterministic in tests. */
  nowMs?: () => number;
}

type Result =
  | { kind: "success" }
  | { kind: "error"; failure: PlaceBidFailure }
  | null;

const inputClass =
  "mt-1 block w-full rounded-lg border border-gray-300 px-3 py-2 text-sm text-gray-900 shadow-sm focus:border-indigo-500 focus:outline-none focus:ring-1 focus:ring-indigo-500 disabled:bg-gray-100 disabled:text-gray-500";

export function PlaceBidForm({
  invoiceId,
  invoiceAmount,
  currency,
  invoiceStatus,
  rankedBids,
  freshness,
  investorLimit,
  onLimitLearned,
  onBidPlaced,
  onAmbiguousFailure,
  onCancel,
  submitBid = placeBid,
  nowMs = Date.now,
}: PlaceBidFormProps) {
  const { state, address } = useWallet();
  const connected = state === "CONNECTED" && Boolean(address);

  const [values, setValues] = useState<PlaceBidFormValues>(() => ({
    amount: "",
    expectedReturn: "",
    expiresAt: toDatetimeLocal(
      Math.floor(nowMs() / 1000) + DEFAULT_EXPIRY_SECONDS
    ),
  }));
  const [touched, setTouched] = useState<
    Partial<Record<keyof PlaceBidFormValues, boolean>>
  >({});
  const [submitAttempted, setSubmitAttempted] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [result, setResult] = useState<Result>(null);
  const [serverFieldErrors, setServerFieldErrors] = useState<FieldErrors>({});
  const [learnedLimit, setLearnedLimit] = useState<InvestorLimit | null>(null);
  const [cooldownUntil, setCooldownUntil] = useState<number | null>(null);

  const mounted = useRef(true);
  const inFlight = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // Re-enable the button when a rate-limit wait ends.
  useEffect(() => {
    if (cooldownUntil === null) return;
    const remaining = cooldownUntil - nowMs();
    if (remaining <= 0) {
      setCooldownUntil(null);
      return;
    }
    const timer = setTimeout(() => setCooldownUntil(null), remaining);
    return () => clearTimeout(timer);
  }, [cooldownUntil, nowMs]);

  const nowSeconds = Math.floor(nowMs() / 1000);
  const validation = validatePlaceBid(values, {
    invoiceId,
    invoiceAmount,
    nowSeconds,
  });

  const limit = investorLimit ?? learnedLimit;
  const parsedBid = parseWholeNumber(values.amount);
  const limitCheck =
    parsedBid !== null && parsedBid >= BigInt(1)
      ? checkInvestorLimit(parsedBid, currency, limit)
      : ({ status: "unknown" } as const);
  const overLimit = limitCheck.status === "exceeded";

  const hasActiveBid = useMemo(
    () =>
      Boolean(address) &&
      rankedBids.some((b) => b.status === "Placed" && b.investor === address),
    [rankedBids, address]
  );
  const notOpen = invoiceStatus !== "Verified";
  const stale = !isFreshEnoughForAction(freshness);
  const coolingDown = cooldownUntil !== null && cooldownUntil > nowMs();

  const blocker: string | null = !connected
    ? "Connect your wallet to place a bid."
    : notOpen
      ? `This invoice is ${invoiceStatus.toLowerCase()} and is not open for bids.`
      : hasActiveBid
        ? "You already have an active bid on this invoice."
        : stale
          ? "Bidding is paused while the data catches up."
          : null;

  const canSubmit =
    blocker === null &&
    validation.ok &&
    !overLimit &&
    !coolingDown &&
    !submitting;

  const fieldsDisabled = blocker !== null || submitting;

  // What to show against each field: the server's complaint wins until the field
  // is edited, then local validation once the field has been touched.
  const showError = (field: keyof PlaceBidFormValues): string | undefined => {
    if (serverFieldErrors[field]) return serverFieldErrors[field];
    if (!validation.ok && (touched[field] || submitAttempted)) {
      return validation.errors[field];
    }
    return undefined;
  };

  // Preview: needs only the two amounts, not the expiry.
  const parsedExpected = parseWholeNumber(values.expectedReturn);
  const previewReady =
    parsedBid !== null &&
    parsedBid >= BigInt(1) &&
    parsedExpected !== null &&
    parsedExpected >= parsedBid;
  const implied = previewReady
    ? computeImpliedReturn(values.amount, values.expectedReturn)
    : null;
  const rank = previewReady
    ? previewRank(rankedBids, {
        bid_amount: parsedBid as bigint,
        expected_return: parsedExpected as bigint,
      })
    : null;

  const update = (field: keyof PlaceBidFormValues, value: string) => {
    setValues((current) => ({ ...current, [field]: value }));
    setServerFieldErrors((current) => {
      if (!current[field]) return current;
      const next = { ...current };
      delete next[field];
      return next;
    });
  };

  const markTouched = (field: keyof PlaceBidFormValues) =>
    setTouched((current) => ({ ...current, [field]: true }));

  async function handleSubmit(event: React.FormEvent) {
    event.preventDefault();
    setSubmitAttempted(true);
    // A second click, or Enter pressed twice, must not send a second bid.
    if (inFlight.current) return;
    if (!canSubmit || !validation.ok) return;

    inFlight.current = true;
    setSubmitting(true);
    setResult(null);
    setServerFieldErrors({});

    try {
      const bid = await submitBid(validation.payload);
      if (!mounted.current) return;
      onBidPlaced(bid);
      setResult({ kind: "success" });
      setValues({
        amount: "",
        expectedReturn: "",
        expiresAt: toDatetimeLocal(
          Math.floor(nowMs() / 1000) + DEFAULT_EXPIRY_SECONDS
        ),
      });
      setTouched({});
      setSubmitAttempted(false);
    } catch (error) {
      const failure = describePlaceBidError(error);
      if (!mounted.current) {
        if (failure.ambiguous) onAmbiguousFailure?.();
        return;
      }
      if (failure.fieldErrors) setServerFieldErrors(failure.fieldErrors);
      if (failure.learnedLimit) {
        setLearnedLimit(failure.learnedLimit);
        onLimitLearned?.(failure.learnedLimit);
      }
      if (failure.retryAfterSeconds && failure.retryAfterSeconds > 0) {
        setCooldownUntil(nowMs() + failure.retryAfterSeconds * 1000);
      }
      if (failure.ambiguous) onAmbiguousFailure?.();
      setResult({ kind: "error", failure });
    } finally {
      inFlight.current = false;
      if (mounted.current) setSubmitting(false);
    }
  }

  const errorId = (field: string) => `place-bid-${field}-error`;

  const field = (
    name: keyof PlaceBidFormValues,
    label: string,
    inputProps: React.InputHTMLAttributes<HTMLInputElement>,
    hint?: string
  ) => {
    const error = showError(name);
    return (
      <div>
        <label
          htmlFor={`place-bid-${name}`}
          className="block text-sm font-medium text-gray-700"
        >
          {label}
        </label>
        <input
          id={`place-bid-${name}`}
          name={name}
          value={values[name]}
          onChange={(e) => update(name, e.target.value)}
          onBlur={() => markTouched(name)}
          aria-invalid={error ? true : undefined}
          aria-describedby={
            [
              error ? errorId(name) : null,
              hint ? `place-bid-${name}-hint` : null,
            ]
              .filter(Boolean)
              .join(" ") || undefined
          }
          className={inputClass}
          {...inputProps}
        />
        {hint && !error && (
          <p
            id={`place-bid-${name}-hint`}
            className="mt-1 text-xs text-gray-500"
          >
            {hint}
          </p>
        )}
        {error && (
          <p
            id={errorId(name)}
            className="mt-1 text-xs font-medium text-red-600"
          >
            {error}
          </p>
        )}
      </div>
    );
  };

  return (
    <section
      aria-labelledby="place-bid-heading"
      className="bg-white rounded-xl border border-gray-200 p-6 sm:p-8 shadow-sm mb-8"
    >
      <div className="flex items-start justify-between gap-4 mb-4">
        <div>
          <h2
            id="place-bid-heading"
            className="text-xl font-bold text-gray-900"
          >
            Place a bid
          </h2>
          {connected && address && (
            <p className="text-xs text-gray-500 mt-1">
              Bidding as{" "}
              <span className="font-mono">{truncateAddress(address)}</span>
            </p>
          )}
        </div>
        {onCancel && (
          <button
            type="button"
            onClick={onCancel}
            className="text-sm font-medium text-gray-500 hover:text-gray-700"
          >
            Close
          </button>
        )}
      </div>

      {blocker && (
        <p
          role="status"
          className="mb-4 rounded-lg border border-amber-200 bg-amber-50 px-4 py-3 text-sm text-amber-800"
        >
          {blocker}
        </p>
      )}

      <form onSubmit={handleSubmit} noValidate aria-busy={submitting}>
        <fieldset disabled={fieldsDisabled} className="space-y-4">
          <legend className="sr-only">Bid details</legend>

          {field(
            "amount",
            `Bid amount (${currency})`,
            { inputMode: "numeric", autoComplete: "off", placeholder: "0" },
            `Whole number, at most the invoice amount (${formatInvoiceAmount(invoiceAmount)}).`
          )}

          {field(
            "expectedReturn",
            `Expected return (${currency})`,
            { inputMode: "numeric", autoComplete: "off", placeholder: "0" },
            "The total you expect back, at least your bid."
          )}

          {field("expiresAt", "Bid expires", { type: "datetime-local" })}
        </fieldset>

        {(implied !== null || rank !== null) && (
          <div
            role="group"
            aria-label="Bid preview"
            className="mt-5 grid grid-cols-1 sm:grid-cols-2 gap-4 rounded-lg bg-gray-50 border border-gray-100 p-4"
          >
            {implied !== null && (
              <div>
                <div className="text-xs font-medium text-gray-500 uppercase tracking-wider">
                  Implied return
                </div>
                <div
                  className="text-lg font-semibold text-gray-900"
                  data-testid="implied-return"
                >
                  {implied}
                </div>
              </div>
            )}
            {rank !== null && (
              <div>
                <div className="text-xs font-medium text-gray-500 uppercase tracking-wider">
                  Ranking
                </div>
                <div
                  className="text-lg font-semibold text-gray-900"
                  data-testid="rank-preview"
                >
                  {`Would rank #${rank.rank} of ${rank.total}`}
                </div>
              </div>
            )}
          </div>
        )}

        {limitCheck.status === "exceeded" && (
          <p
            role="alert"
            className="mt-4 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700"
          >
            This bid would take you over your exposure limit. Your limit is{" "}
            {formatUsdMicro(limitCheck.capUsd)} USD, you have{" "}
            {formatUsdMicro(limitCheck.currentExposureUsd)} USD in use, and{" "}
            {formatUsdMicro(limitCheck.remainingUsd)} USD remains. Lower the
            amount to continue.
          </p>
        )}

        {result?.kind === "error" && (
          <p
            role="alert"
            className="mt-4 rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700"
          >
            {result.failure.message}
          </p>
        )}

        {result?.kind === "success" && (
          <p
            role="status"
            className="mt-4 rounded-lg border border-emerald-200 bg-emerald-50 px-4 py-3 text-sm text-emerald-800"
          >
            Bid placed. The ranked bids have been updated.
          </p>
        )}

        {validation.ok === false && validation.formError && (
          <p role="alert" className="mt-4 text-sm text-red-600">
            {validation.formError}
          </p>
        )}

        <div className="mt-6 flex items-center gap-3">
          <button
            type="submit"
            disabled={!canSubmit}
            className="px-5 py-2.5 bg-indigo-600 hover:bg-indigo-700 disabled:bg-gray-300 disabled:cursor-not-allowed text-white font-semibold text-sm rounded-lg shadow-sm transition-colors"
          >
            {submitting ? "Submitting…" : "Submit bid"}
          </button>
        </div>
      </form>
    </section>
  );
}

function formatInvoiceAmount(value: string): string {
  try {
    return BigInt(value).toLocaleString();
  } catch {
    return value;
  }
}
