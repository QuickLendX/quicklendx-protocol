"use client";

import React, { useEffect, useState } from "react";
import Link from "next/link";
import { listPortfolio } from "../lib/api/endpoints";
import { useWallet } from "../lib/wallet/WalletProvider";
import { formatAtomicAmount } from "../business/invoices/_lib/math";

type PortfolioPage = Awaited<ReturnType<typeof listPortfolio>>;
type PortfolioEntry = PortfolioPage["data"][number];

/**
 * Investor portfolio — lists the connected wallet's investment positions
 * from GET /api/v1/portfolio?investor=&cursor=.
 */
export default function PortfolioPageView() {
  const { address } = useWallet();

  if (!address) {
    return (
      <div className="max-w-7xl mx-auto px-4 py-8 sm:px-6 lg:px-8">
        <PageHeading />
        <div
          role="status"
          className="text-center py-16 bg-white border border-gray-200 rounded-xl shadow-sm"
        >
          <h2 className="text-sm font-medium text-gray-900">
            Connect your wallet
          </h2>
          <p className="mt-1 text-sm text-gray-500">
            Connect your wallet to view your investment portfolio.
          </p>
        </div>
      </div>
    );
  }

  // Keyed by address so pagination state resets when the wallet changes.
  return <PortfolioList key={address} investor={address} />;
}

function PageHeading() {
  return (
    <div className="mb-6 flex flex-col gap-2">
      <h1 className="text-3xl font-extrabold text-gray-900 tracking-tight">
        Portfolio
      </h1>
      <p className="text-gray-600 text-sm">
        Your active and completed investment positions.
      </p>
    </div>
  );
}

function PortfolioList({ investor }: { investor: string }) {
  // cursor: cursor for the page currently shown (null = first page).
  const [cursor, setCursor] = useState<string | null>(null);
  // history: cursors of previously visited pages, used by "Previous".
  const [history, setHistory] = useState<(string | null)[]>([]);
  const [page, setPage] = useState<PortfolioPage | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    let active = true;
    setLoading(true);
    setError(null);

    listPortfolio(investor, cursor)
      .then((res) => {
        if (active) setPage(res);
      })
      .catch((err: unknown) => {
        if (!active) return;
        setPage(null);
        setError(
          err instanceof Error && err.message
            ? err.message
            : "Failed to load portfolio"
        );
      })
      .finally(() => {
        if (active) setLoading(false);
      });

    return () => {
      active = false;
    };
  }, [investor, cursor, attempt]);

  const goNext = () => {
    if (!page?.next_cursor || loading) return;
    setHistory((prev) => [...prev, cursor]);
    setCursor(page.next_cursor);
  };

  const goPrevious = () => {
    if (history.length === 0 || loading) return;
    const previous = history[history.length - 1] ?? null;
    setHistory(history.slice(0, -1));
    setCursor(previous);
  };

  const entries: PortfolioEntry[] = page?.data ?? [];
  const canGoNext = Boolean(page?.has_more && page?.next_cursor) && !loading;
  const canGoPrevious = history.length > 0 && !loading;

  return (
    <div className="max-w-7xl mx-auto px-4 py-8 sm:px-6 lg:px-8">
      <PageHeading />

      {error && (
        <div
          role="alert"
          className="mb-6 p-4 bg-red-50 border border-red-200 text-red-700 rounded-lg text-sm flex items-center justify-between gap-4"
        >
          <span>{error}</span>
          <button
            type="button"
            onClick={() => setAttempt((n) => n + 1)}
            className="px-3 py-1.5 bg-white border border-red-300 hover:bg-red-50 text-red-700 text-sm font-semibold rounded-lg"
          >
            Try again
          </button>
        </div>
      )}

      {loading ? (
        <div className="bg-white border border-gray-200 rounded-xl shadow-sm overflow-hidden">
          <p role="status" className="sr-only">
            Loading portfolio…
          </p>
          <div className="animate-pulse divide-y divide-gray-100">
            {[1, 2, 3].map((i) => (
              <div key={i} className="h-12 bg-gray-50" />
            ))}
          </div>
        </div>
      ) : !error && entries.length === 0 ? (
        <div className="text-center py-16 bg-white border border-gray-200 rounded-xl shadow-sm">
          <h2 className="mt-2 text-sm font-medium text-gray-900">
            No investment positions yet
          </h2>
          <p className="mt-1 text-sm text-gray-500">
            Positions appear here once you fund an invoice.
          </p>
          <Link
            href="/invest"
            className="mt-4 inline-block text-sm font-semibold text-[#2563EB] hover:underline"
          >
            Browse the marketplace
          </Link>
        </div>
      ) : entries.length > 0 ? (
        <div className="overflow-x-auto bg-white border border-gray-200 rounded-xl shadow-sm">
          <table className="min-w-full divide-y divide-gray-200 text-sm">
            <thead className="bg-gray-50">
              <tr>
                <th
                  scope="col"
                  className="px-4 py-3 text-left font-semibold text-gray-700"
                >
                  Invoice
                </th>
                <th
                  scope="col"
                  className="px-4 py-3 text-right font-semibold text-gray-700"
                >
                  Invested
                </th>
                <th
                  scope="col"
                  className="px-4 py-3 text-right font-semibold text-gray-700"
                >
                  Expected return
                </th>
                <th
                  scope="col"
                  className="px-4 py-3 text-left font-semibold text-gray-700"
                >
                  Status
                </th>
              </tr>
            </thead>
            <tbody className="divide-y divide-gray-100">
              {entries.map((entry) => (
                <tr key={entry.id}>
                  <td className="px-4 py-3 break-all">
                    <Link
                      href={`/invest/invoices/${encodeURIComponent(
                        entry.invoice_id
                      )}`}
                      className="text-[#2563EB] hover:underline"
                    >
                      {entry.invoice_id}
                    </Link>
                  </td>
                  <td className="px-4 py-3 text-right tabular-nums">
                    {formatAtomicAmount(entry.invested_amount)}
                  </td>
                  <td className="px-4 py-3 text-right tabular-nums">
                    {formatAtomicAmount(entry.expected_return)}
                  </td>
                  <td className="px-4 py-3">{entry.status}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ) : null}

      {(history.length > 0 || page?.has_more) && (
        <nav
          aria-label="Portfolio pagination"
          className="mt-6 flex items-center justify-between gap-4 text-sm"
        >
          <button
            type="button"
            onClick={goPrevious}
            disabled={!canGoPrevious}
            className="px-4 py-2 bg-white border border-gray-300 hover:bg-gray-50 text-gray-800 font-semibold rounded-lg shadow-sm disabled:opacity-50"
          >
            Previous page
          </button>
          <span className="text-gray-500">Page {history.length + 1}</span>
          <button
            type="button"
            onClick={goNext}
            disabled={!canGoNext}
            className="px-4 py-2 bg-white border border-gray-300 hover:bg-gray-50 text-gray-800 font-semibold rounded-lg shadow-sm disabled:opacity-50"
          >
            Next page
          </button>
        </nav>
      )}
    </div>
  );
}
