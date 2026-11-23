"use client";

import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import Link from "next/link";
import { usePathname, useRouter, useSearchParams } from "next/navigation";
import {
  WalletConnectControl,
  useWallet,
} from "@/app/lib/wallet/WalletProvider";
import { EmptySettlements } from "./_components/EmptySettlements";
import { SettlementsFilters } from "./_components/SettlementsFilters";
import { SettlementsTable } from "./_components/SettlementsTable";
import { listSettlements, type Settlement } from "./_lib/data";
import { settlementsCsvFilename, settlementsToCsv } from "./_lib/csv";
import {
  selectRows,
  SETTLEMENT_PAGE_SIZE,
  sumAmounts,
  type SettlementRange,
  type SettlementSort,
  type SettlementStatusFilter,
} from "./_lib/selectRows";

function validStatus(value: string | null): SettlementStatusFilter {
  return value === "Pending" || value === "Paid" || value === "Defaulted"
    ? value
    : "All";
}

function validRange(value: string | null): SettlementRange {
  return value === "90d" || value === "all" ? value : "30d";
}

function validSort(value: string | null): SettlementSort {
  return value === "date_asc" ||
    value === "amount_desc" ||
    value === "amount_asc"
    ? value
    : "date_desc";
}

function readFilters(search: string) {
  const params = new URLSearchParams(search);
  return {
    status: validStatus(params.get("status")),
    range: validRange(params.get("range")),
    sort: validSort(params.get("sort")),
  };
}

function SettlementsContent() {
  const wallet = useWallet();
  const router = useRouter();
  const pathname = usePathname();
  const searchParams = useSearchParams();
  const search = searchParams.toString();
  const [allRows, setAllRows] = useState<Settlement[]>([]);
  const [freshnessLag, setFreshnessLag] = useState<number | null>(null);
  const [loadState, setLoadState] = useState<"loading" | "ready" | "error">(
    "loading"
  );
  const [retryCount, setRetryCount] = useState(0);
  const [page, setPage] = useState(1);
  const [focusNewRow, setFocusNewRow] = useState<number | null>(null);
  const [filters, setFilters] = useState(() => readFilters(search));
  const { status, range, sort } = filters;

  useEffect(() => {
    setFilters(readFilters(search));
    setPage(1);
  }, [search]);

  useEffect(() => {
    if (wallet.state !== "connected" || !wallet.address) return;

    let active = true;
    setLoadState("loading");
    listSettlements({})
      .then((response) => {
        if (!active) return;
        setAllRows(response.data);
        setFreshnessLag(response.freshness.indexLagSeconds);
        setLoadState("ready");
      })
      .catch(() => {
        if (active) setLoadState("error");
      });

    return () => {
      active = false;
    };
  }, [wallet.address, wallet.state, retryCount]);

  const filteredRows = useMemo(
    () =>
      wallet.address
        ? selectRows(allRows, {
            address: wallet.address,
            status,
            range,
            sort,
            page: Math.ceil(allRows.length / SETTLEMENT_PAGE_SIZE) || 1,
          })
        : [],
    [allRows, range, sort, status, wallet.address]
  );
  const visibleRows = filteredRows.slice(0, page * SETTLEMENT_PAGE_SIZE);
  const total = sumAmounts(filteredRows);

  useEffect(() => {
    if (focusNewRow === null) return;
    document
      .querySelector<HTMLElement>(`[data-row-index="${focusNewRow}"]`)
      ?.focus();
    setFocusNewRow(null);
  }, [focusNewRow, visibleRows.length]);

  const updateUrl = useCallback(
    (
      next: Partial<{
        status: SettlementStatusFilter;
        range: SettlementRange;
        sort: SettlementSort;
      }>
    ) => {
      const params = new URLSearchParams(search);
      const values = {
        status: next.status ?? status,
        range: next.range ?? range,
        sort: next.sort ?? sort,
      };
      setFilters(values);
      for (const [key, value] of Object.entries(values)) {
        const defaultValue =
          key === "status" ? "All" : key === "range" ? "30d" : "date_desc";
        if (value === defaultValue) params.delete(key);
        else params.set(key, value);
      }
      const query = params.toString();
      router.replace(query ? `${pathname}?${query}` : pathname, {
        scroll: false,
      });
    },
    [pathname, range, router, search, sort, status]
  );

  const exportCsv = useCallback(() => {
    const blob = new Blob([settlementsToCsv(filteredRows)], {
      type: "text/csv;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement("a");
    link.href = url;
    link.download = settlementsCsvFilename();
    link.click();
    URL.revokeObjectURL(url);
  }, [filteredRows]);

  if (wallet.state !== "connected" || !wallet.address) {
    return (
      <main className="mx-auto flex min-h-screen max-w-5xl flex-col px-4 py-10 sm:px-6 lg:px-8">
        <Link
          href="/business"
          className="mb-8 w-fit text-sm font-medium text-blue-800 underline underline-offset-2 focus:outline-none focus:ring-2 focus:ring-blue-600"
        >
          Back to business dashboard
        </Link>
        <section className="m-auto max-w-lg rounded-xl border border-slate-200 bg-white p-8 text-center shadow-sm">
          <h1 className="text-2xl font-bold text-slate-950">
            Settlement receipts
          </h1>
          <p className="mt-3 text-slate-700">
            Connect your wallet to see your settlements.
          </p>
          <div className="mt-6 flex justify-center">
            <WalletConnectControl />
          </div>
        </section>
      </main>
    );
  }

  return (
    <main className="mx-auto min-h-screen max-w-7xl space-y-6 px-4 py-8 sm:px-6 lg:px-8">
      <header className="space-y-2">
        <Link
          href="/business"
          className="inline-flex min-h-8 items-center text-sm font-medium text-blue-800 underline underline-offset-2 focus:outline-none focus:ring-2 focus:ring-blue-600"
        >
          Back to business dashboard
        </Link>
        <div>
          <p className="text-sm font-semibold uppercase tracking-wide text-blue-800">
            Business
          </p>
          <h1 className="text-3xl font-bold text-slate-950">
            Settlement receipts
          </h1>
          <p className="mt-1 text-slate-600">
            View and export settlement activity for this wallet.
          </p>
        </div>
      </header>

      <SettlementsFilters
        status={status}
        range={range}
        sort={sort}
        onStatusChange={(value) => {
          setPage(1);
          updateUrl({ status: value });
        }}
        onRangeChange={(value) => {
          setPage(1);
          updateUrl({ range: value });
        }}
        onSortChange={(value) => {
          setPage(1);
          updateUrl({ sort: value });
        }}
        onExport={exportCsv}
      />

      {freshnessLag !== null && freshnessLag > 30 && (
        <p
          role="status"
          className="rounded-md border border-amber-300 bg-amber-50 px-4 py-3 text-sm text-amber-950"
        >
          Data may be up to {freshnessLag} seconds behind the network.
        </p>
      )}

      {loadState === "error" ? (
        <div
          role="alert"
          className="flex flex-col items-start gap-3 rounded-lg border border-rose-300 bg-rose-50 p-4 text-rose-950 sm:flex-row sm:items-center sm:justify-between"
        >
          <p>Could not load settlements. Please try again.</p>
          <button
            type="button"
            onClick={() => setRetryCount((count) => count + 1)}
            className="min-h-10 rounded-md bg-rose-800 px-4 font-semibold text-white hover:bg-rose-900 focus:outline-none focus:ring-2 focus:ring-rose-700 focus:ring-offset-2"
          >
            Retry
          </button>
        </div>
      ) : (
        <>
          <SettlementsTable
            rows={visibleRows}
            loading={loadState === "loading"}
          />

          {loadState === "ready" && filteredRows.length === 0 && (
            <EmptySettlements />
          )}

          {loadState === "ready" && filteredRows.length > 0 && (
            <>
              {visibleRows.length < filteredRows.length && (
                <div className="flex justify-center">
                  <button
                    type="button"
                    onClick={() => {
                      setFocusNewRow(visibleRows.length);
                      setPage((current) => current + 1);
                    }}
                    className="min-h-11 rounded-md border border-blue-700 px-5 font-semibold text-blue-800 hover:bg-blue-50 focus:outline-none focus:ring-2 focus:ring-blue-600"
                  >
                    Load more
                  </button>
                </div>
              )}
              <footer className="rounded-lg bg-slate-100 px-4 py-4 text-right font-semibold text-slate-900">
                Total this period: {total.toString()} units
              </footer>
            </>
          )}
        </>
      )}
    </main>
  );
}

export default function SettlementsPage() {
  return (
    <Suspense
      fallback={
        <main className="mx-auto max-w-7xl px-4 py-8" aria-busy="true">
          Loading settlement receipts…
        </main>
      }
    >
      <SettlementsContent />
    </Suspense>
  );
}
