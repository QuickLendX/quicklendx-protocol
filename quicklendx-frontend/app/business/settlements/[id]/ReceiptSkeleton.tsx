import React from "react";

export const ReceiptSkeleton: React.FC = () => {
  return (
    <div
      role="status"
      aria-busy="true"
      aria-label="Loading settlement receipt"
      className="space-y-6 animate-pulse"
    >
      {/* Header skeleton */}
      <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-4">
        <div className="space-y-2">
          <div className="h-8 bg-neutral-200 rounded w-48" />
          <div className="h-4 bg-neutral-200 rounded w-36" />
          <div className="h-4 bg-neutral-200 rounded w-28" />
        </div>
        <div className="h-7 bg-neutral-200 rounded-full w-24" />
      </div>

      {/* Block 1 Skeleton: Invoice Details */}
      <div className="bg-white border border-[#E2E8F0] rounded-lg p-6 space-y-4 shadow-[0_1px_3px_rgba(0,0,0,0.1)]">
        <div className="h-4 bg-neutral-200 rounded w-32 mb-4" />
        <div className="space-y-3">
          <div className="flex justify-between py-1.5 border-b border-[#E2E8F0]">
            <div className="h-4 bg-neutral-200 rounded w-24" />
            <div className="h-4 bg-neutral-200 rounded w-32" />
          </div>
          <div className="flex justify-between py-1.5 border-b border-[#E2E8F0]">
            <div className="h-4 bg-neutral-200 rounded w-28" />
            <div className="h-4 bg-neutral-200 rounded w-24" />
          </div>
          <div className="flex justify-between py-1.5 border-b border-[#E2E8F0]">
            <div className="h-4 bg-neutral-200 rounded w-20" />
            <div className="h-4 bg-neutral-200 rounded w-36" />
          </div>
          <div className="flex justify-between py-1.5 border-b border-[#E2E8F0]">
            <div className="h-4 bg-neutral-200 rounded w-24" />
            <div className="h-4 bg-neutral-200 rounded w-28" />
          </div>
          <div className="flex justify-between py-1.5">
            <div className="h-4 bg-neutral-200 rounded w-24" />
            <div className="h-4 bg-neutral-200 rounded w-48" />
          </div>
        </div>
      </div>

      {/* Block 2 Skeleton: Settlement Details */}
      <div className="bg-white border border-[#E2E8F0] rounded-lg p-6 space-y-4 shadow-[0_1px_3px_rgba(0,0,0,0.1)]">
        <div className="h-4 bg-neutral-200 rounded w-40 mb-4" />
        <div className="space-y-3">
          <div className="flex justify-between py-1.5 border-b border-[#E2E8F0]">
            <div className="h-4 bg-neutral-200 rounded w-28" />
            <div className="h-4 bg-neutral-200 rounded w-24" />
          </div>
          <div className="flex justify-between py-1.5 border-b border-[#E2E8F0]">
            <div className="h-4 bg-neutral-200 rounded w-20" />
            <div className="h-4 bg-neutral-200 rounded w-36" />
          </div>
          <div className="flex justify-between py-1.5">
            <div className="h-4 bg-neutral-200 rounded w-20" />
            <div className="h-4 bg-neutral-200 rounded w-36" />
          </div>
        </div>
      </div>

      {/* Actions Skeleton */}
      <div className="flex flex-wrap gap-3 pt-2">
        <div className="h-9 bg-neutral-200 rounded-md w-32" />
        <div className="h-9 bg-neutral-200 rounded-md w-36" />
        <div className="h-9 bg-neutral-200 rounded-md w-24" />
      </div>
    </div>
  );
};
