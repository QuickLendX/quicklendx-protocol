import { ENTITY_PREFIXES, assertBidId, assertInvoiceId } from "./entityId";

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

describe("assertBidId deterministic failure boundaries", () => {
  function expectBidInvalid(value: unknown): void {
    try {
      assertBidId(value);
      throw new Error("expected assertBidId to reject");
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

  it.each([
    `${ENTITY_PREFIXES.BID}${VALID_ULID}`,
    `${ENTITY_PREFIXES.BID}${VALID_ULID.toLowerCase()}`,
    `  ${ENTITY_PREFIXES.BID}${VALID_ULID}  `,
    `\t${ENTITY_PREFIXES.BID}${VALID_ULID}\n`,
    `${ENTITY_PREFIXES.BID}${"0".repeat(26)}`,
  ])("accepts valid bid representation %j repeatedly", (value) => {
    expect(() => assertBidId(value)).not.toThrow();
    expect(() => assertBidId(value)).not.toThrow();
    expect(() => assertBidId(value)).not.toThrow();
  });

  it.each([null, undefined, 0, false, true, NaN, {}, [], () => {}])(
    "rejects non-string input %p with the stable public error",
    (value) => {
      expectBidInvalid(value);
      expectBidInvalid(value);
    },
  );

  it.each([
    "",
    "   ",
    ENTITY_PREFIXES.BID,
    `${ENTITY_PREFIXES.BID}${"0".repeat(25)}`,
    `${ENTITY_PREFIXES.BID}${"0".repeat(27)}`,
    `${ENTITY_PREFIXES.BID}${VALID_ULID}extra`,
    `${ENTITY_PREFIXES.BID}${ENTITY_PREFIXES.BID}${VALID_ULID}`,
    `bid ${VALID_ULID}`,
    `BID_${VALID_ULID}`,
    `bid_ ${VALID_ULID}`,
    `bid_${VALID_ULID.slice(0, 25)}_`,
  ])("rejects malformed boundary representation %j deterministically", (value) => {
    expectBidInvalid(value);
    expectBidInvalid(value);
  });

  it.each(["I", "L", "O", "U"])(
    "rejects the excluded Crockford character %s when repeated to ULID length",
    (char) => {
      const excluded = `${ENTITY_PREFIXES.BID}${char.repeat(26)}`;
      expectBidInvalid(excluded);
      expectBidInvalid(excluded);
    },
  );

  it.each([
    `${ENTITY_PREFIXES.INVOICE}${VALID_ULID}`,
    `${ENTITY_PREFIXES.SETTLEMENT}${VALID_ULID}`,
    `${ENTITY_PREFIXES.EXPORT_TOKEN}${VALID_ULID}`,
    VALID_ULID,
  ])("rejects the wrong-entropy prefix representation %j", (value) => {
    expectBidInvalid(value);
  });

  it.each(["0x", "0x0", "0xdeadBEEF", "  0xdeadBEEF  "])(
    "rejects the invoice-only 0x hex form %j, which is not a bid id",
    (value) => {
      expectBidInvalid(value);
      expectBidInvalid(value);
    },
  );

  it("does not mutate a caller-owned string across retry-style validation", () => {
    const bidId = `  ${ENTITY_PREFIXES.BID}${VALID_ULID}  `;
    const original = bidId;

    assertBidId(bidId);
    assertBidId(bidId);

    expect(bidId).toBe(original);
  });

  it("accepts a boundary-length ULID and rejects one character over deterministically", () => {
    const boundary = `${ENTITY_PREFIXES.BID}${VALID_ULID}`;
    const overByOne = `${ENTITY_PREFIXES.BID}${VALID_ULID}Z`;

    for (let i = 0; i < 3; i++) {
      expect(() => assertBidId(boundary)).not.toThrow();
      expectBidInvalid(overByOne);
    }
  });
});