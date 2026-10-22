import React from "react";
import { Bid, truncateAddress } from "../_lib/view-model";

export interface RankedBidsTableProps {
  bids: Bid[];
}

export function RankedBidsTable({ bids }: RankedBidsTableProps) {
  if (bids.length === 0) {
    return (
      <div className="text-center py-8 bg-gray-50 border border-gray-100 rounded-lg text-sm text-gray-500">
        No bids placed yet for this invoice.
      </div>
    );
  }

  const formatAmount = (val: string) => {
    try {
      return BigInt(val).toLocaleString();
    } catch {
      return val;
    }
  };

  return (
    <div className="overflow-x-auto border border-gray-200 rounded-lg">
      <table className="min-w-full divide-y divide-gray-200 text-sm">
        <thead className="bg-gray-50">
          <tr>
            <th className="px-4 py-3 text-left font-semibold text-gray-700">
              Rank
            </th>
            <th className="px-4 py-3 text-left font-semibold text-gray-700">
              Bidder
            </th>
            <th className="px-4 py-3 text-right font-semibold text-gray-700">
              Bid Amount
            </th>
            <th className="px-4 py-3 text-right font-semibold text-gray-700">
              Expected Return
            </th>
            <th className="px-4 py-3 text-center font-semibold text-gray-700">
              Status
            </th>
            <th className="px-4 py-3 text-right font-semibold text-gray-700">
              Submitted
            </th>
          </tr>
        </thead>
        <tbody className="divide-y divide-gray-100 bg-white">
          {bids.map((bid, index) => {
            const dateStr = bid.timestamp
              ? new Date(bid.timestamp * 1000).toLocaleDateString(undefined, {
                  month: "short",
                  day: "numeric",
                  hour: "2-digit",
                  minute: "2-digit",
                })
              : "N/A";

            return (
              <tr
                key={bid.bid_id}
                className={index === 0 ? "bg-amber-50/50" : undefined}
              >
                <td className="px-4 py-3 font-medium text-gray-900">
                  {index === 0 ? (
                    <span className="inline-flex items-center gap-1 font-bold text-amber-700">
                      #{index + 1} Best
                    </span>
                  ) : (
                    `#${index + 1}`
                  )}
                </td>
                <td className="px-4 py-3 font-mono text-xs text-gray-600">
                  {truncateAddress(bid.investor)}
                </td>
                <td className="px-4 py-3 text-right font-medium text-gray-900">
                  {formatAmount(bid.bid_amount)}
                </td>
                <td className="px-4 py-3 text-right font-medium text-emerald-700">
                  {formatAmount(bid.expected_return)}
                </td>
                <td className="px-4 py-3 text-center">
                  <span className="inline-block px-2 py-0.5 text-xs rounded-full bg-blue-100 text-blue-800 font-medium">
                    {bid.status}
                  </span>
                </td>
                <td className="px-4 py-3 text-right text-xs text-gray-500">
                  {dateStr}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
