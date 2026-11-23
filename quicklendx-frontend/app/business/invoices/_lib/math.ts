/**
 * Exact BigInt financial helpers for QuickLendX.
 * Never converts i128 strings to Number() to prevent IEEE-754 precision loss.
 */

/**
 * Formats an integer atomic amount string (i128) with `decimals` fractional digits.
 * Preserves all exact digits without converting through `Number()`.
 */
export function formatAtomicAmount(
  atomicStr: string,
  decimals: number = 7
): string {
  const trimmed = atomicStr.trim();
  if (!/^-?\d+$/.test(trimmed)) {
    return trimmed;
  }

  const isNegative = trimmed.startsWith("-");
  const digits = (isNegative ? trimmed.slice(1) : trimmed).replace(
    /^0+(?=\d)/,
    ""
  );

  if (decimals <= 0) {
    return (isNegative && digits !== "0" ? "-" : "") + digits;
  }

  const padded = digits.padStart(decimals + 1, "0");
  const intPart = padded.slice(0, padded.length - decimals);
  const fracPart = padded.slice(padded.length - decimals);

  return `${isNegative && (intPart !== "0" || !/^0+$/.test(fracPart)) ? "-" : ""}${intPart}.${fracPart}`;
}

/**
 * Computes implied return percentage from bid_amount and expected_return using BigInt basis points:
 * `(expected_return - bid_amount) * 10000n / bid_amount`
 * Formatted as a percentage with 2 decimals (e.g., "5.00%").
 * A bid of "0" (or invalid/non-positive bid) returns "—" (no division by zero).
 */
export function computeImpliedReturn(
  bidAmount: string,
  expectedReturn: string
): string {
  try {
    const bid = BigInt(bidAmount.trim());
    const expected = BigInt(expectedReturn.trim());

    const zero = BigInt(0);
    if (bid <= zero) {
      return "\u2014";
    }

    const bps = ((expected - bid) * BigInt(10000)) / bid;
    const isNegative = bps < zero;
    const absBps = isNegative ? -bps : bps;

    const hundred = BigInt(100);
    const whole = absBps / hundred;
    const frac = (absBps % hundred).toString().padStart(2, "0");

    return `${isNegative ? "-" : ""}${whole.toString()}.${frac}%`;
  } catch {
    return "\u2014";
  }
}

/**
 * Checks whether an invoice ID is a valid 0x-prefixed hexadecimal string.
 */
export function isValidHexInvoiceId(id: string): boolean {
  return /^0x[0-9a-fA-F]+$/.test(id);
}
