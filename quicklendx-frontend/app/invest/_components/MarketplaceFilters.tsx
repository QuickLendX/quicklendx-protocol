import React from "react";

export interface MarketplaceFiltersState {
  category: string;
  dueWithinDays: string; // "7" | "30" | "90" | ""
  minAmount: string;
  maxAmount: string;
  sortBy: "dueDate" | "amount";
  sortOrder: "asc" | "desc";
}

export interface MarketplaceFiltersProps {
  categories: string[];
  filters: MarketplaceFiltersState;
  onChange: (filters: MarketplaceFiltersState) => void;
}

export function MarketplaceFilters({
  categories,
  filters,
  onChange,
}: MarketplaceFiltersProps) {
  const update = (partial: Partial<MarketplaceFiltersState>) => {
    onChange({ ...filters, ...partial });
  };

  return (
    <div className="bg-white p-4 rounded-xl border border-gray-200 mb-6 shadow-sm flex flex-col md:flex-row flex-wrap gap-4 items-end">
      {/* Category */}
      <div className="w-full md:w-auto flex-1 min-w-[150px]">
        <label
          htmlFor="filter-category"
          className="block text-xs font-semibold text-gray-700 mb-1"
        >
          Category
        </label>
        <select
          id="filter-category"
          value={filters.category}
          onChange={(e) => update({ category: e.target.value })}
          className="w-full text-sm border border-gray-300 rounded-lg px-3 py-2 bg-white focus:outline-none focus:ring-2 focus:ring-indigo-500"
        >
          <option value="">All Categories</option>
          {categories.map((c) => (
            <option key={c} value={c}>
              {c}
            </option>
          ))}
        </select>
      </div>

      {/* Due within */}
      <div className="w-full md:w-auto flex-1 min-w-[150px]">
        <label
          htmlFor="filter-due"
          className="block text-xs font-semibold text-gray-700 mb-1"
        >
          Due Within
        </label>
        <select
          id="filter-due"
          value={filters.dueWithinDays}
          onChange={(e) => update({ dueWithinDays: e.target.value })}
          className="w-full text-sm border border-gray-300 rounded-lg px-3 py-2 bg-white focus:outline-none focus:ring-2 focus:ring-indigo-500"
        >
          <option value="">Any Time</option>
          <option value="7">Due within 7 days</option>
          <option value="30">Due within 30 days</option>
          <option value="90">Due within 90 days</option>
        </select>
      </div>

      {/* Amount Range */}
      <div className="w-full md:w-auto flex-1 min-w-[150px]">
        <label
          htmlFor="filter-min-amount"
          className="block text-xs font-semibold text-gray-700 mb-1"
        >
          Min Amount
        </label>
        <input
          id="filter-min-amount"
          type="number"
          placeholder="Min Amount"
          value={filters.minAmount}
          onChange={(e) => update({ minAmount: e.target.value })}
          className="w-full text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-indigo-500"
        />
      </div>

      <div className="w-full md:w-auto flex-1 min-w-[150px]">
        <label
          htmlFor="filter-max-amount"
          className="block text-xs font-semibold text-gray-700 mb-1"
        >
          Max Amount
        </label>
        <input
          id="filter-max-amount"
          type="number"
          placeholder="Max Amount"
          value={filters.maxAmount}
          onChange={(e) => update({ maxAmount: e.target.value })}
          className="w-full text-sm border border-gray-300 rounded-lg px-3 py-2 focus:outline-none focus:ring-2 focus:ring-indigo-500"
        />
      </div>

      {/* Sort */}
      <div className="w-full md:w-auto flex-1 min-w-[150px]">
        <label
          htmlFor="filter-sort"
          className="block text-xs font-semibold text-gray-700 mb-1"
        >
          Sort By
        </label>
        <select
          id="filter-sort"
          value={`${filters.sortBy}_${filters.sortOrder}`}
          onChange={(e) => {
            const [sortBy, sortOrder] = e.target.value.split("_") as [
              "dueDate" | "amount",
              "asc" | "desc",
            ];
            update({ sortBy, sortOrder });
          }}
          className="w-full text-sm border border-gray-300 rounded-lg px-3 py-2 bg-white focus:outline-none focus:ring-2 focus:ring-indigo-500"
        >
          <option value="dueDate_asc">Due Date (Earliest first)</option>
          <option value="dueDate_desc">Due Date (Latest first)</option>
          <option value="amount_asc">Amount (Lowest first)</option>
          <option value="amount_desc">Amount (Highest first)</option>
        </select>
      </div>

      {/* Clear Filters */}
      {(filters.category ||
        filters.dueWithinDays ||
        filters.minAmount ||
        filters.maxAmount) && (
        <button
          type="button"
          onClick={() =>
            onChange({
              category: "",
              dueWithinDays: "",
              minAmount: "",
              maxAmount: "",
              sortBy: "dueDate",
              sortOrder: "asc",
            })
          }
          className="text-xs text-indigo-600 hover:text-indigo-800 font-medium px-2 py-2"
        >
          Reset Filters
        </button>
      )}
    </div>
  );
}
