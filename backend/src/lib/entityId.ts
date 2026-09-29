export const ENTITY_PREFIXES = {
  INVOICE: "inv_",
  BID: "bid_",
  SETTLEMENT: "stl_",
  EXPORT_TOKEN: "exp_",
} as const;

const ULID_REGEX = /^[0-9A-HJKMNP-TV-Z]{26}$/i;

class BadRequestError extends Error {
  public readonly statusCode = 400;
  public readonly code: string;
  public readonly status = 400;

  constructor(message: string, code: string) {
    super(message);
    this.name = "BadRequestError";
    this.code = code;
  }
}

function assertEntityId(prefix: string, value: unknown): asserts value is string {
  if (typeof value !== "string") {
    throw new BadRequestError("Invalid entity ID", "INVALID_ENTITY_ID");
  }

  const trimmed = value.trim();

  if (!trimmed.startsWith(prefix)) {
    throw new BadRequestError("Invalid entity ID", "INVALID_ENTITY_ID");
  }

  const ulidPart = trimmed.slice(prefix.length);

  if (ulidPart.length !== 26 || !ULID_REGEX.test(ulidPart)) {
    throw new BadRequestError("Invalid entity ID", "INVALID_ENTITY_ID");
  }
}

/**
 * Asserts that `value` is a valid invoice ID (`inv_` + 26-char ULID or `0x` hex string).
 */
export function assertInvoiceId(value: unknown): asserts value is string {
  if (typeof value !== "string") {
    throw new BadRequestError("Invalid entity ID", "INVALID_ENTITY_ID");
  }
  const trimmed = value.trim();
  if (trimmed.startsWith("0x")) {
    if (!/^0x[a-fA-F0-9]+$/.test(trimmed)) {
      throw new BadRequestError("Invalid entity ID", "INVALID_ENTITY_ID");
    }
    return;
  }
  assertEntityId(ENTITY_PREFIXES.INVOICE, value);
}

/**
 * Asserts that `value` is a valid bid ID (`bid_` + 26-char ULID).
 */
export function assertBidId(value: unknown): asserts value is string {
  assertEntityId(ENTITY_PREFIXES.BID, value);
}

/**
 * Asserts that `value` is a valid settlement ID (`stl_` + 26-char ULID).
 */
export function assertSettlementId(value: unknown): asserts value is string {
  assertEntityId(ENTITY_PREFIXES.SETTLEMENT, value);
}

/**
 * Asserts that `value` is a valid export token (`exp_` + 26-char ULID).
 */
export function assertExportToken(value: unknown): asserts value is string {
  assertEntityId(ENTITY_PREFIXES.EXPORT_TOKEN, value);
}
