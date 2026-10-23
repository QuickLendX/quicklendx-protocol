/**
 * Shared identifier helpers for route parameters.
 *
 * On-chain identifiers are hex strings prefixed with `0x`. The backend enforces
 * the same shape in `backend/src/validators/shared.ts` (`hexStringSchema`:
 * `/^0x[a-fA-F0-9]+$/`), so the frontend mirrors it here to reject malformed
 * ids before a request is ever made.
 */

/** Matches the backend's `hexStringSchema` (`0x` followed by hex digits). */
const HEX_ID_PATTERN = /^0x[a-fA-F0-9]+$/;

/**
 * Type guard for on-chain hex identifiers.
 *
 * @example
 * isHexId("0x1a");   // true
 * isHexId("REC-1");  // false
 * isHexId("0x");     // false
 * isHexId("0xZZ");   // false
 */
export function isHexId(value: unknown): value is string {
  return typeof value === "string" && HEX_ID_PATTERN.test(value);
}
