import React from "react";
import Link from "next/link";
import { getInvoiceStatusMeta, type InvoiceStatus } from "./invoice-status";
import { formatAtomicAmount } from "@/app/business/invoices/_lib/math";

export interface InvoiceCardInvoice {
  id: string;
  amount: string;
  currency?: string;
  due_date: number;
  status: InvoiceStatus;
  description?: string;
  category?: string;
  decimals?: number;
}

export interface InvoiceCardProps {
  invoice: InvoiceCardInvoice;
  variant?: "compact" | "expanded";
  decimals?: number;
  href?: string;
}

function formatDueDate(unixSeconds: number): string {
  const ms = unixSeconds < 10_000_000_000 ? unixSeconds * 1000 : unixSeconds;
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) {
    return String(unixSeconds);
  }
  return d.toLocaleDateString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    timeZone: "UTC",
  });
}

export const StatusIcon: React.FC<{
  status: InvoiceStatus;
  size?: number;
}> = ({ status, size = 16 }) => {
  switch (status) {
    case "Pending":
      return (
        <svg
          width={size}
          height={size}
          viewBox="0 0 20 20"
          fill="none"
          aria-hidden="true"
          className="shrink-0"
        >
          <path
            d="M6 3h8M6 17h8M7 3v3.5a3 3 0 001.2 2.4L10 10l1.8-1.1A3 3 0 0013 6.5V3M7 17v-3.5a3 3 0 011.2-2.4L10 10l1.8 1.1a3 3 0 011.2 2.4V17"
            stroke="currentColor"
            strokeWidth="1.75"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      );
    case "Verified":
      return (
        <svg
          width={size}
          height={size}
          viewBox="0 0 20 20"
          fill="none"
          aria-hidden="true"
          className="shrink-0"
        >
          <circle
            cx="10"
            cy="10"
            r="7"
            stroke="currentColor"
            strokeWidth="1.75"
          />
          <path
            d="M10 6.5v7M6.5 10h7"
            stroke="currentColor"
            strokeWidth="1.75"
            strokeLinecap="round"
          />
        </svg>
      );
    case "Funded":
      return (
        <svg
          width={size}
          height={size}
          viewBox="0 0 20 20"
          fill="none"
          aria-hidden="true"
          className="shrink-0"
        >
          <rect
            x="3"
            y="5"
            width="14"
            height="10"
            rx="2"
            stroke="currentColor"
            strokeWidth="1.75"
          />
          <circle
            cx="10"
            cy="10"
            r="2"
            stroke="currentColor"
            strokeWidth="1.75"
          />
        </svg>
      );
    case "Paid":
      return (
        <svg
          width={size}
          height={size}
          viewBox="0 0 20 20"
          fill="none"
          aria-hidden="true"
          className="shrink-0"
        >
          <circle
            cx="10"
            cy="10"
            r="7"
            stroke="currentColor"
            strokeWidth="1.75"
          />
          <path
            d="M7 10.2l2 2 4-4.4"
            stroke="currentColor"
            strokeWidth="1.75"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      );
    case "Defaulted":
      return (
        <svg
          width={size}
          height={size}
          viewBox="0 0 20 20"
          fill="none"
          aria-hidden="true"
          className="shrink-0"
        >
          <path
            d="M10 3.5L2.8 16h14.4L10 3.5z"
            stroke="currentColor"
            strokeWidth="1.75"
            strokeLinejoin="round"
          />
          <path
            d="M10 8.5v3.5M10 14h.01"
            stroke="currentColor"
            strokeWidth="1.75"
            strokeLinecap="round"
          />
        </svg>
      );
    case "Cancelled":
      return (
        <svg
          width={size}
          height={size}
          viewBox="0 0 20 20"
          fill="none"
          aria-hidden="true"
          className="shrink-0"
        >
          <circle
            cx="10"
            cy="10"
            r="7"
            stroke="currentColor"
            strokeWidth="1.75"
          />
          <path
            d="M7.5 7.5l5 5M12.5 7.5l-5 5"
            stroke="currentColor"
            strokeWidth="1.75"
            strokeLinecap="round"
          />
        </svg>
      );
  }
};

export const InvoiceCard: React.FC<InvoiceCardProps> = ({
  invoice,
  variant = "expanded",
  decimals,
  href,
}) => {
  const meta = getInvoiceStatusMeta(invoice.status);
  const resolvedDecimals = decimals ?? invoice.decimals ?? 7;
  const formattedAmount = formatAtomicAmount(invoice.amount, resolvedDecimals);
  const currency = invoice.currency || "USDC";
  const targetHref = href ?? `/business/invoices/${invoice.id}`;

  const isCompact = variant === "compact";
  const paddingClass = isCompact
    ? "px-3 py-2 text-[13px]"
    : "px-6 py-4 text-[15px]";
  const iconSize = isCompact ? 16 : 20;

  return (
    <Link
      href={targetHref}
      data-rail={meta.rail}
      data-variant={variant}
      className={`block bg-white border border-[#E2E8F0] border-l-4 ${meta.railClass} rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] hover:bg-[#F8FAFC] focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1 transition-colors ${paddingClass}`}
    >
      <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-2">
        <div className="min-w-0">
          <div className="font-mono font-semibold text-[#0F172A] truncate">
            {invoice.id}
          </div>
          {!isCompact && invoice.description && (
            <p className="mt-1 text-sm text-[#64748B] truncate">
              {invoice.description}
            </p>
          )}
        </div>

        <span
          className={`inline-flex items-center gap-1.5 self-start sm:self-auto px-2.5 py-1 rounded-full text-xs font-medium border ${meta.badgeClass}`}
        >
          <StatusIcon status={invoice.status} size={iconSize} />
          <span>{meta.label}</span>
        </span>
      </div>

      <div
        className={`mt-3 flex flex-wrap items-center justify-between gap-x-4 gap-y-1 border-t border-[#E2E8F0] pt-2.5 text-[#0F172A]`}
      >
        <div className="flex items-baseline gap-1.5">
          <span className="text-xs font-medium text-[#64748B] uppercase tracking-wide">
            Amount:
          </span>
          <span className="font-mono font-semibold text-[#0F172A]">
            {formattedAmount}
          </span>
          <span className="text-xs font-medium text-[#64748B]">{currency}</span>
        </div>

        <div className="flex items-baseline gap-1.5 text-xs text-[#64748B]">
          <span className="font-medium uppercase tracking-wide">Due:</span>
          <span className="text-[#0F172A] font-medium">
            {formatDueDate(invoice.due_date)}
          </span>
        </div>
      </div>
    </Link>
  );
};

export default InvoiceCard;
