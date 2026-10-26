import Link from "next/link";
import { invoicePath } from "../../../lib/stellar/links";
import type { Settlement } from "../_lib/data";
import { SettlementRowSkeleton } from "./SettlementRowSkeleton";

interface SettlementsTableProps {
  rows: Settlement[];
  loading: boolean;
}

function shortInvoiceId(invoiceId: string): string {
  return invoiceId.length > 12
    ? `${invoiceId.slice(0, 6)}…${invoiceId.slice(-4)}`
    : invoiceId;
}

function InvoiceLink({ invoiceId }: { invoiceId: string }) {
  const href = invoicePath(invoiceId);
  return href ? (
    <Link
      href={href}
      title={invoiceId}
      className="font-mono text-blue-800 underline decoration-blue-300 underline-offset-2 hover:text-blue-950 focus:outline-none focus:ring-2 focus:ring-blue-600"
    >
      {shortInvoiceId(invoiceId)}
    </Link>
  ) : (
    <span className="font-mono" title={invoiceId}>
      {shortInvoiceId(invoiceId)}
    </span>
  );
}

function shortDate(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleDateString("en-US", {
    month: "numeric",
    day: "numeric",
    year: "2-digit",
  });
}

function SettlementStatus({ status }: { status: Settlement["status"] }) {
  const details = {
    Pending: { icon: "◷", classes: "text-amber-800" },
    Paid: { icon: "✓", classes: "text-emerald-800" },
    Defaulted: { icon: "!", classes: "text-rose-800" },
  }[status];

  return (
    <span className={`inline-flex items-center gap-2 ${details.classes}`}>
      <span aria-hidden="true" className="font-bold">
        {details.icon}
      </span>
      <span>{status}</span>
    </span>
  );
}

export function SettlementsTable({ rows, loading }: SettlementsTableProps) {
  return (
    <div className="overflow-hidden rounded-xl border border-slate-200 bg-white">
      <table
        aria-label="Settlement receipts"
        aria-busy={loading}
        className="settlements-table w-full border-collapse text-left text-sm"
      >
        <thead className="bg-slate-50 text-xs uppercase tracking-wide text-slate-600">
          <tr>
            {["Date", "Invoice #", "Amount", "Status", "Action"].map(
              (heading) => (
                <th
                  key={heading}
                  scope="col"
                  className="px-4 py-3 font-semibold"
                >
                  {heading}
                </th>
              )
            )}
          </tr>
        </thead>
        <tbody className="divide-y divide-slate-200">
          {loading
            ? Array.from({ length: 5 }, (_, index) => (
                <SettlementRowSkeleton key={index} />
              ))
            : rows.map((settlement, index) => (
                <tr
                  key={settlement.id}
                  id={`settlement-row-${index}`}
                  data-testid="settlement-row"
                  data-row-index={index}
                  tabIndex={-1}
                  className="settlement-record text-slate-800 hover:bg-slate-50 focus:outline-none focus:ring-2 focus:ring-inset focus:ring-blue-600"
                >
                  <td data-label="Date" className="px-4 py-4">
                    <time
                      dateTime={new Date(
                        settlement.timestamp * 1000
                      ).toISOString()}
                    >
                      {shortDate(settlement.timestamp)}
                    </time>
                  </td>
                  <td data-label="Invoice #" className="px-4 py-4">
                    <InvoiceLink invoiceId={settlement.invoice_id} />
                  </td>
                  <td data-label="Amount" className="px-4 py-4">
                    <span title={settlement.amount}>{settlement.amount}</span>
                  </td>
                  <td data-label="Status" className="px-4 py-4">
                    <SettlementStatus status={settlement.status} />
                  </td>
                  <td data-label="Action" className="px-4 py-4">
                    <Link
                      href={`/business/settlements/${encodeURIComponent(settlement.id)}`}
                      className="inline-flex min-h-9 items-center rounded-md px-3 font-semibold text-blue-800 underline decoration-blue-300 underline-offset-2 hover:bg-blue-50 focus:outline-none focus:ring-2 focus:ring-blue-600"
                    >
                      View
                    </Link>
                  </td>
                </tr>
              ))}
        </tbody>
      </table>
    </div>
  );
}
