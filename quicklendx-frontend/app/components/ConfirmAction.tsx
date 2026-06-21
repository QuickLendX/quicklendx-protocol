"use client";

import React, {
  useCallback,
  useEffect,
  useId,
  useReducer,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import toast from "react-hot-toast";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface FeeLineItem {
  label: string;
  amount: number; // XLM, 7 decimal places
  isEstimate: boolean;
  estimateTooltip?: string; // required when isEstimate === true
}

export interface ConfirmActionProps {
  isOpen: boolean;
  onClose: () => void;
  /** Resolves on server success; rejects with Error on failure. */
  onConfirm: () => Promise<void>;
  /** Optional: called immediately after hold completes (optimistic update). */
  onOptimisticUpdate?: () => void;
  /** Optional: called when server rejects (revert optimistic update). */
  onRevert?: () => void;

  // Commitment summary data
  invoiceId: string;
  bidId: string;
  lockedAmountXlm: number;
  lockUntilDate: Date;
  fees: FeeLineItem[];
  collateralisationRatio: number; // e.g. 1.08 = 108%
  stellarExplorerUrl: string;

  /** Ref to the element that triggered the dialog — focus returns here on close. */
  triggerRef?: React.RefObject<HTMLElement | null>;
}

// ---------------------------------------------------------------------------
// State machine
// ---------------------------------------------------------------------------

type DialogPhase =
  | { phase: "idle" }
  | { phase: "holding"; progressPct: number }
  | { phase: "submitting" }
  | { phase: "error"; message: string }
  | { phase: "timeout" };

type DialogAction =
  | { type: "HOLD_START" }
  | { type: "HOLD_PROGRESS"; progressPct: number }
  | { type: "HOLD_CANCEL" }
  | { type: "HOLD_COMPLETE" }
  | { type: "SUBMIT" }
  | { type: "SUCCESS" }
  | { type: "ERROR"; message: string }
  | { type: "TIMEOUT" }
  | { type: "RESET" };

function dialogReducer(state: DialogPhase, action: DialogAction): DialogPhase {
  switch (action.type) {
    case "HOLD_START":
      return { phase: "holding", progressPct: 0 };
    case "HOLD_PROGRESS":
      return { phase: "holding", progressPct: action.progressPct };
    case "HOLD_CANCEL":
      return { phase: "idle" };
    case "HOLD_COMPLETE":
    case "SUBMIT":
      return { phase: "submitting" };
    case "SUCCESS":
      return { phase: "idle" }; // dialog will be closed by parent
    case "ERROR":
      return { phase: "error", message: action.message };
    case "TIMEOUT":
      return { phase: "timeout" };
    case "RESET":
      return { phase: "idle" };
    default:
      return state;
  }
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const HOLD_DURATION_MS = 1500;
const HOLD_TICK_MS = 50;
const SERVER_TIMEOUT_MS = 10_000;
const TWO_STEP_MIN_GAP_MS = 500;
const TWO_STEP_RESET_MS = 5_000;
const PROGRESS_ARC_RADIUS = 20;
const PROGRESS_ARC_CIRCUMFERENCE = 2 * Math.PI * PROGRESS_ARC_RADIUS;
const LOW_COLLATERAL_THRESHOLD = 1.1; // 110%

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatXlm(amount: number): string {
  return amount.toLocaleString("en-US", {
    minimumFractionDigits: 7,
    maximumFractionDigits: 7,
  });
}

function formatLockDate(date: Date): string {
  const abs = date.toLocaleDateString("en-US", {
    month: "long",
    day: "numeric",
    year: "numeric",
  });
  const time = date.toLocaleTimeString("en-US", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
    timeZone: "UTC",
  });
  const now = Date.now();
  const diffDays = Math.round((date.getTime() - now) / 86_400_000);
  const relative =
    diffDays > 0 ? ` (${diffDays} day${diffDays !== 1 ? "s" : ""})` : "";
  return `${abs}, ${time} UTC${relative}`;
}

// ---------------------------------------------------------------------------
// ProgressArc sub-component
// ---------------------------------------------------------------------------

interface ProgressArcProps {
  progressPct: number;
  reducedMotion: boolean;
}

const ProgressArc: React.FC<ProgressArcProps> = ({
  progressPct,
  reducedMotion,
}) => {
  const offset =
    PROGRESS_ARC_CIRCUMFERENCE * (1 - progressPct / 100);

  if (reducedMotion) {
    return (
      <span aria-live="polite" className="text-xs font-mono text-blue-600 ml-2">
        {Math.round(progressPct)}%
      </span>
    );
  }

  return (
    <svg
      aria-hidden="true"
      width="48"
      height="48"
      viewBox="0 0 48 48"
      className="absolute inset-0 pointer-events-none"
    >
      {/* Track */}
      <circle
        cx="24"
        cy="24"
        r={PROGRESS_ARC_RADIUS}
        fill="none"
        stroke="#E2E8F0"
        strokeWidth="3"
      />
      {/* Progress */}
      <circle
        cx="24"
        cy="24"
        r={PROGRESS_ARC_RADIUS}
        fill="none"
        stroke="#60A5FA"
        strokeWidth="3"
        strokeLinecap="round"
        strokeDasharray={PROGRESS_ARC_CIRCUMFERENCE}
        strokeDashoffset={offset}
        transform="rotate(-90 24 24)"
        style={{ transition: "stroke-dashoffset 50ms linear" }}
      />
    </svg>
  );
};

// ---------------------------------------------------------------------------
// CommitmentSummary sub-component
// ---------------------------------------------------------------------------

interface CommitmentSummaryProps {
  id: string;
  invoiceId: string;
  lockedAmountXlm: number;
  lockUntilDate: Date;
  fees: FeeLineItem[];
  collateralisationRatio: number;
  stellarExplorerUrl: string;
}

const CommitmentSummary: React.FC<CommitmentSummaryProps> = ({
  id,
  invoiceId,
  lockedAmountXlm,
  lockUntilDate,
  fees,
  collateralisationRatio,
  stellarExplorerUrl,
}) => {
  const netPayout =
    lockedAmountXlm - fees.reduce((sum, f) => sum + f.amount, 0);
  const isLowCollateral = collateralisationRatio < LOW_COLLATERAL_THRESHOLD;
  const collateralPct = Math.round(collateralisationRatio * 100);

  return (
    <section
      id={id}
      aria-label="Commitment summary"
      className="overflow-y-auto max-h-[60vh] md:max-h-[70vh] text-sm text-gray-900"
    >
      {/* Key facts */}
      <dl className="space-y-1 mb-4">
        <div className="flex justify-between">
          <dt className="text-gray-500">Invoice</dt>
          <dd className="font-mono">{invoiceId}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-500">Locked</dt>
          <dd className="font-mono">{formatXlm(lockedAmountXlm)} XLM</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-500">Until</dt>
          <dd className="font-mono">{formatLockDate(lockUntilDate)}</dd>
        </div>
      </dl>

      {/* Commitment statement */}
      <p className="mb-4 text-gray-700">
        By accepting, you lock{" "}
        <span className="font-mono">{formatXlm(lockedAmountXlm)} XLM</span>{" "}
        until {formatLockDate(lockUntilDate)}.
      </p>

      {/* Fee receipt */}
      <div className="border-t border-gray-200 pt-3 mb-3 space-y-1">
        {fees.map((fee, i) => (
          <div key={i} className="flex justify-between items-center">
            <span className="text-gray-600">
              {fee.label}
              {fee.isEstimate && (
                <>
                  {" "}
                  <span className="text-gray-400">(est.)</span>{" "}
                  <button
                    type="button"
                    aria-label={`Fee estimate info: ${fee.estimateTooltip ?? ""}`}
                    title={fee.estimateTooltip ?? ""}
                    className="inline-flex items-center justify-center w-4 h-4 rounded-full bg-gray-200 text-gray-600 text-xs leading-none focus-visible:ring-2 focus-visible:ring-blue-600 focus-visible:ring-offset-1"
                  >
                    ?
                  </button>
                </>
              )}
            </span>
            <span className="font-mono">{formatXlm(fee.amount)} XLM</span>
          </div>
        ))}
      </div>
      <div className="border-t border-gray-200 pt-3 mb-4 flex justify-between font-semibold">
        <span>Expected net payout</span>
        <span className="font-mono">{formatXlm(netPayout)} XLM</span>
      </div>

      {/* Low-collateral warning */}
      {isLowCollateral && (
        <p
          role="status"
          className="flex items-center gap-1 text-yellow-500 mb-3 text-xs"
        >
          <span aria-hidden="true">⚠</span>
          Collateral ratio {collateralPct}% — below 110% threshold
        </p>
      )}

      {/* Stellar Explorer link */}
      <a
        href={stellarExplorerUrl}
        target="_blank"
        rel="noopener noreferrer"
        className="inline-flex items-center gap-1 text-blue-600 underline text-xs focus-visible:ring-2 focus-visible:ring-blue-600 focus-visible:ring-offset-1 rounded"
      >
        View on Stellar Explorer
        <span aria-hidden="true">↗</span>
      </a>
    </section>
  );
};

// ---------------------------------------------------------------------------
// HoldButton sub-component
// ---------------------------------------------------------------------------

interface HoldButtonProps {
  phase: DialogPhase["phase"];
  progressPct: number;
  reducedMotion: boolean;
  onHoldStart: () => void;
  onHoldEnd: () => void;
}

const HoldButton: React.FC<HoldButtonProps> = ({
  phase,
  progressPct,
  reducedMotion,
  onHoldStart,
  onHoldEnd,
}) => {
  const isSubmitting = phase === "submitting";
  const isHolding = phase === "holding";
  const ariaLabelPct = Math.round(progressPct / 10) * 10;

  const label = isSubmitting ? "Confirming\u2026" : "Hold to confirm";
  const ariaLabel = isHolding
    ? `Hold to confirm \u2014 ${ariaLabelPct}% complete`
    : label;

  return (
    <button
      type="button"
      aria-label={ariaLabel}
      aria-disabled={isSubmitting ? "true" : undefined}
      disabled={isSubmitting}
      onPointerDown={onHoldStart}
      onPointerUp={onHoldEnd}
      onPointerLeave={onHoldEnd}
      onKeyDown={(e) => {
        if (e.key === " " || e.key === "Enter") {
          e.preventDefault();
          onHoldStart();
        }
      }}
      onKeyUp={(e) => {
        if (e.key === " " || e.key === "Enter") {
          onHoldEnd();
        }
      }}
      style={{ touchAction: "none" }}
      className={[
        "relative flex items-center justify-center gap-2",
        "w-full md:w-auto min-h-[48px] px-6 py-3 rounded-lg",
        "text-white font-medium text-sm select-none",
        "focus-visible:ring-2 focus-visible:ring-blue-600 focus-visible:ring-offset-2",
        "transition-colors",
        isSubmitting
          ? "bg-blue-600 opacity-40 cursor-not-allowed pointer-events-none"
          : isHolding
            ? "bg-blue-400"
            : "bg-blue-600 hover:bg-blue-700 active:scale-[0.98]",
      ].join(" ")}
    >
      {isSubmitting ? (
        <svg
          aria-hidden="true"
          className="animate-spin h-4 w-4 text-white"
          fill="none"
          viewBox="0 0 24 24"
        >
          <circle
            className="opacity-25"
            cx="12"
            cy="12"
            r="10"
            stroke="currentColor"
            strokeWidth="4"
          />
          <path
            className="opacity-75"
            fill="currentColor"
            d="M4 12a8 8 0 018-8v4a4 4 0 00-4 4H4z"
          />
        </svg>
      ) : null}
      <span>{label}</span>
      {isHolding && (
        <ProgressArc
          progressPct={progressPct}
          reducedMotion={reducedMotion}
        />
      )}
    </button>
  );
};

// ---------------------------------------------------------------------------
// TwoStepButton sub-component
// ---------------------------------------------------------------------------

interface TwoStepButtonProps {
  phase: DialogPhase["phase"];
  onConfirm: () => void;
}

const TwoStepButton: React.FC<TwoStepButtonProps> = ({ phase, onConfirm }) => {
  const [pending, setPending] = useState(false);
  const firstClickTimeRef = useRef<number>(0);
  const resetTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const isSubmitting = phase === "submitting";

  const handleClick = useCallback(() => {
    if (isSubmitting) return;

    if (!pending) {
      setPending(true);
      firstClickTimeRef.current = Date.now();
      resetTimerRef.current = setTimeout(() => {
        setPending(false);
      }, TWO_STEP_RESET_MS);
    } else {
      const elapsed = Date.now() - firstClickTimeRef.current;
      if (elapsed >= TWO_STEP_MIN_GAP_MS) {
        if (resetTimerRef.current) clearTimeout(resetTimerRef.current);
        setPending(false);
        onConfirm();
      }
    }
  }, [pending, isSubmitting, onConfirm]);

  // Reset when dialog phase changes away from idle/holding
  useEffect(() => {
    if (phase !== "idle" && phase !== "holding") {
      setPending(false);
      if (resetTimerRef.current) clearTimeout(resetTimerRef.current);
    }
  }, [phase]);

  useEffect(() => {
    return () => {
      if (resetTimerRef.current) clearTimeout(resetTimerRef.current);
    };
  }, []);

  return (
    <button
      type="button"
      onClick={handleClick}
      disabled={isSubmitting}
      aria-disabled={isSubmitting ? "true" : undefined}
      className={[
        "w-full md:w-auto min-h-[48px] px-6 py-3 rounded-lg",
        "text-sm font-medium border border-gray-300",
        "focus-visible:ring-2 focus-visible:ring-blue-600 focus-visible:ring-offset-2",
        "transition-colors",
        isSubmitting
          ? "opacity-40 cursor-not-allowed bg-gray-100 text-gray-500"
          : pending
            ? "bg-blue-50 text-blue-700 border-blue-300"
            : "bg-white text-gray-900 hover:bg-gray-50",
      ].join(" ")}
    >
      {pending ? "Press again to confirm" : "Confirm acceptance"}
    </button>
  );
};

// ---------------------------------------------------------------------------
// Main ConfirmAction component
// ---------------------------------------------------------------------------

export const ConfirmAction: React.FC<ConfirmActionProps> = ({
  isOpen,
  onClose,
  onConfirm,
  onOptimisticUpdate,
  onRevert,
  invoiceId,
  bidId: _bidId,
  lockedAmountXlm,
  lockUntilDate,
  fees,
  collateralisationRatio,
  stellarExplorerUrl,
  triggerRef,
}) => {
  const [state, dispatch] = useReducer(dialogReducer, { phase: "idle" });
  const [mounted, setMounted] = useState(false);
  const [closing, setClosing] = useState(false);

  const dialogRef = useRef<HTMLDivElement>(null);
  const cancelBtnRef = useRef<HTMLButtonElement>(null);
  const holdIntervalRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const serverTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const progressRef = useRef(0);
  // Stable ref so handleHoldStart can call handleSubmit without a circular dep
  const handleSubmitRef = useRef<() => Promise<void>>(() => Promise.resolve());

  const titleId = useId();
  const summaryId = useId();

  // Detect reduced-motion preference
  const [reducedMotion, setReducedMotion] = useState(false);
  useEffect(() => {
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    setReducedMotion(mq.matches);
    const handler = (e: MediaQueryListEvent) => setReducedMotion(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  // Portal mount guard
  useEffect(() => setMounted(true), []);

  // Open / close lifecycle
  useEffect(() => {
    if (isOpen) {
      setClosing(false);
      dispatch({ type: "RESET" });
      // Focus Cancel button after paint
      requestAnimationFrame(() => cancelBtnRef.current?.focus());
    }
  }, [isOpen]);

  // Hold mechanic — defined before handleClose so it can be called from it
  const clearHoldInterval = useCallback(() => {
    if (holdIntervalRef.current) {
      clearInterval(holdIntervalRef.current);
      holdIntervalRef.current = null;
    }
    progressRef.current = 0;
  }, []);

  const handleClose = useCallback(() => {
    if (state.phase === "submitting") return;
    clearHoldInterval();
    setClosing(true);
    const delay = reducedMotion ? 0 : 200;
    setTimeout(() => {
      setClosing(false);
      onClose();
      // Return focus to trigger
      const target = triggerRef?.current;
      if (target && typeof target.focus === "function") {
        target.focus();
      }
    }, delay);
  }, [state.phase, reducedMotion, onClose, triggerRef, clearHoldInterval]);

  // Escape key handler
  useEffect(() => {
    if (!isOpen) return;
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape" && state.phase !== "submitting") {
        handleClose();
      }
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen, state.phase]);

  const handleHoldStart = useCallback(() => {
    if (state.phase !== "idle") return;
    dispatch({ type: "HOLD_START" });
    progressRef.current = 0;

    holdIntervalRef.current = setInterval(() => {
      progressRef.current += (HOLD_TICK_MS / HOLD_DURATION_MS) * 100;
      if (progressRef.current >= 100) {
        clearHoldInterval();
        dispatch({ type: "HOLD_COMPLETE" });
        handleSubmitRef.current();
      } else {
        dispatch({ type: "HOLD_PROGRESS", progressPct: progressRef.current });
      }
    }, HOLD_TICK_MS);
  }, [state.phase, clearHoldInterval]);

  const handleHoldEnd = useCallback(() => {
    if (state.phase !== "holding") return;
    clearHoldInterval();
    dispatch({ type: "HOLD_CANCEL" });
  }, [state.phase, clearHoldInterval]);

  const handleSubmit = useCallback(async () => {
    dispatch({ type: "SUBMIT" });
    onOptimisticUpdate?.();

    serverTimeoutRef.current = setTimeout(() => {
      dispatch({ type: "TIMEOUT" });
    }, SERVER_TIMEOUT_MS);

    try {
      await onConfirm();
      if (serverTimeoutRef.current) clearTimeout(serverTimeoutRef.current);
      dispatch({ type: "SUCCESS" });
      onClose();
      toast.success("Bid accepted. Funds are in escrow.", { duration: 5000 });
    } catch (err) {
      if (serverTimeoutRef.current) clearTimeout(serverTimeoutRef.current);
      const message =
        err instanceof Error
          ? err.message
          : "Transaction could not be submitted. Check your connection and try again.";
      dispatch({ type: "ERROR", message });
      onRevert?.();
    }
  }, [onConfirm, onClose, onOptimisticUpdate, onRevert]);

  // Keep the ref in sync so handleHoldStart can always call the latest version
  handleSubmitRef.current = handleSubmit;

  // Cleanup on unmount
  useEffect(() => {
    return () => {
      clearHoldInterval();
      if (serverTimeoutRef.current) clearTimeout(serverTimeoutRef.current);
    };
  }, [clearHoldInterval]);

  if (!mounted || !isOpen) return null;

  const isSubmitting = state.phase === "submitting";
  const progressPct =
    state.phase === "holding" ? state.progressPct : 0;

  // Determine viewport for animation class
  const isMobile =
    typeof window !== "undefined" && window.innerWidth < 768;
  const noAnim = reducedMotion || window.innerWidth < 375;

  const panelAnimClass = noAnim
    ? ""
    : isMobile
      ? closing
        ? "animate-[cad-close-mobile_300ms_ease-in_forwards]"
        : "animate-[cad-open-mobile_300ms_ease-out_forwards]"
      : closing
        ? "animate-[cad-close-desktop_200ms_ease-in_forwards]"
        : "animate-[cad-open-desktop_200ms_ease-out_forwards]";

  return createPortal(
    <>
      {/* Keyframe styles injected inline for portability */}
      <style>{`
        @keyframes cad-open-desktop {
          from { opacity: 0; transform: scale(0.95); }
          to   { opacity: 1; transform: scale(1); }
        }
        @keyframes cad-close-desktop {
          from { opacity: 1; transform: scale(1); }
          to   { opacity: 0; transform: scale(0.95); }
        }
        @keyframes cad-open-mobile {
          from { transform: translateY(100%); }
          to   { transform: translateY(0); }
        }
        @keyframes cad-close-mobile {
          from { transform: translateY(0); }
          to   { transform: translateY(100%); }
        }
        @media (prefers-reduced-motion: reduce) {
          .cad-panel { animation: none !important; transition: none !important; }
        }
      `}</style>

      {/* Backdrop */}
      <div
        aria-hidden="true"
        className="fixed inset-0 bg-black/50 z-40"
        onClick={() => {
          if (!isSubmitting) handleClose();
        }}
      />

      {/* Dialog */}
      <div
        ref={dialogRef}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby={titleId}
        aria-describedby={summaryId}
        tabIndex={-1}
        className={[
          "fixed z-50",
          // Mobile: full-screen sheet
          "inset-0 flex flex-col bg-white",
          // Desktop: centred modal
          "md:inset-auto md:top-1/2 md:left-1/2 md:-translate-x-1/2 md:-translate-y-1/2",
          "md:w-full md:max-w-[500px] md:rounded-lg md:shadow-lg",
          "cad-panel",
          panelAnimClass,
        ].join(" ")}
      >
        {/* Focus sentinel — start */}
        <span
          tabIndex={0}
          aria-hidden="true"
          className="sr-only"
          onFocus={() => {
            // Wrap to last focusable: the cancel button
            cancelBtnRef.current?.focus();
          }}
        />

        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 shrink-0">
          <h2
            id={titleId}
            className="text-2xl font-semibold text-gray-900"
          >
            Accept bid
          </h2>
          <button
            type="button"
            aria-label="Close dialog"
            onClick={handleClose}
            disabled={isSubmitting}
            className={[
              "flex items-center justify-center w-8 h-8 rounded text-gray-500",
              "hover:bg-gray-100 focus-visible:ring-2 focus-visible:ring-blue-600",
              "focus-visible:ring-offset-1 transition-colors",
              isSubmitting ? "opacity-40 cursor-not-allowed" : "",
            ].join(" ")}
          >
            ×
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-6 py-4">
          <CommitmentSummary
            id={summaryId}
            invoiceId={invoiceId}
            lockedAmountXlm={lockedAmountXlm}
            lockUntilDate={lockUntilDate}
            fees={fees}
            collateralisationRatio={collateralisationRatio}
            stellarExplorerUrl={stellarExplorerUrl}
          />

          {/* Error state */}
          {state.phase === "error" && (
            <p
              role="alert"
              className="mt-4 text-sm text-red-600 bg-red-50 border border-red-200 rounded p-3"
            >
              Error: {state.message}
            </p>
          )}

          {/* Timeout state */}
          {state.phase === "timeout" && (
            <p
              role="status"
              className="mt-4 text-sm text-gray-600 bg-gray-50 border border-gray-200 rounded p-3"
            >
              This is taking longer than expected. Your action may still be
              processing. Check back shortly.
            </p>
          )}
        </div>

        {/* Footer */}
        <div className="shrink-0 px-6 py-4 border-t border-gray-200">
          {/* Hold + two-step buttons */}
          <div className="flex flex-col md:flex-row md:justify-end gap-2 mb-2">
            <HoldButton
              phase={state.phase}
              progressPct={progressPct}
              reducedMotion={reducedMotion}
              onHoldStart={handleHoldStart}
              onHoldEnd={handleHoldEnd}
            />
            <TwoStepButton
              phase={state.phase}
              onConfirm={handleSubmit}
            />
          </div>

          {/* Cancel / Dismiss row */}
          <div className="flex flex-col md:flex-row md:justify-end gap-2">
            {state.phase === "timeout" ? (
              <button
                type="button"
                onClick={onClose}
                className="w-full md:w-auto min-h-[48px] px-6 py-3 rounded-lg text-sm font-medium bg-gray-100 text-gray-900 hover:bg-gray-200 focus-visible:ring-2 focus-visible:ring-blue-600 focus-visible:ring-offset-2 transition-colors"
              >
                Dismiss
              </button>
            ) : (
              <button
                ref={cancelBtnRef}
                type="button"
                onClick={handleClose}
                disabled={isSubmitting}
                aria-disabled={isSubmitting ? "true" : undefined}
                className={[
                  "w-full md:w-auto min-h-[48px] px-6 py-3 rounded-lg",
                  "text-sm font-medium bg-gray-100 text-gray-900",
                  "focus-visible:ring-2 focus-visible:ring-blue-600 focus-visible:ring-offset-2",
                  "transition-colors",
                  isSubmitting
                    ? "opacity-40 cursor-not-allowed"
                    : "hover:bg-gray-200",
                ].join(" ")}
              >
                Cancel
              </button>
            )}
          </div>
        </div>

        {/* Focus sentinel — end */}
        <span
          tabIndex={0}
          aria-hidden="true"
          className="sr-only"
          onFocus={() => {
            // Wrap to first focusable: the cancel button
            cancelBtnRef.current?.focus();
          }}
        />
      </div>
    </>,
    document.body
  );
};

export default ConfirmAction;
