"use client";

import React, { useState } from "react";
import Link from "next/link";
import { useParams } from "next/navigation";

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

function formatCurrency(amount: number): string {
  return new Intl.NumberFormat("en-US", {
    style: "currency",
    currency: "USD",
  }).format(amount);
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-US", {
    year: "numeric",
    month: "long",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

// Placeholder data — replace with API call to GET /api/settlements/:id
const PLACEHOLDER: SettlementReceipt = {
  id: "REC-20260428-001",
  settlementDate: "2026-04-28T15:45:00Z",
  status: "SETTLED",
  invoice: {
    id: "INV-8201",
    debtor: "Acme Corp",
    debtorVerified: true,
    amount: 5000,
    issueDate: "2026-04-15",
    dueDate: "2026-05-15",
    description: "Professional Services, April 2026",
  },
  funding: {
    fundedDate: "2026-04-23",
    fullFundingAt: "2026-04-28T14:15:00Z",
    investorCount: 3,
    escrowRelease: "Automatic, April 28, 2026",
    paymentReceived: true,
  },
  calculation: {
    grossAmount: 5000,
    fees: [{ label: "Service Fee (3%)", amount: -150 }],
    netPayout: 4850,
  },
  transfer: {
    accountLast4: "5678",
    transferredAt: "2026-04-28T15:45:00Z",
    stellarTxHash: "abc123def456",
  },
};

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

const StatusBadge: React.FC<{ status: SettlementReceipt["status"] }> = ({ status }) => {
  const map = {
    SETTLED: { label: "✓ Settled & Transferred", cls: "bg-emerald-50 text-emerald-700 border-emerald-200" },
    PENDING: { label: "⏳ Pending", cls: "bg-amber-50 text-amber-700 border-amber-200" },
    DELAYED: { label: "⚠ Delayed", cls: "bg-orange-50 text-orange-700 border-orange-200" },
  };
  const { label, cls } = map[status];
  return (
    <span className={`inline-flex items-center px-3 py-1 rounded-full text-sm font-medium border ${cls}`}>
      {label}
    </span>
  );
};

/** Reusable info block card */
const Block: React.FC<{ title: string; children: React.ReactNode }> = ({ title, children }) => (
  <div className="bg-white border border-[#E2E8F0] rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] p-6">
    <h2 className="text-xs font-semibold text-[#64748B] uppercase tracking-wider mb-4">{title}</h2>
    {children}
  </div>
);

/** Label/value row used inside blocks */
const Row: React.FC<{ label: string; children: React.ReactNode }> = ({ label, children }) => (
  <div className="flex justify-between items-baseline py-1.5 border-b border-[#E2E8F0] last:border-0">
    <dt className="text-xs font-medium text-[#64748B] uppercase tracking-wide">{label}</dt>
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
  const [state, setState] = useState<"idle" | "loading" | "success" | "error">("idle");

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
    secondary: "border border-[#E2E8F0] text-[#0F172A] bg-white hover:bg-[#F8FAFC] disabled:opacity-40",
  };

  const currentIcon =
    state === "loading" ? (
      <svg className="animate-spin h-4 w-4" viewBox="0 0 24 24" fill="none" role="status" aria-label={label}>
        <circle className="opacity-25" cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="4" />
        <path className="opacity-75" fill="currentColor" d="M4 12a8 8 0 018-8v4a4 4 0 00-4 4H4z" />
      </svg>
    ) : state === "success" ? (
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
        <path d="M3 8l4 4 6-6" stroke="#10B981" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    ) : state === "error" ? (
      <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
        <path d="M4 4l8 8M12 4l-8 8" stroke="#EF4444" strokeWidth="2" strokeLinecap="round" />
      </svg>
    ) : (
      icon
    );

  const currentLabel =
    state === "loading" ? `${label.includes("PDF") ? "Generating" : "Preparing"}…`
    : state === "success" ? "✓ Downloaded"
    : state === "error" ? "Failed — Retry"
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
// Page
// ---------------------------------------------------------------------------

export default function SettlementReceiptPage() {
  const params = useParams();
  const receiptId = (params?.id as string) ?? PLACEHOLDER.id;

  // TODO: replace with authenticated API call: GET /api/settlements/:id
  const receipt = PLACEHOLDER;

  // aria-live region for export feedback
  const [liveMsg, setLiveMsg] = useState("");

  const handleDownloadJSON = async () => {
    // TODO: replace with authenticated fetch
    const blob = new Blob([JSON.stringify({ receipt }, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `${receiptId}.json`;
    a.click();
    URL.revokeObjectURL(url);
    setLiveMsg("JSON downloaded.");
    setTimeout(() => setLiveMsg(""), 3000);
  };

  const handleDownloadPDF = async () => {
    // TODO: call authenticated server-side PDF endpoint
    // e.g. GET /api/settlements/:id/pdf?token=<short-lived-token>
    throw new Error("PDF generation not yet implemented.");
  };

  const handlePrint = async () => {
    window.print();
  };

  const handleEmail = async () => {
    // TODO: POST /api/settlements/:id/email
    throw new Error("Email not yet implemented.");
  };

  const explorerUrl = `https://stellar.expert/explorer/testnet/tx/${receipt.transfer.stellarTxHash}`;

  return (
    <div className="min-h-screen bg-gray-50 py-12 px-4 sm:px-6 lg:px-8 print:bg-white print:py-4">
      <div className="max-w-3xl mx-auto space-y-6">

        {/* Back link — hidden on print */}
        <Link
          href="/business/settlements"
          className="print:hidden inline-flex items-center gap-1 text-sm text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
        >
          <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
            <path d="M10 12L6 8l4-4" stroke="currentColor" strokeWidth="1.75" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
          Back to Settlements
        </Link>

        {/* Header */}
        <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-2">
          <div>
            <h1 className="text-3xl font-bold text-[#0F172A]">Settlement Receipt</h1>
            <p className="mt-1 font-mono text-sm text-[#64748B]">#{receipt.id}</p>
            <p className="text-sm text-[#64748B]">{formatDate(receipt.settlementDate)}</p>
          </div>
          <StatusBadge status={receipt.status} />
        </div>

        {/* Block 1: Invoice Details */}
        <Block title="Invoice Details">
          <dl className="space-y-0">
            <Row label="Invoice ID">
              <a
                href={`/business/invoices/${receipt.invoice.id}`}
                className="text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
              >
                {receipt.invoice.id}
              </a>
            </Row>
            <Row label="Debtor">
              <span className="flex items-center gap-1.5">
                {receipt.invoice.debtor}
                {receipt.invoice.debtorVerified && (
                  <span className="text-[#10B981] text-xs font-medium" aria-label="Verified">✓ Verified</span>
                )}
              </span>
            </Row>
            <Row label="Invoice Amount">
              <span className="font-mono">{formatCurrency(receipt.invoice.amount)}</span>
            </Row>
            <Row label="Issue Date">{receipt.invoice.issueDate}</Row>
            <Row label="Due Date">{receipt.invoice.dueDate}</Row>
            <Row label="Description">{receipt.invoice.description}</Row>
          </dl>
        </Block>

        {/* Block 2: Funding Details */}
        <Block title="Funding Details">
          <dl className="space-y-0">
            <Row label="Funded Date">{receipt.funding.fundedDate}</Row>
            <Row label="Full Funding Achieved">{formatDate(receipt.funding.fullFundingAt)}</Row>
            <Row label="Funding Source">{receipt.funding.investorCount} Investors</Row>
            <Row label="Payment Status">
              <span className="flex items-center gap-1">
                Received from Debtor
                {receipt.funding.paymentReceived && (
                  <span className="text-[#10B981]" aria-hidden="true"> ✓</span>
                )}
              </span>
            </Row>
            <Row label="Escrow Release">{receipt.funding.escrowRelease}</Row>
          </dl>
        </Block>

        {/* Block 3: Settlement Calculation */}
        <Block title="Settlement Calculation">
          {/* Semantic fee breakdown table — spec §2.3 */}
          <table className="w-full" aria-label="Settlement fee breakdown">
            <caption className="sr-only">Fee breakdown for receipt {receipt.id}</caption>
            <tbody>
              <tr className="border-b border-[#E2E8F0]">
                <th scope="row" className="py-2 text-left text-sm font-normal text-[#64748B]">
                  Invoice Amount
                </th>
                <td className="py-2 text-right font-mono text-sm text-[#0F172A]">
                  {formatCurrency(receipt.calculation.grossAmount)}
                </td>
              </tr>
              {receipt.calculation.fees.map((fee) => (
                <tr key={fee.label} className="border-b border-[#E2E8F0]">
                  <th scope="row" className="py-2 text-left text-sm font-normal text-[#64748B] pl-4">
                    {fee.label}
                  </th>
                  <td className="py-2 text-right font-mono text-sm text-[#EF4444]">
                    {formatCurrency(fee.amount)}
                  </td>
                </tr>
              ))}
              <tr className="border-t-2 border-[#0F172A]">
                <th scope="row" className="py-3 text-left text-sm font-bold text-[#0F172A]">
                  Net Payout
                </th>
                <td
                  className="py-3 text-right font-mono text-sm font-bold text-[#0F172A]"
                  aria-label={`Net payout: ${formatCurrency(receipt.calculation.netPayout)}`}
                >
                  {formatCurrency(receipt.calculation.netPayout)}
                </td>
              </tr>
            </tbody>
          </table>

          {/* Transfer details */}
          <div className="mt-4 pt-4 border-t border-[#E2E8F0] space-y-1 text-sm text-[#64748B]">
            <p>
              Transferred to account{" "}
              <span
                className="font-mono text-[#0F172A]"
                aria-label={`Account ending in ${receipt.transfer.accountLast4}`}
              >
                ••••{receipt.transfer.accountLast4}
              </span>
            </p>
            <p>Transfer time: 2–4 business hours</p>
            <p>Transferred at: {formatDate(receipt.transfer.transferredAt)}</p>
            <a
              href={explorerUrl}
              target="_blank"
              rel="noopener noreferrer"
              aria-label={`View transaction ${receipt.transfer.stellarTxHash} on Stellar Expert (opens in new tab)`}
              className="inline-flex items-center gap-1 text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded font-mono text-xs mt-1"
            >
              {receipt.transfer.stellarTxHash.slice(0, 6)}…{receipt.transfer.stellarTxHash.slice(-4)}
              <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <path d="M7 3H3a1 1 0 00-1 1v9a1 1 0 001 1h9a1 1 0 001-1V9" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
                <path d="M10 2h4v4M14 2l-6 6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </a>
          </div>
        </Block>

        {/* Export Actions — hidden on print */}
        <div className="print:hidden flex flex-wrap gap-3">
          <ExportButton
            label="Download PDF"
            ariaLabel={`Download settlement receipt ${receipt.id} as PDF`}
            variant="primary"
            onClick={handleDownloadPDF}
            icon={
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <path d="M8 2v8M5 7l3 3 3-3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                <path d="M2 12h12" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
              </svg>
            }
          />
          <ExportButton
            label="Download JSON"
            ariaLabel={`Download settlement receipt ${receipt.id} as JSON`}
            onClick={handleDownloadJSON}
            icon={
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <path d="M8 2v8M5 7l3 3 3-3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
                <path d="M2 12h12" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
              </svg>
            }
          />
          <ExportButton
            label="Print"
            ariaLabel={`Print settlement receipt ${receipt.id}`}
            onClick={handlePrint}
            icon={
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <rect x="3" y="6" width="10" height="7" rx="1" stroke="currentColor" strokeWidth="1.5" />
                <path d="M5 6V3h6v3" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
                <path d="M5 10h6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
              </svg>
            }
          />
          <ExportButton
            label="Email Receipt"
            ariaLabel={`Email settlement receipt ${receipt.id}`}
            onClick={handleEmail}
            icon={
              <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                <rect x="2" y="4" width="12" height="9" rx="1" stroke="currentColor" strokeWidth="1.5" />
                <path d="M2 5l6 5 6-5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
              </svg>
            }
          />
          <Link
            href={`/business/settlements/${receiptId}/timeline`}
            className="inline-flex items-center gap-2 px-4 py-2 rounded-md text-sm font-medium border border-[#E2E8F0] text-[#0F172A] bg-white hover:bg-[#F8FAFC] focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1"
          >
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
              <circle cx="8" cy="8" r="6" stroke="currentColor" strokeWidth="1.5" />
              <path d="M8 5v3.5l2 2" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
            </svg>
            View Timeline
          </Link>
        </div>

        {/* aria-live region for export feedback */}
        <span aria-live="polite" className="sr-only">{liveMsg}</span>

        {/* Print-only footer */}
        <div className="hidden print:block mt-8 pt-4 border-t border-[#E2E8F0] text-xs text-[#64748B]">
          <p>Confidential — for authorised account holder only.</p>
          <p>Printed on {new Date().toLocaleDateString("en-US", { year: "numeric", month: "long", day: "numeric" })}</p>
        </div>
      </div>
    </div>
  );
}
