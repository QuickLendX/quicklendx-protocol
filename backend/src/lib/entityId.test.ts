import { ENTITY_PREFIXES, assertInvoiceId, assertEntityId } from "./entityId";

const VALID_ULID = "01ARZ3NDEKTSV4RRFFQ69G5FAV";

function expectInvalid(value: unknown): void {
  try {
    assertInvoiceId(value);
    throw new Error("expected assertInvoiceId to reject");
  } catch (error) {
    expect(error).toMatchObject({
      name: "BadRequestError",
      status: 400,
      statusCode: 400,
      code: "INVALID_ENTITY_ID",
      message: "Invalid entity ID",
    });
  }
}

describe("assertInvoiceId deterministic failure boundaries", () => {
  it.each([
    `${ENTITY_PREFIXES.INVOICE}${VALID_ULID}`,
    `${ENTITY_PREFIXES.INVOICE}${VALID_ULID.toLowerCase()}`,
    `  ${ENTITY_PREFIXES.INVOICE}${VALID_ULID}  `,
    "0x0",
    "0xdeadBEEF",
    "  0xdeadBEEF  ",
  ])("accepts valid representation %j repeatedly", (value) => {
    expect(() => assertInvoiceId(value)).not.toThrow();
    expect(() => assertInvoiceId(value)).not.toThrow();
  });

  it.each([null, undefined, 0, false, {}, []])(
    "rejects non-string input %p with the stable public error",
    (value) => {
      expectInvalid(value);
      expectInvalid(value);
    },
  );

  it.each([
    "",
    "   ",
    ENTITY_PREFIXES.INVOICE,
    `${ENTITY_PREFIXES.INVOICE}${"0".repeat(25)}`,
    `${ENTITY_PREFIXES.INVOICE}${"0".repeat(27)}`,
    `${ENTITY_PREFIXES.INVOICE}${"I".repeat(26)}`,
    `bid_${VALID_ULID}`,
    "0x",
    "0XdeadBEEF",
    "0xdead-beef",
    "0xnothex",
  ])("rejects malformed boundary representation %j deterministically", (value) => {
    expectInvalid(value);
    expectInvalid(value);
  });

  it("does not mutate a caller-owned string across retry-style validation", () => {
    const invoiceId = `  ${ENTITY_PREFIXES.INVOICE}${VALID_ULID}  `;
    const original = invoiceId;

    assertInvoiceId(invoiceId);
    assertInvoiceId(invoiceId);

    expect(invoiceId).toBe(original);
  });
});
describe("assertEntityId deterministic failure boundaries", () => {
  it.each([
    [`${VALID_ULID}, ENTITY_PREFIXES.BID],
    [`${VALID_ULID}, ENTITY_PREFIXES.SETTLEMENT],
    [`${VALID_ULID}, ENTITY_PREFIXES.EXPORT_TOKEN],
    [  ${ENTITY_PREFIXES.BID}${VALID_ULID}  , ENTITY_PREFIXES.BID],
    [`${VALID_ULID.toLowerCase()}, ENTITY_PREFIXES.BID],
  ])("accepts valid representation %j for prefix %j repeatedly", (value, prefix) => {
    expect(() => assertEntityId(prefix, value)).not.toThrow();
    expect(() => assertEntityId(prefix, value)).not.toThrow();
  });

  it.each([null, undefined, 0, false, {}, []])(
    "rejects non-string input %p with the stable public error",
    (value) => {
      try {
        assertEntityId(ENTITY_PREFIXES.BID, value);
        throw new Error("expected assertEntityId to reject");
      } catch (error) {
        expect(error).toMatchObject({
          name: "BadRequestError",
          status: 400,
          statusCode: 400,
          code: "INVALID_ENTITY_ID",
          message: "Invalid entity ID",
        });
      }
    },
  );

  it.each([
    ["", ENTITY_PREFIXES.BID],
    ["   ", ENTITY_PREFIXES.BID],
    [ENTITY_PREFIXES.BID, ENTITY_PREFIXES.BID],
    [`${"0".repeat(25)}, ENTITY_PREFIXES.BID],
    [`${"0".repeat(27)}, ENTITY_PREFIXES.BID],
    [`${"I".repeat(26)}, ENTITY_PREFIXES.BID],
    [`${VALID_ULID}, ENTITY_PREFIXES.BID],
  ])("rejects malformed boundary representation %j for prefix %j deterministically", (value, prefix) => {
    try {
      assertEntityId(prefix, value);
      throw new Error("expected assertEntityId to reject");
    } catch (error) {
      expect(error).toMatchObject({
        name: "BadRequestError",
        status: 400,
        statusCode: 400,
        code: "INVALID_ENTITY_ID",
        message: "Invalid entity ID",
      });
    }
  });

  it("does not mutate a caller-owned string across retry-style validation", () => {
    const bidId = "  ${ENTITY_PREFIXES.BID}${VALID_ULID}  ";
    const original = bidId;

    assertEntityId(ENTITY_PREFIXES.BID, bidId);
    assertEntityId(ENTITY_PREFIXES.BID, bidId);

    expect(bidId).toBe(original);
  });
});
