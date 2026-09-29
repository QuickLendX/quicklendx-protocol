import { ENTITY_PREFIXES, assertInvoiceId } from "./entityId";

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