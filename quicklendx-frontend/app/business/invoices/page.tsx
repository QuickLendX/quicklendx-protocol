"use client";

import React, { Suspense, useCallback, useEffect, useState } from "react";
import Link from "next/link";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import { InvoiceCard } from "@/app/components/InvoiceCard";
import {
  INVOICE_STATUSES,
  type InvoiceStatus,
} from "@/app/components/invoice-status";
import {
  InvoiceFilters,
  type InvoiceStatusFilter,
} from "./_components/InvoiceFilters";
import { listInvoices, type Invoice } from "@/app/lib/api/endpoints";
import { useWallet } from "@/app/lib/wallet/WalletProvider";

const DEFAULT_PAGE_LIMIT = 20;

function parseStatusParam(raw: string | null | undefined): InvoiceStatusFilter {
  if (!raw) return "All";
  if ((INVOICE_STATUSES as readonly string[]).includes(raw)) {
    return raw as InvoiceStatus;
  }
  return "All";
}

const BusinessInvoicesView: React.FC = () => {
  const enableNewInvoiceRoute =
    process.env.NEXT_PUBLIC_ENABLE_NEW_INVOICE_ROUTE === "true";
  const wallet = useWallet();
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();

  const urlStatus = parseStatusParam(searchParams?.get("status"));
  const [selectedStatus, setSelectedStatus] =
    useState<InvoiceStatusFilter>(urlStatus);

  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [hasMore, setHasMore] = useState<boolean>(false);
  const [loading, setLoading] = useState<boolean>(false);
  const [loadingMore, setLoadingMore] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setSelectedStatus(urlStatus);
  }, [urlStatus]);

  const isConnected =
    wallet.state === "CONNECTED" && Boolean(wallet.address?.trim());
  const businessAddress = wallet.address?.trim() || "";

  const fetchFirstPage = useCallback(
    async (statusFilter: InvoiceStatusFilter, business: string) => {
      setLoading(true);
      setError(null);
      try {
        const res = await listInvoices({
          business,
          limit: DEFAULT_PAGE_LIMIT,
          ...(statusFilter !== "All" ? { status: statusFilter } : {}),
        });
        setInvoices(res.data);
        setNextCursor(res.next_cursor ?? null);
        setHasMore(Boolean(res.has_more));
      } catch (err: unknown) {
        const msg =
          err instanceof Error ? err.message : "Failed to load invoices.";
        setError(msg);
      } finally {
        setLoading(false);
      }
    },
    []
  );

  useEffect(() => {
    if (!isConnected || !businessAddress) {
      setInvoices([]);
      setNextCursor(null);
      setHasMore(false);
      return;
    }
    void fetchFirstPage(selectedStatus, businessAddress);
  }, [isConnected, businessAddress, selectedStatus, fetchFirstPage]);

  const handleSelectStatus = (nextStatus: InvoiceStatusFilter) => {
    if (nextStatus === selectedStatus) return;
    setSelectedStatus(nextStatus);
    setNextCursor(null);

    try {
      const params = new URLSearchParams(searchParams?.toString() || "");
      if (nextStatus === "All") {
        params.delete("status");
      } else {
        params.set("status", nextStatus);
      }
      const qs = params.toString();
      const targetUrl = `${pathname || "/business/invoices"}${qs ? `?${qs}` : ""}`;
      router?.replace?.(targetUrl);
    } catch {
      // Safe fallback when router/searchParams are not mocked in unit tests
    }
  };

  const handleLoadMore = async () => {
    if (
      !isConnected ||
      !businessAddress ||
      !hasMore ||
      !nextCursor ||
      loadingMore
    ) {
      return;
    }
    setLoadingMore(true);
    setError(null);
    try {
      const res = await listInvoices({
        business: businessAddress,
        limit: DEFAULT_PAGE_LIMIT,
        cursor: nextCursor,
        ...(selectedStatus !== "All" ? { status: selectedStatus } : {}),
      });
      setInvoices((prev) => [...prev, ...res.data]);
      setNextCursor(res.next_cursor ?? null);
      setHasMore(Boolean(res.has_more));
    } catch (err: unknown) {
      const msg =
        err instanceof Error ? err.message : "Failed to load more invoices.";
      setError(msg);
    } finally {
      setLoadingMore(false);
    }
  };

  return (
    <main className="min-h-screen bg-[#F8FAFC] py-8 sm:py-12 px-4 sm:px-6 lg:px-8">
      <div className="max-w-5xl mx-auto space-y-6">
        {/* Header */}
        <header className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-3">
          <div>
            <h1 className="text-2xl sm:text-3xl font-bold text-[#0F172A]">
              Business Invoices
            </h1>
            <p className="mt-1 text-sm text-[#64748B]">
              Track your listed invoices, funding status, bids, and settlements.
            </p>
          </div>
          {enableNewInvoiceRoute && isConnected && (
            <Link
              href="/business/invoices/new"
              className="inline-flex items-center justify-center px-4 py-2 rounded-md bg-[#2563EB] text-white text-sm font-medium hover:brightness-110 focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1"
            >
              Upload Invoice
            </Link>
          )}
        </header>

        {!isConnected ? (
          <section
            aria-label="Wallet connection required"
            className="bg-white border border-[#E2E8F0] rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] p-8 text-center"
          >
            <h2 className="text-lg font-semibold text-[#0F172A]">
              Connect your wallet
            </h2>
            <p className="mt-2 text-sm text-[#64748B] max-w-md mx-auto">
              Connect your Stellar business wallet to view and manage your
              invoices.
            </p>
          </section>
        ) : (
          <>
            <InvoiceFilters
              selectedStatus={selectedStatus}
              onSelectStatus={handleSelectStatus}
            />

            {error && (
              <div
                role="alert"
                className="bg-rose-50 border border-rose-200 rounded-lg p-4 text-sm text-rose-800 flex items-center justify-between gap-4"
              >
                <span>Error: {error}</span>
                <button
                  type="button"
                  onClick={() =>
                    void fetchFirstPage(selectedStatus, businessAddress)
                  }
                  className="px-3 py-1.5 rounded-md bg-white border border-rose-300 text-xs font-medium text-rose-800 hover:bg-rose-100 focus:outline-none focus:ring-2 focus:ring-[#2563EB]"
                >
                  Retry
                </button>
              </div>
            )}

            {loading ? (
              <div
                role="status"
                aria-busy="true"
                aria-live="polite"
                aria-label="Loading invoices"
                className="space-y-4"
              >
                {[0, 1, 2].map((i) => (
                  <div
                    key={i}
                    aria-hidden="true"
                    className="h-24 bg-[#E2E8F0] rounded-lg animate-pulse"
                  />
                ))}
                <span className="sr-only">Loading invoices…</span>
              </div>
            ) : invoices.length === 0 && !error ? (
              <section
                aria-label="Empty invoices state"
                className="bg-white border border-[#E2E8F0] rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] p-8 text-center"
              >
                <h2 className="text-lg font-semibold text-[#0F172A]">
                  No invoices yet
                </h2>
                <p className="mt-2 text-sm text-[#64748B] max-w-md mx-auto">
                  You haven&apos;t uploaded any invoices. Start by uploading
                  your first invoice to begin fundraising.
                </p>
                {enableNewInvoiceRoute && (
                  <div className="mt-5">
                    <Link
                      href="/business/invoices/new"
                      className="inline-flex items-center justify-center px-4 py-2 rounded-md bg-[#2563EB] text-white text-sm font-medium hover:brightness-110 focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1"
                    >
                      Upload Invoice
                    </Link>
                  </div>
                )}
              </section>
            ) : (
              <div className="space-y-4">
                <div className="grid grid-cols-1 gap-4">
                  {invoices.map((invoice) => (
                    <InvoiceCard
                      key={invoice.id}
                      invoice={invoice}
                      variant="expanded"
                    />
                  ))}
                </div>

                {hasMore && (
                  <div className="pt-2 flex justify-center">
                    <button
                      type="button"
                      onClick={handleLoadMore}
                      disabled={loadingMore}
                      className="px-5 py-2.5 rounded-md bg-white border border-[#E2E8F0] text-sm font-medium text-[#0F172A] hover:bg-[#F8FAFC] disabled:opacity-40 disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-[#2563EB] focus:ring-offset-1"
                    >
                      {loadingMore ? "Loading…" : "Load more"}
                    </button>
                  </div>
                )}
              </div>
            )}
          </>
        )}
      </div>
    </main>
  );
};

export default function BusinessInvoicesPage() {
  return (
    <Suspense
      fallback={
        <main className="min-h-screen bg-[#F8FAFC] py-12 px-4 sm:px-6 lg:px-8">
          <div className="max-w-5xl mx-auto">
            <p className="text-sm text-[#64748B]">Loading invoices…</p>
          </div>
        </main>
      }
    >
      <BusinessInvoicesView />
    </Suspense>
  );
}
