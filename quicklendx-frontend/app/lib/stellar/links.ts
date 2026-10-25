import { getNetworkConfig } from "./network";

const TX_HASH_PATTERN = /^[0-9a-fA-F]{64}$/;
const ACCOUNT_PATTERN = /^G[A-Z2-7]{55}$/;
const CONTRACT_PATTERN = /^C[A-Z2-7]{55}$/;
const INVOICE_ID_PATTERN = /^0x[0-9a-fA-F]{1,64}$/;

function explorerPath(
  kind: "tx" | "account" | "contract",
  value: string
): string {
  return `${getNetworkConfig().explorerBase}/${kind}/${encodeURIComponent(value)}`;
}

export function explorerTxUrl(hash: string): string | null {
  return TX_HASH_PATTERN.test(hash) ? explorerPath("tx", hash) : null;
}

export function explorerAccountUrl(address: string): string | null {
  return ACCOUNT_PATTERN.test(address) ? explorerPath("account", address) : null;
}

export function explorerContractUrl(id: string): string | null {
  return CONTRACT_PATTERN.test(id) ? explorerPath("contract", id) : null;
}

export function invoicePath(id: string): string | null {
  return INVOICE_ID_PATTERN.test(id)
    ? `/business/invoices/${encodeURIComponent(id)}`
    : null;
}
