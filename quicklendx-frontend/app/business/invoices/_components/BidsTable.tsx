import React from "react";
import type { Bid } from "@/app/business/invoices/_lib/data";
import {
  computeImpliedReturn,
  formatAtomicAmount,
} from "@/app/business/invoices/_lib/math";

export interface BidsTableProps {
  bids: Bid[];
  currency?: string;
  decimals?: number;
}

function formatExpiration(unixSeconds: number): string {
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

export const BidsTable: React.FC<BidsTableProps> = ({
  bids,
  currency = "USDC",
  decimals = 7,
}) => {
  if (bids.length === 0) {
    return (
      <p className="text-sm text-[#64748B]">
        No ranked bids placed on this invoice yet.
      </p>
    );
  }

  return (
    <div
      tabIndex={0}
      role="region"
      aria-label="Ranked bids table container"
      className="overflow-x-auto focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
    >
      <table
        className="w-full text-left border-collapse"
        aria-label="Ranked bids"
      >
        <thead>
          <tr className="border-b border-[#E2E8F0] text-xs font-semibold text-[#64748B] uppercase tracking-wider">
            <th scope="col" className="py-2.5 pr-4">
              Rank
            </th>
            <th scope="col" className="py-2.5 px-4">
              Bid Amount ({currency})
            </th>
            <th scope="col" className="py-2.5 px-4">
              Expected Return ({currency})
            </th>
            <th scope="col" className="py-2.5 px-4">
              Implied Return %
            </th>
            <th scope="col" className="py-2.5 pl-4 text-right">
              Expires At
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-[#E2E8F0] text-sm text-[#0F172A]">
          {bids.map((bid, idx) => {
            const rank = idx + 1;
            const formattedBid = formatAtomicAmount(bid.bid_amount, decimals);
            const formattedReturn = formatAtomicAmount(
              bid.expected_return,
              decimals
            );
            const impliedReturn = computeImpliedReturn(
              bid.bid_amount,
              bid.expected_return
            );

            return (
              <tr key={bid.bid_id || idx}>
                <td className="py-3 pr-4 font-mono font-semibold text-[#0F172A]">
                  #{rank}
                </td>
                <td className="py-3 px-4 font-mono">{formattedBid}</td>
                <td className="py-3 px-4 font-mono">{formattedReturn}</td>
                <td className="py-3 px-4 font-mono font-medium text-[#0F172A]">
                  {impliedReturn}
                </td>
                <td className="py-3 pl-4 text-right text-[#64748B] whitespace-nowrap">
                  {formatExpiration(bid.expiration_timestamp)}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
};

export default BidsTable;
