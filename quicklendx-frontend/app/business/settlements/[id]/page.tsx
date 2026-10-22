"use client";

import React, { useState, useEffect, useCallback, useRef } from "react";
import Link from "next/link";
import { useParams, notFound } from "next/navigation";
import { isHexId } from "../../../lib/ids";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

interface FeeLineItem {
  label: string;
  amount: number;
}

interface SettlementReceipt {
  id: string;
  settlementDate: string;
  status: "SETTLED" | "PENDING" | "DELAYED";
  invoice: {
    id: string;
    debtor: string;
    debtorVerified: boolean;
    amount: number;
    issueDate: string;
    dueDate: string;
    description: string;
  };
  funding: {
    fundedDate: string;
    fullFundingAt: string;
    investorCount: number;
    escrowRelease: string;
    paymentReceived: boolean;
  };
  calculation: {
    grossAmount: number;
    fees: FeeLineItem[];
    netPayout: number;
  };
  transfer: {
    accountLast4: string;
    transferredAt: string;
    stellarTxHash: string;
  };
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function formatDisplayDate(iso: string): string {
  try {
    return new Date(iso).toLocaleDateString("en-US", {
      year: "numeric",
      month: "long",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      timeZone: "UTC",
      timeZoneName: "short",
    });
  } catch {
    return iso;
  }
}

function truncateMiddle(str: string, prefixLen = 6, suffixLen = 4): string {
  if (str.length <= prefixLen + suffixLen + 2) return str;
  return `${str.slice(0, prefixLen)}…${str.slice(-suffixLen)}`;
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

const StatusBadge: React.FC<{ status: SettlementStatus }> = ({ status }) => {
  const map: Record<SettlementStatus, { label: string; cls: string }> = {
    Paid: {
      label: "✓ Settled",
      cls: "bg-emerald-50 text-emerald-700 border-emerald-200",
    },
    Pending: {
      label: "⏳ Pending",
      cls: "bg-amber-50 text-amber-700 border-amber-200",
    },
    Defaulted: {
      label: "⚠ Defaulted",
      cls: "bg-red-50 text-red-700 border-red-200",
    },
  };

  const { label, cls } = map[status] || {
    label: status,
    cls: "bg-gray-50 text-gray-700 border-gray-200",
  };

  return (
    <span
      className={`inline-flex items-center px-3 py-1 rounded-full text-sm font-medium border ${cls}`}
    >
      {label}
    </span>
  );
};

/** Copy button with live text feedback */
const CopyButton: React.FC<{ value: string; label?: string }> = ({
  value,
  label = "Copy full value",
}) => {
  const [copied, setCopied] = useState(false);

  const handleCopy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch {
      // Ignore copy error
    }
  };

  return (
    <button
      type="button"
      onClick={handleCopy}
      aria-label={label}
      title={label}
      className="inline-flex items-center text-neutral-500 hover:text-neutral-900 focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded p-0.5"
    >
      {copied ? (
        <svg
          width="14"
          height="14"
          viewBox="0 0 16 16"
          fill="none"
          aria-hidden="true"
        >
          <path
            d="M3 8l4 4 6-6"
            stroke="#10B981"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      ) : (
        <svg
          width="14"
          height="14"
          viewBox="0 0 16 16"
          fill="none"
          aria-hidden="true"
        >
          <rect
            x="5"
            y="5"
            width="8"
            height="8"
            rx="1.5"
            stroke="currentColor"
            strokeWidth="1.5"
          />
          <path
            d="M3 11V3.5A1.5 1.5 0 014.5 2H11"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
          />
        </svg>
      )}
    </button>
  );
};

/** Reusable info block card */
const Block: React.FC<{ title: string; children: React.ReactNode }> = ({
  title,
  children,
}) => (
  <div className="bg-white border border-[#E2E8F0] rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] p-6">
    <h2 className="text-xs font-semibold text-[#64748B] uppercase tracking-wider mb-4">
      {title}
    </h2>
    {children}
  </div>
);

/** Label/value row used inside blocks */
const Row: React.FC<{ label: string; children: React.ReactNode }> = ({
  label,
  children,
}) => (
  <div className="flex justify-between items-baseline py-1.5 border-b border-[#E2E8F0] last:border-0">
    <dt className="text-xs font-medium text-[#64748B] uppercase tracking-wide">
      {label}
    </dt>
    <dd className="text-sm text-[#0F172A] text-right">{children}</dd>
  </div>
);

/** Download button with loading/success states */
const ExportButton: React.FC<{
  label: string;
  icon: React.ReactNode;
  ariaLabel: string;
  variant?: "primary" | "secondary";
  onClick: () => Promise<void>;
}> = ({ label, icon, ariaLabel, variant = "secondary", onClick }) => {
  const [state, setState] = useState<"idle" | "loading" | "success" | "error">(
    "idle"
  );

  const handleClick = async () => {
    setState("loading");
    try {
      await onClick();
      setState("success");
      setTimeout(() => setState("idle"), 2000);
    } catch {
      setState("error");
      setTimeout(() => setState("idle"), 3000);
    }
  };

  const base =
    "inline-flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1 transition-all";
  const styles = {
    primary: "bg-[#2563EB] text-white hover:brightness-110 disabled:opacity-40",
    secondary:
      "border border-[#E2E8F0] text-[#0F172A] bg-white hover:bg-[#F8FAFC] disabled:opacity-40",
  };

  const currentIcon =
    state === "loading" ? (
      <svg
        className="animate-spin h-4 w-4"
        viewBox="0 0 24 24"
        fill="none"
        role="status"
        aria-label={label}
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
    ) : state === "success" ? (
      <svg
        width="16"
        height="16"
        viewBox="0 0 16 16"
        fill="none"
        aria-hidden="true"
      >
        <path
          d="M3 8l4 4 6-6"
          stroke="#10B981"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    ) : state === "error" ? (
      <svg
        width="16"
        height="16"
        viewBox="0 0 16 16"
        fill="none"
        aria-hidden="true"
      >
        <path
          d="M4 4l8 8M12 4l-8 8"
          stroke="#EF4444"
          strokeWidth="2"
          strokeLinecap="round"
        />
      </svg>
    ) : (
      icon
    );

  const currentLabel =
    state === "loading"
      ? `${label.includes("PDF") ? "Generating" : "Preparing"}…`
      : state === "success"
        ? "✓ Downloaded"
        : state === "error"
          ? "Failed — Retry"
          : label;

  return (
    <button
      type="button"
      onClick={handleClick}
      disabled={state === "loading"}
      aria-label={ariaLabel}
      className={`${base} ${styles[variant]} disabled:cursor-not-allowed`}
    >
      {currentIcon}
      {currentLabel}
    </button>
  );
};

// ---------------------------------------------------------------------------
// Main Page Component
// ---------------------------------------------------------------------------

export default function SettlementReceiptPage() {
  const params = useParams();
  const requestedId =
    typeof params?.id === "string" ? (params.id as string) : undefined;

  // Reject malformed identifiers before any API call is made: settlement ids
  // are on-chain hex strings (`0x` + hex digits, see
  // `backend/src/validators/shared.ts` `hexStringSchema`).
  if (requestedId !== undefined && !isHexId(requestedId)) {
    notFound();
  }

  const receiptId = requestedId ?? PLACEHOLDER.id;

  const { address, connect } = useWallet();

  const [viewModel, setViewModel] = useState<ReceiptViewModel | null>(null);
  const [loading, setLoading] = useState<boolean>(false);
  const [notFound, setNotFound] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  // aria-live region for export feedback
  const [liveMsg, setLiveMsg] = useState("");

  const abortControllerRef = useRef<AbortController | null>(null);

  const fetchData = useCallback(async () => {
    if (!settlementId || !address) return;

    if (abortControllerRef.current) {
      abortControllerRef.current.abort();
    }
    const controller = new AbortController();
    abortControllerRef.current = controller;

    setLoading(true);
    setNotFound(false);
    setError(null);

    try {
      const settlement = await getSettlement(settlementId, controller.signal);
      const invoice = await getInvoice(
        settlement.invoice_id,
        controller.signal
      );

      const vm = toReceiptViewModel(settlement, invoice);
      setViewModel(vm);
    } catch (err: unknown) {
      if (controller.signal.aborted) {
        return;
      }
      const errObj = err as {
        name?: string;
        message?: string;
        code?: string;
        context?: { additionalData?: { status?: number } };
        response?: { status?: number };
      };
      const is404 =
        errObj?.name === "BusinessLogicError" ||
        errObj?.code === "NOT_FOUND" ||
        errObj?.context?.additionalData?.status === 404 ||
        errObj?.response?.status === 404 ||
        errObj?.message?.toLowerCase().includes("not found");

      if (is404) {
        setNotFound(true);
      } else {
        setError(errObj?.message || "Failed to load settlement receipt");
      }
    } finally {
      if (!controller.signal.aborted) {
        setLoading(false);
      }
    }
  }, [settlementId, address]);

  useEffect(() => {
    if (address && settlementId) {
      fetchData();
    } else {
      setViewModel(null);
      setLoading(false);
      setNotFound(false);
      setError(null);
    }

    return () => {
      if (abortControllerRef.current) {
        abortControllerRef.current.abort();
      }
    };
  }, [address, settlementId, fetchData]);

  const handleDownloadJSON = async () => {
    if (!viewModel) return;
    const exportData = toJsonExport(viewModel);
    const blob = new Blob([JSON.stringify(exportData, null, 2)], {
      type: "application/json",
    });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${viewModel.id}.json`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    setLiveMsg("JSON downloaded.");
    setTimeout(() => setLiveMsg(""), 3000);
  };

  const handleDownloadPDF = async () => {
    throw new Error("PDF generation not yet implemented.");
  };

  const handlePrint = async () => {
    window.print();
  };

  const handleEmail = async () => {
    throw new Error("Email not yet implemented.");
  };

  // 1. No wallet state: Connect prompt
  if (!address) {
    return (
      <div className="min-h-screen bg-gray-50 py-12 px-4 sm:px-6 lg:px-8 print:bg-white print:py-4">
        <div className="max-w-3xl mx-auto space-y-6">
          <Link
            href="/"
            className="print:hidden inline-flex items-center gap-1 text-sm text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 16 16"
              fill="none"
              aria-hidden="true"
            >
              <path
                d="M10 12L6 8l4-4"
                stroke="currentColor"
                strokeWidth="1.75"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
            Back to Home
          </Link>

          <div className="bg-white border border-[#E2E8F0] rounded-lg p-8 text-center shadow-sm space-y-4">
            <h2 className="text-xl font-bold text-[#0F172A]">
              Connect Wallet to View Receipt
            </h2>
            <p className="text-sm text-[#64748B] max-w-md mx-auto">
              Please connect your Stellar wallet to view this settlement
              receipt.
            </p>
            <div>
              <button
                type="button"
                onClick={() => connect()}
                className="inline-flex items-center px-4 py-2 bg-[#2563EB] text-white rounded-md text-sm font-medium hover:brightness-110 focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1"
              >
                Connect Wallet
              </button>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // 2. Loading state: Skeleton
  if (loading) {
    return (
      <div className="min-h-screen bg-gray-50 py-12 px-4 sm:px-6 lg:px-8 print:bg-white print:py-4">
        <div className="max-w-3xl mx-auto space-y-6">
          <Link
            href="/"
            className="print:hidden inline-flex items-center gap-1 text-sm text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 16 16"
              fill="none"
              aria-hidden="true"
            >
              <path
                d="M10 12L6 8l4-4"
                stroke="currentColor"
                strokeWidth="1.75"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
            Back to Home
          </Link>
          <ReceiptSkeleton />
        </div>
      </div>
    );
  }

  // 3. 404 state: Receipt not found
  if (notFound) {
    return (
      <div className="min-h-screen bg-gray-50 py-12 px-4 sm:px-6 lg:px-8 print:bg-white print:py-4">
        <div className="max-w-3xl mx-auto space-y-6">
          <Link
            href="/"
            className="print:hidden inline-flex items-center gap-1 text-sm text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 16 16"
              fill="none"
              aria-hidden="true"
            >
              <path
                d="M10 12L6 8l4-4"
                stroke="currentColor"
                strokeWidth="1.75"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
            Back to Home
          </Link>

          <div className="bg-white border border-[#E2E8F0] rounded-lg p-8 text-center shadow-sm space-y-4">
            <h2 className="text-xl font-bold text-[#0F172A]">
              Receipt not found
            </h2>
            <p className="text-sm text-[#64748B]">
              The requested settlement receipt could not be found.
            </p>
            <div>
              <Link
                href="/business/settlements"
                className="inline-flex items-center px-4 py-2 bg-[#2563EB] text-white rounded-md text-sm font-medium hover:brightness-110 focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1"
              >
                Back to Settlements
              </Link>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // 4. Other error state: Role="alert" with Retry
  if (error) {
    return (
      <div className="min-h-screen bg-gray-50 py-12 px-4 sm:px-6 lg:px-8 print:bg-white print:py-4">
        <div className="max-w-3xl mx-auto space-y-6">
          <Link
            href="/"
            className="print:hidden inline-flex items-center gap-1 text-sm text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 16 16"
              fill="none"
              aria-hidden="true"
            >
              <path
                d="M10 12L6 8l4-4"
                stroke="currentColor"
                strokeWidth="1.75"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
            Back to Home
          </Link>

          <div
            role="alert"
            className="bg-red-50 border border-red-200 rounded-lg p-6 space-y-4 text-red-700"
          >
            <h2 className="text-lg font-bold">Error Loading Receipt</h2>
            <p className="text-sm">{error}</p>
            <button
              type="button"
              onClick={() => fetchData()}
              className="inline-flex items-center px-4 py-2 bg-red-600 text-white rounded-md text-sm font-medium hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-red-500 focus:ring-offset-1"
            >
              Retry
            </button>
          </div>
        </div>
      </div>
    );
  }

  // 5. Access Gate: Client-side access gate
  // NOTE: This is a client-side gate since the read API is public.
  // The receipt is only shown if the connected wallet equals invoice.business,
  // settlement.payer, or settlement.recipient.
  const isAuthorized =
    viewModel &&
    (address === viewModel.invoice.business ||
      address === viewModel.payer ||
      address === viewModel.recipient);

  if (viewModel && !isAuthorized) {
    return (
      <div className="min-h-screen bg-gray-50 py-12 px-4 sm:px-6 lg:px-8 print:bg-white print:py-4">
        <div className="max-w-3xl mx-auto space-y-6">
          <Link
            href="/"
            className="print:hidden inline-flex items-center gap-1 text-sm text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 16 16"
              fill="none"
              aria-hidden="true"
            >
              <path
                d="M10 12L6 8l4-4"
                stroke="currentColor"
                strokeWidth="1.75"
                strokeLinecap="round"
                strokeLinejoin="round"
              />
            </svg>
            Back to Home
          </Link>

          <div className="bg-white border border-[#E2E8F0] rounded-lg p-8 text-center shadow-sm space-y-4">
            <h2 className="text-xl font-bold text-[#0F172A]">
              You don&apos;t have access to this receipt.
            </h2>
            <p className="text-sm text-[#64748B]">
              Only the participating parties (business, payer, or recipient) can
              view this settlement receipt.
            </p>
          </div>
        </div>
      </div>
    );
  }

  if (!viewModel) {
    return null;
  }

  return (
    <div className="min-h-screen bg-gray-50 py-12 px-4 sm:px-6 lg:px-8 print:bg-white print:py-4">
      <div className="max-w-3xl mx-auto space-y-6">
        {/* Back link — hidden on print */}
        <Link
          href="/business/settlements"
          className="print:hidden inline-flex items-center gap-1 text-sm text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
        >
          <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            aria-hidden="true"
          >
            <path
              d="M10 12L6 8l4-4"
              stroke="currentColor"
              strokeWidth="1.75"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          Back to settlements
        </Link>

        {/* Header */}
        <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-2">
          <div>
            <h1 className="text-3xl font-bold text-[#0F172A]">
              Settlement Receipt
            </h1>
            <div className="mt-1 flex items-center gap-1.5 font-mono text-sm text-[#64748B]">
              <span>#{truncateMiddle(viewModel.id)}</span>
              <CopyButton
                value={viewModel.id}
                label="Copy full receipt ID to clipboard"
              />
            </div>
            <p className="text-sm text-[#64748B]">
              {formatDisplayDate(viewModel.settlementDate)}
            </p>
          </div>
          <StatusBadge status={viewModel.status} />
        </div>

        {/* Block 1: Invoice Details */}
        <Block title="Invoice Details">
          <dl className="space-y-0">
            <Row label="Invoice ID">
              <Link
                href={`/business/invoices/${viewModel.invoice.id}`}
                className="text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
              >
                {viewModel.invoice.id}
              </Link>
            </Row>
            <Row label="Invoice Amount">
              <span className="font-mono">
                {viewModel.invoice.amount} {viewModel.invoice.currency}
              </span>
            </Row>
            <Row label="Due Date">
              {formatDisplayDate(viewModel.invoice.dueDate)}
            </Row>
            {viewModel.invoice.category && (
              <Row label="Category">{viewModel.invoice.category}</Row>
            )}
            <Row label="Description">{viewModel.invoice.description}</Row>
          </dl>
        </Block>

        {/* Block 2: Settlement Details */}
        <Block title="Settlement Details">
          <dl className="space-y-0">
            <Row label="Settled Amount">
              <span className="font-mono font-bold text-[#0F172A]">
                {viewModel.settledAmount}
              </span>
            </Row>
            <Row label="Payer">
              <span className="inline-flex items-center gap-1.5 font-mono">
                <a
                  href={`https://stellar.expert/explorer/testnet/account/${viewModel.payer}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
                >
                  {truncateAddress(viewModel.payer)}
                </a>
                <CopyButton
                  value={viewModel.payer}
                  label="Copy payer address"
                />
              </span>
            </Row>
            <Row label="Recipient">
              <span className="inline-flex items-center gap-1.5 font-mono">
                <a
                  href={`https://stellar.expert/explorer/testnet/account/${viewModel.recipient}`}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
                >
                  {truncateAddress(viewModel.recipient)}
                </a>
                <CopyButton
                  value={viewModel.recipient}
                  label="Copy recipient address"
                />
              </span>
            </Row>
          </dl>
        </Block>

        {/* Export Actions — hidden on print */}
        <div className="print:hidden flex flex-wrap gap-3">
          <ExportButton
            label="Download PDF"
            ariaLabel={`Download settlement receipt ${viewModel.id} as PDF`}
            variant="primary"
            onClick={handleDownloadPDF}
            icon={
              <svg
                width="16"
                height="16"
                viewBox="0 0 16 16"
                fill="none"
                aria-hidden="true"
              >
                <path
                  d="M8 2v8M5 7l3 3 3-3"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
                <path
                  d="M2 12h12"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
            }
          />
          <ExportButton
            label="Download JSON"
            ariaLabel={`Download settlement receipt ${viewModel.id} as JSON`}
            onClick={handleDownloadJSON}
            icon={
              <svg
                width="16"
                height="16"
                viewBox="0 0 16 16"
                fill="none"
                aria-hidden="true"
              >
                <path
                  d="M8 2v8M5 7l3 3 3-3"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                />
                <path
                  d="M2 12h12"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
            }
          />
          <ExportButton
            label="Print"
            ariaLabel={`Print settlement receipt ${viewModel.id}`}
            onClick={handlePrint}
            icon={
              <svg
                width="16"
                height="16"
                viewBox="0 0 16 16"
                fill="none"
                aria-hidden="true"
              >
                <rect
                  x="3"
                  y="6"
                  width="10"
                  height="7"
                  rx="1"
                  stroke="currentColor"
                  strokeWidth="1.5"
                />
                <path
                  d="M5 6V3h6v3"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
                <path
                  d="M5 10h6"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
            }
          />
          <ExportButton
            label="Email Receipt"
            ariaLabel={`Email settlement receipt ${viewModel.id}`}
            onClick={handleEmail}
            icon={
              <svg
                width="16"
                height="16"
                viewBox="0 0 16 16"
                fill="none"
                aria-hidden="true"
              >
                <rect
                  x="2"
                  y="4"
                  width="12"
                  height="9"
                  rx="1"
                  stroke="currentColor"
                  strokeWidth="1.5"
                />
                <path
                  d="M2 5l6 5 6-5"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
            }
          />
          <Link
            href={`/business/settlements/${settlementId}/timeline`}
            className="inline-flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium border border-[#E2E8F0] text-[#0F172A] bg-white hover:bg-[#F8FAFC] focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1"
          >
            <svg
              width="16"
              height="16"
              viewBox="0 0 16 16"
              fill="none"
              aria-hidden="true"
            >
              <circle
                cx="8"
                cy="8"
                r="6"
                stroke="currentColor"
                strokeWidth="1.5"
              />
              <path
                d="M8 5v3.5l2 2"
                stroke="currentColor"
                strokeWidth="1.5"
                strokeLinecap="round"
              />
            </svg>
            View Timeline
          </Link>
        </div>

        {/* aria-live region for export feedback */}
        <span aria-live="polite" className="sr-only">
          {liveMsg}
        </span>

        {/* Print-only footer */}
        <div className="hidden print:block mt-8 pt-4 border-t border-[#E2E8F0] text-xs text-[#64748B]">
          <p>Confidential — for authorised account holder only.</p>
          <p>
            Printed on{" "}
            {new Date().toLocaleDateString("en-US", {
              year: "numeric",
              month: "long",
              day: "numeric",
            })}
          </p>
        </div>
      </div>
    </div>
  );
}
