"use client";

import dynamic from "next/dynamic";
import React from "react";

// Dynamically import InvoiceForm so that stellar-sdk is bundled only into this route's chunk
const DynamicInvoiceForm = dynamic(
  () => import("./InvoiceForm").then((mod) => mod.InvoiceForm),
  {
    ssr: false,
    loading: () => (
      <div className="max-w-2xl mx-auto p-6 text-center text-neutral-500">
        Loading invoice creation form…
      </div>
    ),
  }
);

export default function NewInvoicePage() {
  return (
    <main className="min-h-screen bg-neutral-50 py-12 px-4 sm:px-6 lg:px-8">
      <div className="max-w-3xl mx-auto">
        <DynamicInvoiceForm />
      </div>
    </main>
  );
}
