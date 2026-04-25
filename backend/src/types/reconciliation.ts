export interface DriftReport {
  timestamp: number;
  totalRecordsChecked: number;
  driftCount: number;
  drifts: DriftItem[];
}

export interface DriftItem {
  id: string;
  type: "Invoice" | "Bid" | "Settlement";
  driftType: "MISSING" | "STATUS_MISMATCH" | "DATA_MISMATCH";
  indexedValue?: any;
  onChainValue?: any;
}

export interface BackfillResult {
  successCount: number;
  failCount: number;
  errors: string[];
}
