/**
 * QuickLendX Soroban Contract Error Definitions and Mapping.
 * Maps protocol error codes (quicklendx-contracts/src/errors.rs) to human-readable user copy.
 */

export class ContractError extends Error {
  public readonly code?: number;
  public readonly originalError?: unknown;

  constructor(message: string, code?: number, originalError?: unknown) {
    super(message);
    this.name = "ContractError";
    this.code = code;
    this.originalError = originalError;
  }
}

export const CONTRACT_ERROR_MESSAGES: Record<number, string> = {
  // Invoice lifecycle (1000-1006)
  1000: "Invoice not found.",
  1001: "Invoice is not available for funding.",
  1002: "Invoice has already been funded.",
  1003: "Invoice amount is invalid.",
  1004: "Invoice due date is invalid.",
  1005: "Invoice is not funded.",
  1006: "Invoice has already defaulted.",
  1007: "Invoice is frozen.",
  1008: "Invalid freeze reason provided.",
  1009: "Invoice lock has expired.",
  1010: "Caller is not an authorized arbiter.",

  // Authorization (1100-1104)
  1100: "Unauthorized access.",
  1101: "Caller is not the business owner.",
  1102: "Caller is not an investor.",
  1103: "Caller is not an admin.",
  1104: "Self call is not allowed.",

  // Input validation (1200-1206)
  1200: "Invalid amount.",
  1201: "Invalid address.",
  1202: "Invalid currency.",
  1203: "Invalid timestamp.",
  1204: "Invalid description.",
  1205: "Self transfer is not allowed.",
  1206: "Invalid cursor.",

  // Storage (1300-1301)
  1300: "Contract storage error.",
  1301: "Storage key not found.",

  // Business logic (1400-1410)
  1400: "Insufficient funds.",
  1401: "Invalid status for this operation.",
  1402: "Operation not allowed.",
  1403: "Payment amount is too low.",
  1404: "Platform account is not configured.",
  1405: "Invalid coverage percentage.",
  1406: "Maximum bids per invoice exceeded.",
  1407: "Maximum active bids per investor exceeded.",
  1408: "Your business has reached the maximum number of active invoices.",
  1409: "Invalid bid time-to-live.",
  1410: "Insurance claim window is closed.",

  // Rating (1500-1504)
  1500: "Invalid rating.",
  1501: "Invoice is not funded.",
  1502: "Invoice already rated.",
  1503: "Caller is not an authorized rater.",
  1504: "Invalid rating override reason.",

  // KYC / verification (1600-1660)
  1600: "Your business must complete KYC before creating invoices.",
  1601: "Your KYC is still under review.",
  1602: "KYC already verified.",
  1603: "KYC record not found.",
  1604: "Invalid KYC status.",
  1605: "Investor is not verified.",
  1610: "Investor account is frozen.",
  1660: "Business account has been deleted.",

  // Audit (1700-1702)
  1700: "Audit log not found.",
  1701: "Audit log integrity error.",
  1702: "Audit query error.",

  // Category / tag (1800-1801)
  1800: "One or more tags are invalid.",
  1801: "You have exceeded the maximum allowed tags.",

  // Emergency / pause (2100-2106)
  2100: "The contract is currently paused. Please try again later.",
  2101: "Emergency withdrawal request not found.",
  2102: "Emergency withdrawal timelock has not elapsed.",
  2103: "Emergency withdrawal request has expired.",
  2104: "Emergency withdrawal request was cancelled.",
  2105: "Emergency withdrawal request already exists.",
  2106: "Emergency withdrawal insufficient balance.",

  // Hard input limits / protocol backstops
  2221: "Input data exceeds the maximum allowed size.",
};

/**
 * Returns human-readable error copy for a given contract error code.
 */
export function getContractErrorMessage(code: number): string {
  if (code in CONTRACT_ERROR_MESSAGES) {
    return CONTRACT_ERROR_MESSAGES[code];
  }
  return `Transaction failed (code ${code}).`;
}
