import type { Settlement } from "./data";

export type SettlementStatusFilter = "All" | Settlement["status"];
export type SettlementRange = "30d" | "90d" | "all";
export type SettlementSort =
  | "date_desc"
  | "date_asc"
  | "amount_desc"
  | "amount_asc";

export interface SettlementSelection {
  address: string;
  status: SettlementStatusFilter;
  range: SettlementRange;
  sort: SettlementSort;
  page: number;
  now?: number;
}

export const SETTLEMENT_PAGE_SIZE = 20;

export function selectRows(
  all: Settlement[],
  { address, status, range, sort, page, now = Date.now() }: SettlementSelection
): Settlement[] {
  const cutoff =
    range === "all"
      ? undefined
      : now - (range === "30d" ? 30 : 90) * 24 * 60 * 60 * 1000;

  const rows = all.filter((settlement) => {
    // The API has no account filter; this client-side scope filter is not a security boundary.
    if (settlement.payer !== address && settlement.recipient !== address) {
      return false;
    }
    if (status !== "All" && settlement.status !== status) return false;
    if (cutoff !== undefined && settlement.timestamp * 1000 < cutoff) {
      return false;
    }
    return true;
  });

  rows.sort((left, right) => {
    switch (sort) {
      case "date_asc":
        return left.timestamp - right.timestamp;
      case "amount_desc": {
        const amountOrder =
          BigInt(right.amount) > BigInt(left.amount)
            ? 1
            : BigInt(right.amount) < BigInt(left.amount)
              ? -1
              : 0;
        return amountOrder || right.timestamp - left.timestamp;
      }
      case "amount_asc": {
        const amountOrder =
          BigInt(left.amount) > BigInt(right.amount)
            ? 1
            : BigInt(left.amount) < BigInt(right.amount)
              ? -1
              : 0;
        return amountOrder || right.timestamp - left.timestamp;
      }
      default:
        return right.timestamp - left.timestamp;
    }
  });

  return rows.slice(0, Math.max(1, page) * SETTLEMENT_PAGE_SIZE);
}

export function sumAmounts(rows: Settlement[]): bigint {
  return rows.reduce((total, row) => total + BigInt(row.amount), 0n);
}
