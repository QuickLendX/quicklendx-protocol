import type {
  SettlementRange,
  SettlementSort,
  SettlementStatusFilter,
} from "../_lib/selectRows";

interface SettlementsFiltersProps {
  status: SettlementStatusFilter;
  range: SettlementRange;
  sort: SettlementSort;
  onStatusChange: (status: SettlementStatusFilter) => void;
  onRangeChange: (range: SettlementRange) => void;
  onSortChange: (sort: SettlementSort) => void;
  onExport: () => void;
}

const selectClassName =
  "min-h-10 rounded-md border border-slate-300 bg-white px-3 text-sm text-slate-900 focus:outline-none focus:ring-2 focus:ring-blue-600";

export function SettlementsFilters({
  status,
  range,
  sort,
  onStatusChange,
  onRangeChange,
  onSortChange,
  onExport,
}: SettlementsFiltersProps) {
  return (
    <div className="flex flex-col gap-3 rounded-xl border border-slate-200 bg-white p-4 sm:flex-row sm:flex-wrap sm:items-end">
      <label className="flex flex-col gap-1 text-sm font-medium text-slate-700">
        Status
        <select
          className={selectClassName}
          value={status}
          onChange={(event) =>
            onStatusChange(event.target.value as SettlementStatusFilter)
          }
        >
          <option>All</option>
          <option>Pending</option>
          <option>Paid</option>
          <option>Defaulted</option>
        </select>
      </label>
      <label className="flex flex-col gap-1 text-sm font-medium text-slate-700">
        Date range
        <select
          className={selectClassName}
          value={range}
          onChange={(event) =>
            onRangeChange(event.target.value as SettlementRange)
          }
        >
          <option value="30d">Last 30 days</option>
          <option value="90d">Last 90 days</option>
          <option value="all">All dates</option>
        </select>
      </label>
      <label className="flex flex-col gap-1 text-sm font-medium text-slate-700">
        Sort
        <select
          className={selectClassName}
          value={sort}
          onChange={(event) =>
            onSortChange(event.target.value as SettlementSort)
          }
        >
          <option value="date_desc">Date: newest first</option>
          <option value="date_asc">Date: oldest first</option>
          <option value="amount_desc">Amount: highest first</option>
          <option value="amount_asc">Amount: lowest first</option>
        </select>
      </label>
      <button
        type="button"
        onClick={onExport}
        className="min-h-10 rounded-md border border-blue-700 px-4 text-sm font-semibold text-blue-800 hover:bg-blue-50 focus:outline-none focus:ring-2 focus:ring-blue-600 sm:ml-auto"
      >
        Export CSV
      </button>
    </div>
  );
}
