import React from "react";
import Link from "next/link";
import { PublicInvoice } from "../invest/_lib/view-model";

export interface InvoiceCardProps {
  invoice: PublicInvoice;
  bestBidSlot?: React.ReactNode;
}

export function InvoiceCard({ invoice, bestBidSlot }: InvoiceCardProps) {
  // Format due date
  const dueDateStr = invoice.due_date
    ? new Date(invoice.due_date * 1000).toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
      })
    : "N/A";

  // Format amount with commas if number
  const formattedAmount = (() => {
    try {
      const bi = BigInt(invoice.amount);
      return bi.toLocaleString();
    } catch {
      return invoice.amount;
    }
  })();

  return (
    <div
      data-testid={`invoice-card-${invoice.id}`}
      className="bg-white rounded-xl border border-gray-200 p-5 shadow-sm hover:shadow-md transition-shadow flex flex-col justify-between"
    >
      <div>
        <div className="flex items-center justify-between mb-3">
          <span className="text-xs font-mono bg-gray-100 text-gray-700 px-2 py-1 rounded">
            {invoice.id}
          </span>
          <span className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium bg-emerald-100 text-emerald-800">
            <svg
              className="w-3.5 h-3.5 text-emerald-600"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
              aria-hidden="true"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M5 13l4 4L19 7"
              />
            </svg>
            {invoice.status}
          </span>
        </div>

        <div className="mb-4">
          <div className="text-2xl font-bold text-gray-900">
            {formattedAmount}{" "}
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

        {invoice.description && (
          <p className="text-sm text-gray-600 line-clamp-2 mb-3">
            {invoice.description}
          </p>
        )}

        {invoice.category && (
          <div className="mb-3">
            <span className="text-xs bg-indigo-50 text-indigo-700 px-2 py-0.5 rounded-full font-medium">
              {invoice.category}
            </span>
          </div>
        )}
      </div>

      <div className="pt-3 border-t border-gray-100 mt-2 flex flex-col gap-3">
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
