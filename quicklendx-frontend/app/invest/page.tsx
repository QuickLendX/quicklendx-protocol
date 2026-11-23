"use client";

import React, { Suspense, useEffect, useMemo, useRef, useState } from "react";
import { useRouter, useSearchParams } from "next/navigation";
import { fetchBestBid, fetchInvoices } from "../lib/api/endpoints";
import { FreshnessBanner } from "../components/FreshnessBanner";
import { InvoiceCard } from "../components/InvoiceCard";
import { BestBidBadge } from "./_components/BestBidBadge";
import {
  MarketplaceFilters,
  MarketplaceFiltersState,
} from "./_components/MarketplaceFilters";
import { pLimit } from "./_lib/limit";
import {
  Bid,
  FreshnessMetadata,
  PublicInvoice,
  toPublicInvoice,
} from "./_lib/view-model";

const bestBidLimiter = pLimit(4);
const bestBidCache = new Map<string, Bid | null>();

function MarketplaceContent() {
  const router = useRouter();
  const searchParams = useSearchParams();

  const [invoices, setInvoices] = useState<PublicInvoice[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [freshness, setFreshness] = useState<FreshnessMetadata | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Best bids state: invoiceId -> { bid, loading }
  const [bestBids, setBestBids] = useState<
    Record<string, { bid: Bid | null; loading: boolean }>
  >({});

  // Filters state initialized from URL query parameters
  const [filters, setFilters] = useState<MarketplaceFiltersState>(() => {
    return {
      category: searchParams.get("category") || "",
      dueWithinDays: searchParams.get("due") || "",
      minAmount: searchParams.get("min") || "",
      maxAmount: searchParams.get("max") || "",
      sortBy: (searchParams.get("sortBy") as "dueDate" | "amount") || "dueDate",
      sortOrder: (searchParams.get("sortOrder") as "asc" | "desc") || "asc",
    };
  });

  // Mirror filters to URL query string
  const updateFilters = (newFilters: MarketplaceFiltersState) => {
    setFilters(newFilters);
    const params = new URLSearchParams();
    if (newFilters.category) params.set("category", newFilters.category);
    if (newFilters.dueWithinDays) params.set("due", newFilters.dueWithinDays);
    if (newFilters.minAmount) params.set("min", newFilters.minAmount);
    if (newFilters.maxAmount) params.set("max", newFilters.maxAmount);
    if (newFilters.sortBy && newFilters.sortBy !== "dueDate")
      params.set("sortBy", newFilters.sortBy);
    if (newFilters.sortOrder && newFilters.sortOrder !== "asc")
      params.set("sortOrder", newFilters.sortOrder);

    const query = params.toString();
    router.replace(`/invest${query ? `?${query}` : ""}`, { scroll: false });
  };

  // Initial load
  useEffect(() => {
    let isMounted = true;
    async function load() {
      try {
        setLoading(true);
        setError(null);
        const res = await fetchInvoices({ status: "Verified" });
        if (!isMounted) return;
        const publicList = res.data.map(toPublicInvoice);
        setInvoices(publicList);
        setNextCursor(res.next_cursor);
        setHasMore(res.has_more);
        if (res.freshness) setFreshness(res.freshness);
      } catch (err: any) {
        if (!isMounted) return;
        setError(err.message || "Failed to load marketplace invoices");
      } finally {
        if (isMounted) setLoading(false);
      }
    }
    load();
    return () => {
      isMounted = false;
    };
  }, []);

  // Load more
  const handleLoadMore = async () => {
    if (!nextCursor || loadingMore) return;
    try {
      setLoadingMore(true);
      const res = await fetchInvoices({
        status: "Verified",
        cursor: nextCursor,
      });
      const publicList = res.data.map(toPublicInvoice);
      setInvoices((prev) => [...prev, ...publicList]);
      setNextCursor(res.next_cursor);
      setHasMore(res.has_more);
      if (res.freshness) setFreshness(res.freshness);
    } catch (err: any) {
      setError(err.message || "Failed to load more invoices");
    } finally {
      setLoadingMore(false);
    }
  };

  // Extract categories for filter dropdown
  const categories = useMemo(() => {
    const set = new Set<string>();
    for (const inv of invoices) {
      if (inv.category) set.add(inv.category);
    }
    return Array.from(set).sort();
  }, [invoices]);

  // Client-side filtering & sorting
  const filteredInvoices = useMemo(() => {
    const nowSec = Math.floor(Date.now() / 1000);

    return invoices
      .filter((inv) => {
        // Category filter
        if (filters.category && inv.category !== filters.category) {
          return false;
        }

        // Due within days filter
        if (filters.dueWithinDays) {
          const days = parseInt(filters.dueWithinDays, 10);
          if (!isNaN(days) && days > 0) {
            const maxDueSec = nowSec + days * 86400;
            if (inv.due_date > maxDueSec) {
              return false;
            }
          }
        }

        // Amount range filter as BigInt
        try {
          const invAmount = BigInt(inv.amount);
          if (filters.minAmount) {
            const minBi = BigInt(filters.minAmount);
            if (invAmount < minBi) return false;
          }
          if (filters.maxAmount) {
            const maxBi = BigInt(filters.maxAmount);
            if (invAmount > maxBi) return false;
          }
        } catch {
          // If BigInt fails parsing, retain
        }

        return true;
      })
      .sort((a, b) => {
        if (filters.sortBy === "amount") {
          try {
            const aBi = BigInt(a.amount);
            const bBi = BigInt(b.amount);
            if (aBi < bBi) return filters.sortOrder === "asc" ? -1 : 1;
            if (aBi > bBi) return filters.sortOrder === "asc" ? 1 : -1;
            return 0;
          } catch {
            return 0;
          }
        } else {
          // Due date
          const diff = a.due_date - b.due_date;
          return filters.sortOrder === "asc" ? diff : -diff;
        }
      });
  }, [invoices, filters]);

  // Request best bid for card when intersecting
  const triggerBestBid = (invoiceId: string) => {
    if (bestBids[invoiceId] !== undefined) {
      return; // Already fetched or fetching
    }
    if (bestBidCache.has(invoiceId)) {
      setBestBids((prev) => ({
        ...prev,
        [invoiceId]: { bid: bestBidCache.get(invoiceId)!, loading: false },
      }));
      return;
    }

    setBestBids((prev) => ({
      ...prev,
      [invoiceId]: { bid: null, loading: true },
    }));

    bestBidLimiter(async () => {
      try {
        const bid = await fetchBestBid(invoiceId);
        bestBidCache.set(invoiceId, bid);
        setBestBids((prev) => ({
          ...prev,
          [invoiceId]: { bid, loading: false },
        }));
      } catch {
        bestBidCache.set(invoiceId, null);
        setBestBids((prev) => ({
          ...prev,
          [invoiceId]: { bid: null, loading: false },
        }));
      }
    });
  };

  return (
    <div className="max-w-7xl mx-auto px-4 py-8 sm:px-6 lg:px-8">
      {/* Header & Freshness */}
      <div className="mb-6 flex flex-col gap-2">
        <h1 className="text-3xl font-extrabold text-gray-900 tracking-tight">
          Investor Marketplace
        </h1>
        <p className="text-gray-600 text-sm">
          Browse verified invoices open for competitive funding. Connect your
          wallet to place bids.
        </p>
        <FreshnessBanner freshness={freshness} />
      </div>

      {/* Filters */}
      <MarketplaceFilters
        categories={categories}
        filters={filters}
        onChange={updateFilters}
      />

      {/* Error State */}
      {error && (
        <div className="mb-6 p-4 bg-red-50 border border-red-200 text-red-700 rounded-lg text-sm">
          {error}
        </div>
      )}

      {/* Loading Skeletons */}
      {loading ? (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {[1, 2, 3, 4, 5, 6].map((i) => (
            <div
              key={i}
              className="bg-white rounded-xl border border-gray-200 p-6 animate-pulse h-64"
            >
              <div className="h-4 bg-gray-200 rounded w-1/3 mb-4"></div>
              <div className="h-8 bg-gray-200 rounded w-2/3 mb-4"></div>
              <div className="h-4 bg-gray-200 rounded w-1/2 mb-2"></div>
              <div className="h-4 bg-gray-200 rounded w-full mt-6"></div>
            </div>
          ))}
        </div>
      ) : filteredInvoices.length === 0 ? (
        <div className="text-center py-16 bg-white border border-gray-200 rounded-xl shadow-sm">
          <svg
            className="mx-auto h-12 w-12 text-gray-400"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            aria-hidden="true"
          >
            <path
              strokeLinecap="round"
              strokeLinejoin="round"
              strokeWidth={1}
              d="M9 12h6m-6 4h6m2 5H7a2 2 0 01-2-2V5a2 2 0 012-2h5.586a1 1 0 01.707.293l5.414 5.414a1 1 0 01.293.707V19a2 2 0 01-2 2z"
            />
          </svg>
          <h3 className="mt-2 text-sm font-medium text-gray-900">
            No invoices found
          </h3>
          <p className="mt-1 text-sm text-gray-500">
            No verified invoices matched your search filters.
          </p>
        </div>
      ) : (
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
          {filteredInvoices.map((inv) => (
            <ObserverCard
              key={inv.id}
              invoice={inv}
              bestBidState={bestBids[inv.id]}
              onVisible={() => triggerBestBid(inv.id)}
            />
          ))}
        </div>
      )}

      {/* Load More */}
      {hasMore && (
        <div className="mt-10 text-center">
          <button
            type="button"
            onClick={handleLoadMore}
            disabled={loadingMore}
            className="px-6 py-2.5 bg-white border border-gray-300 hover:bg-gray-50 text-gray-800 text-sm font-semibold rounded-lg shadow-sm transition-colors disabled:opacity-50"
          >
            {loadingMore ? "Loading more…" : "Load more"}
          </button>
        </div>
      )}
    </div>
  );
}

function ObserverCard({
  invoice,
  bestBidState,
  onVisible,
}: {
  invoice: PublicInvoice;
  bestBidState?: { bid: Bid | null; loading: boolean };
  onVisible: () => void;
}) {
  const cardRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = cardRef.current;
    if (!el || typeof IntersectionObserver === "undefined") {
      onVisible();
      return;
    }

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            onVisible();
            observer.unobserve(entry.target);
          }
        }
      },
      { threshold: 0.1 }
    );

    observer.observe(el);

    return () => {
      observer.disconnect();
    };
  }, [onVisible]);

  return (
    <div ref={cardRef}>
      <InvoiceCard
        invoice={invoice}
        bestBidSlot={
          <BestBidBadge
            bid={bestBidState?.bid}
            loading={bestBidState?.loading}
          />
        }
      />
    </div>
  );
}

export default function InvestMarketplacePage() {
  return (
    <Suspense
      fallback={
        <div className="max-w-7xl mx-auto px-4 py-8">
          <div className="h-8 bg-gray-200 rounded w-1/4 mb-4 animate-pulse"></div>
          <div className="h-4 bg-gray-200 rounded w-1/2 mb-8 animate-pulse"></div>
        </div>
      }
    >
      <MarketplaceContent />
    </Suspense>
  );
}
