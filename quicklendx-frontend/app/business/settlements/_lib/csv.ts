import type { Settlement } from "./data";

const CSV_COLUMNS = [
  "id",
  "invoice_id",
  "amount",
  "status",
  "timestamp_iso",
  "payer",
  "recipient",
] as const;

function escapeCell(value: string): string {
  const safeValue = /^[=+\-@\t\r]/.test(value) ? `'${value}` : value;
  return /[",\r\n]/.test(safeValue)
    ? `"${safeValue.replace(/"/g, '""')}"`
    : safeValue;
}

export function settlementsToCsv(rows: Settlement[]): string {
  const header = CSV_COLUMNS.join(",");
  const records = rows.map((row) =>
    [
      row.id,
      row.invoice_id,
      row.amount,
      row.status,
      new Date(row.timestamp * 1000).toISOString(),
      row.payer,
      row.recipient,
    ]
      .map(escapeCell)
      .join(",")
  );
  return [header, ...records].join("\r\n");
}

export function settlementsCsvFilename(now = new Date()): string {
  return `settlements-${now.toISOString().slice(0, 10)}.csv`;
}
