import React from "react";
import Link from "next/link";
import { PublicInvoice } from "../invest/_lib/view-model";
import { type Invoice } from "../lib/api/endpoints";
import { getInvoiceStatusMeta, type InvoiceStatus } from "./invoice-status";
import { formatAtomicAmount } from "../business/invoices/_lib/math";

export interface StatusIconProps {
  status: InvoiceStatus;
  size?: number;
  className?: string;
}

export function StatusIcon({
  status,
  size = 14,
  className = "",
}: StatusIconProps) {
  const commonProps = {
    width: size,
    height: size,
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    strokeWidth: 2,
    strokeLinecap: "round" as const,
    strokeLinejoin: "round" as const,
    "aria-hidden": true as const,
    className,
  };

  switch (status) {
    case "Pending":
      return (
        <svg {...commonProps}>
          <path d="M12 6v6l4 2" />
          <circle cx="12" cy="12" r="9" />
        </svg>
      );
    case "Verified":
      return (
        <svg {...commonProps}>
          <path d="M5 13l4 4L19 7" />
        </svg>
      );
    case "Funded":
      return (
        <svg {...commonProps}>
          <rect x="2" y="6" width="20" height="12" rx="2" />
          <circle cx="12" cy="12" r="2" />
        </svg>
      );
    case "Paid":
      return (
        <svg {...commonProps}>
          <circle cx="12" cy="12" r="9" />
          <path d="M9 12l2 2 4-4" />
        </svg>
      );
    case "Defaulted":
      return (
        <svg {...commonProps}>
          <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
          <line x1="12" y1="9" x2="12" y2="13" />
          <line x1="12" y1="17" x2="12.01" y2="17" />
        </svg>
      );
    case "Cancelled":
      return (
        <svg {...commonProps}>
          <circle cx="12" cy="12" r="9" />
          <line x1="15" y1="9" x2="9" y2="15" />
          <line x1="9" y1="9" x2="15" y2="15" />
        </svg>
      );
  }
}

export type InvoiceCardVariant = "default" | "compact" | "expanded";

export interface InvoiceCardProps {
  invoice: PublicInvoice | Invoice;
  bestBidSlot?: React.ReactNode;
  variant?: InvoiceCardVariant;
  decimals?: number;
}

export function InvoiceCard({
  invoice,
  bestBidSlot,
  variant = "default",
  decimals = 7,
}: InvoiceCardProps) {
  const isCompact = variant === "compact";
  const statusMeta = getInvoiceStatusMeta(invoice.status);

  // Format due date
  const dueDateStr = invoice.due_date
    ? new Date(invoice.due_date * 1000).toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
      })
    : "N/A";

  const formattedAmount = formatAtomicAmount(invoice.amount, decimals);

  return (
    <div
      data-testid={`invoice-card-${invoice.id}`}
      data-variant={variant}
      className={`bg-white rounded-xl border border-gray-200 border-l-4 ${statusMeta.railClass} ${
        isCompact ? "p-3" : "p-5"
      } shadow-sm hover:shadow-md transition-shadow flex flex-col justify-between`}
    >
      <div>
        <div
          className={`flex items-center justify-between ${
            isCompact ? "mb-2" : "mb-3"
          }`}
        >
          <span className="text-xs font-mono bg-gray-100 text-gray-700 px-2 py-1 rounded">
            {invoice.id}
          </span>
          <span
            className={`inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium border ${statusMeta.badgeClass}`}
          >
            <StatusIcon status={invoice.status} size={14} />
            <span>{statusMeta.label}</span>
          </span>
        </div>

        <div className={isCompact ? "mb-2" : "mb-4"}>
          <div
            className={`${
              isCompact ? "text-lg" : "text-2xl"
            } font-bold text-gray-900`}
          >
            <span>{formattedAmount}</span>{" "}
            <span className="text-sm font-semibold text-gray-500 uppercase">
              {invoice.currency}
            </span>
          </div>
          <div className="flex items-center gap-1 text-xs text-gray-500 mt-1">
            <svg
              className="w-4 h-4 text-gray-400"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
              aria-hidden="true"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z"
              />
            </svg>
            <span>Due {dueDateStr}</span>
          </div>
        </div>

        {!isCompact && invoice.description && (
          <p className="text-sm text-gray-600 line-clamp-2 mb-3">
            {invoice.description}
          </p>
        )}

        {invoice.category && (
          <div className={isCompact ? "mb-2" : "mb-3"}>
            <span className="text-xs bg-indigo-50 text-indigo-700 px-2 py-0.5 rounded-full font-medium">
              {invoice.category}
            </span>
          </div>
        )}
      </div>

      <div
        className={`${
          isCompact ? "pt-2 mt-1 gap-2" : "pt-3 mt-2 gap-3"
        } border-t border-gray-100 flex flex-col`}
      >
        {bestBidSlot && <div className="min-h-[28px]">{bestBidSlot}</div>}
        <Link
          href={`/invest/invoices/${encodeURIComponent(invoice.id)}`}
          style={{ color: "#ffffff" }}
          className="w-full text-center py-2 px-3 bg-indigo-600 hover:bg-indigo-700 !text-white font-medium text-sm rounded-lg transition-colors"
        >
          View Details
        </Link>
      </div>
    </div>
  );
}
