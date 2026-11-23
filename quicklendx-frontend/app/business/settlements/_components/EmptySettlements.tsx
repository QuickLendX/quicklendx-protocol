export function EmptySettlements() {
  return (
    <div className="rounded-xl border border-dashed border-slate-300 bg-white px-6 py-12 text-center">
      <span
        className="mx-auto mb-4 flex h-12 w-12 items-center justify-center rounded-full bg-blue-50 text-2xl text-blue-700"
        aria-hidden="true"
      >
        ▤
      </span>
      <h2 className="text-lg font-semibold text-slate-900">
        No settlement history
      </h2>
      <p className="mt-2 text-sm text-slate-600">
        Settlement receipts will appear here when an invoice is settled.
      </p>
    </div>
  );
}
