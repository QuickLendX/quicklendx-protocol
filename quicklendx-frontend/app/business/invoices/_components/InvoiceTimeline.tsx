import React from "react";
import type {
  Dispute,
  Invoice,
  Settlement,
} from "@/app/business/invoices/_lib/data";

export interface InvoiceTimelineProps {
  invoice: Invoice;
  disputes?: Dispute[];
  settlements?: Settlement[];
}

interface TimelineEvent {
  id: string;
  label: string;
  timestamp: number;
}

function formatTimestamp(unixSeconds: number): string {
  const ms = unixSeconds < 10_000_000_000 ? unixSeconds * 1000 : unixSeconds;
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) {
    return String(unixSeconds);
  }
  return d.toLocaleDateString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    timeZone: "UTC",
    timeZoneName: "short",
  });
}

export const InvoiceTimeline: React.FC<InvoiceTimelineProps> = ({
  invoice,
  disputes = [],
  settlements = [],
}) => {
  const events: TimelineEvent[] = [
    {
      id: "created",
      label: "Invoice created",
      timestamp: invoice.created_at,
    },
  ];

  if (invoice.updated_at && invoice.updated_at !== invoice.created_at) {
    events.push({
      id: "updated",
      label: `Invoice updated (${invoice.status})`,
      timestamp: invoice.updated_at,
    });
  }

  for (const dispute of disputes) {
    events.push({
      id: `dispute-${dispute.id}`,
      label: `Dispute opened (${dispute.status})`,
      timestamp: dispute.created_at,
    });
    if (dispute.resolved_at) {
      events.push({
        id: `dispute-resolved-${dispute.id}`,
        label: "Dispute resolved",
        timestamp: dispute.resolved_at,
      });
    }
  }

  for (const settlement of settlements) {
    events.push({
      id: `settlement-${settlement.id}`,
      label: `Settlement (${settlement.status})`,
      timestamp: settlement.timestamp,
    });
  }

  events.sort((a, b) => a.timestamp - b.timestamp);

  return (
    <ol className="relative border-l border-[#E2E8F0] ml-3 space-y-4">
      {events.map((event) => (
        <li key={event.id} className="ml-4">
          <span
            aria-hidden="true"
            className="absolute -left-1.5 mt-1.5 h-3 w-3 rounded-full border border-white bg-[#2563EB]"
          />
          <p className="text-sm font-medium text-[#0F172A]">{event.label}</p>
          <time className="block text-xs text-[#64748B]">
            {formatTimestamp(event.timestamp)}
          </time>
        </li>
      ))}
    </ol>
  );
};

export default InvoiceTimeline;
