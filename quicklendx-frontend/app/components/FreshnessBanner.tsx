import React from "react";
import { FreshnessMetadata } from "../invest/_lib/view-model";

export interface FreshnessBannerProps {
  freshness?: FreshnessMetadata | null;
}

/**
 * Returns false when indexLagSeconds > 120, blocking irreversible financial actions.
 */
export function isFreshEnoughForAction(
  freshness?: FreshnessMetadata | null
): boolean {
  if (!freshness) return true;
  return freshness.indexLagSeconds <= 120;
}

export function FreshnessBanner({ freshness }: FreshnessBannerProps) {
  if (!freshness) {
    return null;
  }

  const { indexLagSeconds, lastUpdatedAt } = freshness;

  // Format UTC time HH:MM
  let formattedTime = "00:00";
  try {
    const d = new Date(lastUpdatedAt);
    if (!isNaN(d.getTime())) {
      const hours = String(d.getUTCHours()).padStart(2, "0");
      const minutes = String(d.getUTCMinutes()).padStart(2, "0");
      formattedTime = `${hours}:${minutes}`;
    }
  } catch {
    formattedTime = "00:00";
  }

  if (indexLagSeconds > 120) {
    return (
      <div
        role="alert"
        className="w-full bg-red-100 border border-red-300 text-red-800 px-4 py-2 text-sm rounded-md flex items-center justify-between"
      >
        <span>
          Data is significantly delayed. Bidding is paused until it catches up.
        </span>
        <span className="text-xs text-red-600 font-mono">
          Lag: {Math.round(indexLagSeconds)}s
        </span>
      </div>
    );
  }

  if (indexLagSeconds > 30) {
    return (
      <div
        role="status"
        className="w-full bg-amber-100 border border-amber-300 text-amber-800 px-4 py-2 text-sm rounded-md flex items-center justify-between"
      >
        <span>
          Data may be up to {Math.round(indexLagSeconds)} s behind the network
        </span>
        <span className="text-xs text-amber-600 font-mono">
          Data as of {formattedTime} UTC
        </span>
      </div>
    );
  }

  return (
    <div className="w-full text-xs text-gray-500 py-1 flex items-center gap-1">
      <span>Data as of {formattedTime} UTC</span>
    </div>
  );
}
