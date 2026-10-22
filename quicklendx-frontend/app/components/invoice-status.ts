export type InvoiceStatus =
  | "Pending"
  | "Verified"
  | "Funded"
  | "Paid"
  | "Defaulted"
  | "Cancelled";

export const INVOICE_STATUSES: readonly InvoiceStatus[] = [
  "Pending",
  "Verified",
  "Funded",
  "Paid",
  "Defaulted",
  "Cancelled",
] as const;

export interface InvoiceStatusMeta {
  status: InvoiceStatus;
  label: string;
  rail: string;
  railClass: string;
  badgeClass: string;
  iconName: string;
}

export const INVOICE_STATUS_MAP: Record<InvoiceStatus, InvoiceStatusMeta> = {
  Pending: {
    status: "Pending",
    label: "Pending verification",
    rail: "amber-400",
    railClass: "border-l-amber-400",
    badgeClass: "bg-amber-50 text-amber-800 border-amber-200",
    iconName: "hourglass.bottomhalf.fill",
  },
  Verified: {
    status: "Verified",
    label: "Open for bids",
    rail: "blue-500",
    railClass: "border-l-blue-500",
    badgeClass: "bg-blue-50 text-blue-800 border-blue-200",
    iconName: "tag.fill",
  },
  Funded: {
    status: "Funded",
    label: "Funded",
    rail: "indigo-500",
    railClass: "border-l-indigo-500",
    badgeClass: "bg-indigo-50 text-indigo-800 border-indigo-200",
    iconName: "banknote.fill",
  },
  Paid: {
    status: "Paid",
    label: "Paid",
    rail: "emerald-500",
    railClass: "border-l-emerald-500",
    badgeClass: "bg-emerald-50 text-emerald-800 border-emerald-200",
    iconName: "checkmark.circle.fill",
  },
  Defaulted: {
    status: "Defaulted",
    label: "Defaulted",
    rail: "rose-600",
    railClass: "border-l-rose-600",
    badgeClass: "bg-rose-50 text-rose-800 border-rose-200",
    iconName: "exclamationmark.triangle.fill",
  },
  Cancelled: {
    status: "Cancelled",
    label: "Cancelled",
    rail: "gray-300",
    railClass: "border-l-gray-300",
    badgeClass: "bg-gray-100 text-gray-800 border-gray-300",
    iconName: "xmark.circle.fill",
  },
};

export function getInvoiceStatusMeta(status: InvoiceStatus): InvoiceStatusMeta {
  return INVOICE_STATUS_MAP[status];
}
