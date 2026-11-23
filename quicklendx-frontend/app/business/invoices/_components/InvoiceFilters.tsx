"use client";

import React from "react";
import {
  INVOICE_STATUSES,
  type InvoiceStatus,
} from "@/app/components/invoice-status";

export type InvoiceStatusFilter = "All" | InvoiceStatus;

export interface InvoiceFiltersProps {
  selectedStatus: InvoiceStatusFilter;
  onSelectStatus: (status: InvoiceStatusFilter) => void;
}

const FILTER_TABS: readonly { value: InvoiceStatusFilter; label: string }[] = [
  { value: "All", label: "All" },
  ...INVOICE_STATUSES.map((status) => ({
    value: status,
    label: status,
  })),
];

export const InvoiceFilters: React.FC<InvoiceFiltersProps> = ({
  selectedStatus,
  onSelectStatus,
}) => {
  return (
    <div
      role="tablist"
      aria-label="Filter invoices by status"
      className="flex flex-wrap gap-2 border-b border-[#E2E8F0] pb-3"
    >
      {FILTER_TABS.map((tab) => {
        const isActive = selectedStatus === tab.value;
        return (
          <button
            key={tab.value}
            type="button"
            role="tab"
            aria-selected={isActive}
            onClick={() => onSelectStatus(tab.value)}
            className={`px-3.5 py-2 rounded-md text-sm font-medium transition-colors focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1 ${
              isActive
                ? "bg-[#2563EB] text-white"
                : "bg-white text-[#0F172A] border border-[#E2E8F0] hover:bg-[#F8FAFC]"
            }`}
          >
            {tab.label}
          </button>
        );
      })}
    </div>
  );
};

export default InvoiceFilters;
