import React from "react";
import { Bid } from "../_lib/view-model";

export interface BestBidBadgeProps {
  bid?: Bid | null;
  loading?: boolean;
}

export function BestBidBadge({ bid, loading }: BestBidBadgeProps) {
  if (loading) {
    return (
      <div className="flex items-center gap-1.5 text-xs text-gray-400 animate-pulse">
        <span className="inline-block w-2 h-2 rounded-full bg-gray-300"></span>
        <span>Checking bids…</span>
      </div>
    );
  }

  if (!bid) {
    return (
      <span className="inline-flex items-center text-xs text-gray-500 font-medium">
        No bids yet
      </span>
    );
  }

  const formattedAmount = (() => {
    try {
      return BigInt(bid.bid_amount).toLocaleString();
    } catch {
      return bid.bid_amount;
    }
  })();

  return (
    <div className="flex items-center justify-between text-xs bg-amber-50 border border-amber-200 text-amber-900 px-2 py-1 rounded">
      <span className="font-medium">Best Bid:</span>
      <span className="font-semibold">{formattedAmount}</span>
    </div>
  );
}
