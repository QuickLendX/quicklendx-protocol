"use client";

import React from "react";

interface CollateralMeterProps {
  ratio: number;
  isLive?: boolean;
}

/**
 * CollateralMeter component visualizes the health of an invoice's collateral.
 * Follows the specification in docs/ux/collateral-indicator.md.
 */
export const CollateralMeter: React.FC<CollateralMeterProps> = ({
  ratio,
  isLive = true,
}) => {
  // Determine status and styling based on ratio bands
  let status: "healthy" | "caution" | "at-risk" | "critical" = "healthy";
  let colorClass = "bg-emerald-500";
  let textClass = "text-emerald-700";
  let animateClass = "";
  let icon = null;

  if (ratio <= 100) {
    status = "critical";
    colorClass = "bg-rose-500";
    textClass = "text-rose-700 font-bold";
    icon = (
      <span className="mr-1" aria-hidden="true">
        🚨
      </span>
    );
  } else if (ratio < 110) {
    status = "at-risk";
    colorClass = "bg-amber-500";
    textClass = "text-amber-700 font-bold";
    animateClass = "animate-pulse shadow-[0_0_8px_rgba(245,158,11,0.6)]";
    icon = (
      <span className="mr-1" aria-hidden="true">
        ⚠️
      </span>
    );
  } else if (ratio <= 125) {
    status = "caution";
    colorClass = "bg-amber-400";
    textClass = "text-amber-800 font-medium";
  }

  return (
    <div className="flex flex-col space-y-2 p-4 bg-white rounded-lg shadow-sm border border-gray-100 max-w-sm">
      <div className="flex justify-between items-end">
        <div>
          <label className="text-xs font-medium text-gray-500 uppercase tracking-wider flex items-center">
            Collateral Ratio
            <button
              className="ml-1 text-gray-400 hover:text-gray-600 focus:outline-none"
              title="Current market value of locked collateral divided by total bid amount. Values fluctuate with asset prices."
            >
              <svg
                className="h-3.5 w-3.5"
                fill="currentColor"
                viewBox="0 0 20 20"
              >
                <path
                  fillRule="evenodd"
                  d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7-4a1 1 0 11-2 0 1 1 0 012 0zM9 9a1 1 0 000 2v3a1 1 0 001 1h1a1 1 0 100-2v-3a1 1 0 00-1-1H9z"
                  clipRule="evenodd"
                />
              </svg>
            </button>
          </label>
          <div
            className={`text-2xl font-mono mt-0.5 flex items-center ${textClass}`}
          >
            {icon}
            {ratio.toFixed(1)}%
          </div>
        </div>

        {isLive && (
          <div className="text-[10px] text-gray-400 font-medium uppercase pb-1 flex items-center">
            <span className="h-1.5 w-1.5 rounded-full bg-emerald-400 mr-1 animate-ping"></span>
            Live
          </div>
        )}
      </div>

      {/* Meter Bar */}
      <div
        className="h-2.5 w-full bg-gray-100 rounded-full overflow-hidden"
        role="meter"
        aria-valuenow={ratio}
        aria-valuemin={0}
        aria-valuemax={200}
        aria-label={`Collateralization Ratio: ${ratio.toFixed(1)}% (${status})`}
      >
        <div
          className={`h-full transition-all duration-500 ease-out ${colorClass} ${animateClass}`}
          style={{ width: `${Math.min(ratio, 120) * (100 / 120)}%` }} // Normalized to 120% for visual impact
        ></div>
      </div>

      {/* Threshold Markers */}
      <div className="flex justify-between text-[10px] text-gray-400 font-mono px-0.5">
        <span>0%</span>
        <span className="text-amber-500 font-bold border-x border-amber-200 px-1">
          110%
        </span>
        <span>120%+</span>
      </div>

      {/* Reduced Motion Warning (Visually hidden, aria only or shown in specific states) */}
      <p className="sr-only">
        {status === "at-risk"
          ? "Warning: Ratio is below the 110% safety buffer."
          : ""}
      </p>
    </div>
  );
};
