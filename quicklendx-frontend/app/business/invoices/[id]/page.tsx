"use client";

import React, { useEffect, useState } from "react";
import Link from "next/link";
import { notFound, useParams } from "next/navigation";
import { StatusIcon } from "@/app/components/InvoiceCard";
import { getInvoiceStatusMeta } from "@/app/components/invoice-status";
import { BidsTable } from "../_components/BidsTable";
import { InvoiceTimeline } from "../_components/InvoiceTimeline";
import { formatAtomicAmount, isValidHexInvoiceId } from "../_lib/math";
import {
  getInvoice,
  getRankedBids,
  isNotFoundError,
  listInvoiceDisputes,
  listSettlements,
  type Bid,
  type Dispute,
  type Invoice,
  type Settlement,
} from "@/app/lib/api/endpoints";
import { useWallet } from "@/app/lib/wallet/WalletProvider";

function formatDateUtc(unixSeconds: number): string {
  const ms = unixSeconds < 10_000_000_000 ? unixSeconds * 1000 : unixSeconds;
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) {
    return String(unixSeconds);
  }
  return d.toLocaleDateString("en-US", {
    year: "numeric",
    month: "long",
    day: "numeric",
    timeZone: "UTC",
  });
}

export default function BusinessInvoiceDetailPage() {
  const params = useParams();
  const rawId = (params?.id as string | undefined) ?? "";

  if (!rawId || !isValidHexInvoiceId(rawId)) {
    notFound();
  }

  const wallet = useWallet();
  const connectedAddress =
    wallet.state === "CONNECTED" ? wallet.address?.trim() : undefined;

  const [invoice, setInvoice] = useState<Invoice | null>(null);
  const [bids, setBids] = useState<Bid[]>([]);
  const [disputes, setDisputes] = useState<Dispute[]>([]);
  const [settlements, setSettlements] = useState<Settlement[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [invoiceNotFound, setInvoiceNotFound] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!rawId || !isValidHexInvoiceId(rawId)) return;

    let cancelled = false;

    async function loadDetail() {
      setLoading(true);
      setInvoiceNotFound(false);
      setError(null);

      try {
        const fetchedInvoice = await getInvoice(rawId);
        if (cancelled) return;
        setInvoice(fetchedInvoice);

        const isOwner =
          Boolean(connectedAddress) &&
          fetchedInvoice.business === connectedAddress;

        if (!isOwner) {
          setBids([]);
          setDisputes([]);
          setSettlements([]);
          return;
        }

        const [bidsRes, disputesRes, settlementsRes] = await Promise.all([
          getRankedBids(rawId),
          listInvoiceDisputes(rawId),
          listSettlements({ invoice_id: rawId }),
        ]);

        if (cancelled) return;
        setBids(bidsRes.data);
        setDisputes(disputesRes.data);
        setSettlements(settlementsRes.data);
      } catch (err: unknown) {
        if (cancelled) return;
        if (isNotFoundError(err)) {
          setInvoiceNotFound(true);
        } else {
          setError(
            err instanceof Error ? err.message : "Failed to load invoice."
          );
        }
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    }

    void loadDetail();

    return () => {
      cancelled = true;
    };
  }, [rawId, connectedAddress]);

  if (invoiceNotFound) {
    return (
      <main className="min-h-screen bg-[#F8FAFC] py-8 sm:py-12 px-4 sm:px-6 lg:px-8">
        <div className="max-w-4xl mx-auto space-y-6">
          <Link
            href="/business/invoices"
            className="inline-flex items-center gap-1 text-sm text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
          >
            ← Back to Invoices
          </Link>
          <section
            aria-label="Invoice not found"
            className="bg-white border border-[#E2E8F0] rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] p-8 text-center"
          >
            <h1 className="text-2xl font-bold text-[#0F172A]">
              Invoice not found
            </h1>
            <p className="mt-2 text-sm text-[#64748B]">
              The requested invoice ({rawId}) could not be found.
            </p>
          </section>
        </div>
      </main>
    );
  }

  if (loading) {
    return (
      <main className="min-h-screen bg-[#F8FAFC] py-8 sm:py-12 px-4 sm:px-6 lg:px-8">
        <div
          role="status"
          aria-busy="true"
          aria-live="polite"
          aria-label={`Loading invoice ${rawId}`}
          className="max-w-4xl mx-auto space-y-6"
        >
          <div
            aria-hidden="true"
            className="h-32 bg-[#E2E8F0] rounded-lg animate-pulse"
          />
          <div
            aria-hidden="true"
            className="h-48 bg-[#E2E8F0] rounded-lg animate-pulse"
          />
          <span className="sr-only">Loading invoice details…</span>
        </div>
      </main>
    );
  }

  if (error || !invoice) {
    return (
      <main className="min-h-screen bg-[#F8FAFC] py-8 sm:py-12 px-4 sm:px-6 lg:px-8">
        <div className="max-w-4xl mx-auto space-y-6">
          <Link
            href="/business/invoices"
            className="inline-flex items-center gap-1 text-sm text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
          >
            ← Back to Invoices
          </Link>
          <div
            role="alert"
            className="bg-rose-50 border border-rose-200 rounded-lg p-6 text-sm text-rose-800"
          >
            <h1 className="text-lg font-semibold text-rose-900">
              Unable to load invoice
            </h1>
            <p className="mt-1">{error || "Unknown error occurred."}</p>
          </div>
        </div>
      </main>
    );
  }

  const isOwner =
    Boolean(connectedAddress) && invoice.business === connectedAddress;
  const statusMeta = getInvoiceStatusMeta(invoice.status);
  const formattedAmount = formatAtomicAmount(invoice.amount, 7);
  const currency = invoice.currency || "USDC";
  const primarySettlement = settlements[0];

  return (
    <main className="min-h-screen bg-[#F8FAFC] py-8 sm:py-12 px-4 sm:px-6 lg:px-8">
      <div className="max-w-4xl mx-auto space-y-6">
        <Link
          href="/business/invoices"
          className="inline-flex items-center gap-1 text-sm text-[#2563EB] hover:underline focus:outline-none focus:ring-2 focus:ring-[#2563EB] rounded"
        >
          ← Back to Invoices
        </Link>

        {/* Invoice Header */}
        <section
          aria-label="Invoice header"
          className={`bg-white border border-[#E2E8F0] border-l-4 ${statusMeta.railClass} rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] p-6 space-y-4`}
        >
          <div className="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-3">
            <div className="min-w-0">
              <h1 className="text-2xl sm:text-3xl font-bold text-[#0F172A] font-mono break-all">
                {invoice.id}
              </h1>
              {invoice.description && (
                <p className="mt-1.5 text-sm text-[#64748B]">
                  {invoice.description}
                </p>
              )}
            </div>

            <span
              className={`inline-flex items-center gap-1.5 self-start px-3 py-1 rounded-full text-sm font-medium border ${statusMeta.badgeClass}`}
            >
              <StatusIcon status={invoice.status} size={18} />
              <span>{statusMeta.label}</span>
            </span>
          </div>

          <dl className="grid grid-cols-1 sm:grid-cols-3 gap-4 pt-4 border-t border-[#E2E8F0]">
            <div>
              <dt className="text-xs font-semibold text-[#64748B] uppercase tracking-wider">
                Amount
              </dt>
              <dd className="mt-1 font-mono text-base font-semibold text-[#0F172A]">
                {formattedAmount}{" "}
                <span className="text-xs font-normal text-[#64748B]">
                  {currency}
                </span>
              </dd>
            </div>

            <div>
              <dt className="text-xs font-semibold text-[#64748B] uppercase tracking-wider">
                Due Date
              </dt>
              <dd className="mt-1 text-sm font-medium text-[#0F172A]">
                {formatDateUtc(invoice.due_date)}
              </dd>
            </div>

            <div>
              <dt className="text-xs font-semibold text-[#64748B] uppercase tracking-wider">
                Category
              </dt>
              <dd className="mt-1 text-sm font-medium text-[#0F172A]">
                {invoice.category || "Other"}
              </dd>
            </div>
          </dl>

          {invoice.tags && invoice.tags.length > 0 && (
            <div className="flex flex-wrap items-center gap-2 pt-3 border-t border-[#E2E8F0]">
              <span className="text-xs font-semibold text-[#64748B] uppercase tracking-wider">
                Tags:
              </span>
              {invoice.tags.map((tag) => (
                <span
                  key={tag}
                  className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-[#F8FAFC] text-[#0F172A] border border-[#E2E8F0]"
                >
                  {tag}
                </span>
              ))}
            </div>
          )}
        </section>

        {!isOwner ? (
          <div
            role="alert"
            className="bg-amber-50 border border-amber-200 rounded-lg p-6 text-sm text-amber-900"
          >
            <p className="font-medium">
              This invoice belongs to another business.
            </p>
          </div>
        ) : (
          <>
            {/* Ranked Bids */}
            <section
              aria-labelledby="bids-section-heading"
              className="bg-white border border-[#E2E8F0] rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] p-6 space-y-4"
            >
              <h2
                id="bids-section-heading"
                className="text-base font-semibold text-[#0F172A]"
              >
                Bids
              </h2>
              <BidsTable bids={bids} currency={currency} decimals={7} />
            </section>

            {/* Disputes */}
            <section
              aria-labelledby="disputes-section-heading"
              className="bg-white border border-[#E2E8F0] rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] p-6 space-y-4"
            >
              <h2
                id="disputes-section-heading"
                className="text-base font-semibold text-[#0F172A]"
              >
                Disputes
              </h2>
              {disputes.length === 0 ? (
                <p className="text-sm text-[#64748B]">
                  No disputes filed for this invoice.
                </p>
              ) : (
                <ul className="space-y-3">
                  {disputes.map((dispute) => (
                    <li
                      key={dispute.id}
                      className="p-4 rounded-md border border-[#E2E8F0] bg-[#F8FAFC] space-y-1"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-mono text-xs text-[#64748B]">
                          #{dispute.id}
                        </span>
                        <span className="inline-flex items-center px-2.5 py-0.5 rounded-full text-xs font-medium bg-amber-50 text-amber-800 border border-amber-200">
                          {dispute.status}
                        </span>
                      </div>
                      <p className="text-sm text-[#0F172A] font-medium">
                        {dispute.reason}
                      </p>
                    </li>
                  ))}
                </ul>
              )}
            </section>

            {/* Settlement */}
            <section
              aria-labelledby="settlement-section-heading"
              className="bg-white border border-[#E2E8F0] rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] p-6 space-y-4"
            >
              <h2
                id="settlement-section-heading"
                className="text-base font-semibold text-[#0F172A]"
              >
                Settlement
              </h2>
              {!primarySettlement ? (
                <p className="text-sm text-[#64748B]">
                  No settlement recorded for this invoice yet.
                </p>
              ) : (
                <div className="flex flex-col sm:flex-row sm:items-center sm:justify-between gap-4">
                  <dl className="grid grid-cols-1 sm:grid-cols-2 gap-4">
                    <div>
                      <dt className="text-xs font-semibold text-[#64748B] uppercase tracking-wider">
                        Settlement Amount
                      </dt>
                      <dd className="mt-1 font-mono text-sm font-semibold text-[#0F172A]">
                        {formatAtomicAmount(primarySettlement.amount, 7)}{" "}
                        {currency}
                      </dd>
                    </div>
                    <div>
                      <dt className="text-xs font-semibold text-[#64748B] uppercase tracking-wider">
                        Settlement Status
                      </dt>
                      <dd className="mt-1 text-sm font-medium text-[#0F172A]">
                        {primarySettlement.status}
                      </dd>
                    </div>
                  </dl>

                  <Link
                    href={`/business/settlements/${primarySettlement.id}`}
                    className="inline-flex items-center justify-center px-4 py-2 rounded-md border border-[#E2E8F0] bg-white text-sm font-medium text-[#2563EB] hover:bg-[#F8FAFC] focus:outline-none focus:ring-2 focus:ring-[#2563EB]"
                  >
                    View Settlement Receipt
                  </Link>
                </div>
              )}
            </section>

            {/* Timeline */}
            <section
              aria-labelledby="timeline-section-heading"
              className="bg-white border border-[#E2E8F0] rounded-lg shadow-[0_1px_3px_rgba(0,0,0,0.1)] p-6 space-y-4"
            >
              <h2
                id="timeline-section-heading"
                className="text-base font-semibold text-[#0F172A]"
              >
                Timeline
              </h2>
              <InvoiceTimeline
                invoice={invoice}
                disputes={disputes}
                settlements={settlements}
              />
            </section>
          </>
        )}
      </div>
    </main>
  );
}
