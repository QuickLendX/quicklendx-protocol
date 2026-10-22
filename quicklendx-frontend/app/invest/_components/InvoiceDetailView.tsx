"use client";

import React, { useEffect, useState } from "react";
import Link from "next/link";
import { fetchInvoiceById, fetchRankedBids } from "../../lib/api/endpoints";
import { RankedBidsTable } from "./RankedBidsTable";
import { Bid, PublicInvoice, toPublicInvoice } from "../_lib/view-model";

export interface InvoiceDetailViewProps {
  params: Promise<{ id: string }> | { id: string };
  searchParams?:
    | Promise<{ [key: string]: string | string[] | undefined }>
    | { [key: string]: string | string[] | undefined };
  bidFlowEnabled?: boolean;
}

export function InvoiceDetailView(props: InvoiceDetailViewProps) {
  const [invoiceId, setInvoiceId] = useState<string>(() => {
    if (
      props.params &&
      typeof (props.params as any).then !== "function" &&
      (props.params as any).id
    ) {
      return (props.params as any).id;
    }
    return "";
  });

  useEffect(() => {
    if (props.params) {
      if (typeof (props.params as any).then === "function") {
        Promise.resolve(props.params).then((p) => {
          if (p?.id) setInvoiceId(p.id);
        });
      } else if ((props.params as any).id) {
        setInvoiceId((props.params as any).id);
      }
    }
  }, [props.params]);

  const [invoice, setInvoice] = useState<PublicInvoice | null>(null);
  const [bids, setBids] = useState<Bid[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // bidFlowEnabled defaults to false, but can be overridden via prop or query/flag
  const bidFlowEnabled = props.bidFlowEnabled ?? false;

  useEffect(() => {
    if (!invoiceId) return;
    let isMounted = true;
    async function load() {
      try {
        setLoading(true);
        setError(null);
        const [rawInvoice, rankedBids] = await Promise.all([
          fetchInvoiceById(invoiceId),
          fetchRankedBids(invoiceId),
        ]);
        if (!isMounted) return;
        setInvoice(toPublicInvoice(rawInvoice));
        setBids(rankedBids);
      } catch (err: any) {
        if (!isMounted) return;
        setError(err.message || "Failed to load invoice details");
      } finally {
        if (isMounted) setLoading(false);
      }
    }
    load();
    return () => {
      isMounted = false;
    };
  }, [invoiceId]);

  if (loading) {
    return (
      <div className="max-w-5xl mx-auto px-4 py-8">
        <div className="h-6 bg-gray-200 rounded w-24 mb-6 animate-pulse"></div>
        <div className="bg-white p-6 rounded-xl border border-gray-200 animate-pulse space-y-4">
          <div className="h-8 bg-gray-200 rounded w-1/3"></div>
          <div className="h-12 bg-gray-200 rounded w-1/2"></div>
          <div className="h-4 bg-gray-200 rounded w-full"></div>
        </div>
      </div>
    );
  }

  if (error || !invoice) {
    return (
      <div className="max-w-5xl mx-auto px-4 py-8">
        <Link
          href="/invest"
          className="text-sm font-medium text-indigo-600 hover:text-indigo-800 mb-6 inline-flex items-center gap-1"
        >
          ← Back to Marketplace
        </Link>
        <div className="p-6 bg-red-50 border border-red-200 rounded-xl text-red-700 text-sm">
          {error || "Invoice not found"}
        </div>
      </div>
    );
  }

  const dueDateStr = invoice.due_date
    ? new Date(invoice.due_date * 1000).toLocaleDateString(undefined, {
        year: "numeric",
        month: "long",
        day: "numeric",
      })
    : "N/A";

  const formattedAmount = (() => {
    try {
      return BigInt(invoice.amount).toLocaleString();
    } catch {
      return invoice.amount;
    }
  })();

  return (
    <div className="max-w-5xl mx-auto px-4 py-8 sm:px-6 lg:px-8">
      {/* Breadcrumb Navigation */}
      <nav className="mb-6">
        <Link
          href="/invest"
          className="text-sm font-medium text-indigo-600 hover:text-indigo-800 inline-flex items-center gap-1"
        >
          ← Back to Marketplace
        </Link>
      </nav>

      {/* Main Invoice Card Summary */}
      <div className="bg-white rounded-xl border border-gray-200 p-6 sm:p-8 shadow-sm mb-8">
        <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-4 pb-6 border-b border-gray-100">
          <div>
            <div className="flex items-center gap-3">
              <h1 className="text-2xl font-bold text-gray-900 font-mono">
                {invoice.id}
              </h1>
              <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-emerald-100 text-emerald-800">
                {invoice.status}
              </span>
            </div>
            {invoice.category && (
              <span className="inline-block mt-2 text-xs font-medium bg-indigo-50 text-indigo-700 px-2.5 py-1 rounded-md">
                {invoice.category}
              </span>
            )}
          </div>

          {/* Call To Action: Place a bid */}
          {bidFlowEnabled && (
            <button
              type="button"
              className="px-5 py-2.5 bg-indigo-600 hover:bg-indigo-700 text-white font-semibold text-sm rounded-lg shadow-sm transition-colors"
            >
              Place a bid
            </button>
          )}
        </div>

        {/* Invoice Summary Details */}
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-6 py-6 border-b border-gray-100">
          <div>
            <div className="text-xs font-medium text-gray-500 uppercase tracking-wider">
              Invoice Amount
            </div>
            <div className="text-3xl font-extrabold text-gray-900 mt-1">
              {formattedAmount}{" "}
              <span className="text-sm font-semibold text-gray-500">
                {invoice.currency}
              </span>
            </div>
          </div>

          <div>
            <div className="text-xs font-medium text-gray-500 uppercase tracking-wider">
              Maturity Date
            </div>
            <div className="text-lg font-semibold text-gray-900 mt-1">
              {dueDateStr}
            </div>
          </div>

          <div>
            <div className="text-xs font-medium text-gray-500 uppercase tracking-wider">
              Category
            </div>
            <div className="text-lg font-semibold text-gray-900 mt-1">
              {invoice.category}
            </div>
          </div>
        </div>

        {/* Description & Tags */}
        <div className="pt-6">
          <div className="text-xs font-medium text-gray-500 uppercase tracking-wider mb-2">
            Description
          </div>
          <p className="text-sm text-gray-700 whitespace-pre-line">
            {invoice.description || "No description provided."}
          </p>

          {invoice.tags && invoice.tags.length > 0 && (
            <div className="mt-4 flex flex-wrap gap-2">
              {invoice.tags.map((tag) => (
                <span
                  key={tag}
                  className="text-xs bg-gray-100 text-gray-600 px-2.5 py-1 rounded-md font-medium"
                >
                  #{tag}
                </span>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Ranked Bids Section */}
      <div className="bg-white rounded-xl border border-gray-200 p-6 sm:p-8 shadow-sm">
        <div className="flex items-center justify-between mb-6">
          <div>
            <h2 className="text-xl font-bold text-gray-900">Ranked Bids</h2>
            <p className="text-xs text-gray-500 mt-1">
              Bids ranked by protocol semantics (best offer first).
            </p>
          </div>
          <span className="text-xs bg-gray-100 text-gray-700 px-2.5 py-1 rounded-full font-medium">
            {bids.length} {bids.length === 1 ? "bid" : "bids"}
          </span>
        </div>

        <RankedBidsTable bids={bids} />
      </div>
    </div>
  );
}
