import {
  createInvoiceSchema,
  parseTokenAmountToUnits,
  MAX_INVOICE_AMOUNT,
} from "../validation";

describe("createInvoiceSchema Unit Tests", () => {
  const fixedNow = new Date("2026-05-01T00:00:00Z");
  const testTokens = [
    {
      code: "USDC",
      address: "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
      decimals: 7,
    },
  ];

  const validTokenAddress = testTokens[0].address;

  it("due date 2026-05-01 is rejected (not in the future), 2026-05-02 is accepted, and 2027-05-02 is rejected (> 365 days)", () => {
    const schema = createInvoiceSchema(fixedNow, testTokens);

    const baseInvoice = {
      currency: validTokenAddress,
      amount: "100.5",
      description: "Standard service invoice",
      category: "Services" as const,
      tags: ["legal", "consulting"],
    };

    // 2026-05-01: not in the future (needs to be >= tomorrow)
    const sameDay = new Date("2026-05-01T00:00:00Z");
    const sameDayResult = schema.safeParse({
      ...baseInvoice,
      dueDate: sameDay,
    });
    expect(sameDayResult.success).toBe(false);

    // 2026-05-02: tomorrow -> accepted
    const tomorrow = new Date("2026-05-02T00:00:00Z");
    const tomorrowResult = schema.safeParse({
      ...baseInvoice,
      dueDate: tomorrow,
    });
    expect(tomorrowResult.success).toBe(true);

    // 2027-05-02: > 365 days -> rejected (2026 is not a leap year, 365 days from May 1 2026 is May 1 2027)
    const nextYear = new Date("2027-05-02T00:00:00Z");
    const nextYearResult = schema.safeParse({
      ...baseInvoice,
      dueDate: nextYear,
    });
    expect(nextYearResult.success).toBe(false);
  });

  it("a description of 'é'.repeat(512) (512 characters, 1024 bytes) is accepted, and 'é'.repeat(512) + 'a' (1025 bytes) is rejected", () => {
    const schema = createInvoiceSchema(fixedNow, testTokens);
    const validDueDate = new Date("2026-05-05T00:00:00Z");

    const baseInvoice = {
      currency: validTokenAddress,
      amount: "50",
      dueDate: validDueDate,
      category: "Services" as const,
      tags: [],
    };

    const desc1024 = "é".repeat(512);
    expect(new TextEncoder().encode(desc1024).length).toBe(1024);
    const result1024 = schema.safeParse({
      ...baseInvoice,
      description: desc1024,
    });
    expect(result1024.success).toBe(true);

    const desc1025 = "é".repeat(512) + "a";
    expect(new TextEncoder().encode(desc1025).length).toBe(1025);
    const result1025 = schema.safeParse({
      ...baseInvoice,
      description: desc1025,
    });
    expect(result1025.success).toBe(false);
  });

  it("11 tags are rejected, and a 51-byte tag is rejected", () => {
    const schema = createInvoiceSchema(fixedNow, testTokens);
    const validDueDate = new Date("2026-05-05T00:00:00Z");

    const baseInvoice = {
      currency: validTokenAddress,
      amount: "50",
      dueDate: validDueDate,
      description: "Tags test invoice",
      category: "Consulting" as const,
    };

    // 10 valid tags accepted
    const tenTags = Array.from({ length: 10 }, (_, i) => `tag-${i}`);
    expect(schema.safeParse({ ...baseInvoice, tags: tenTags }).success).toBe(
      true
    );

    // 11 tags rejected
    const elevenTags = Array.from({ length: 11 }, (_, i) => `tag-${i}`);
    const elevenResult = schema.safeParse({ ...baseInvoice, tags: elevenTags });
    expect(elevenResult.success).toBe(false);

    // 50-byte tag accepted
    const tag50 = "a".repeat(50);
    expect(schema.safeParse({ ...baseInvoice, tags: [tag50] }).success).toBe(
      true
    );

    // 51-byte tag rejected
    const tag51 = "a".repeat(51);
    const tag51Result = schema.safeParse({ ...baseInvoice, tags: [tag51] });
    expect(tag51Result.success).toBe(false);
  });

  it("currency 'USD' is rejected, and a configured C… address is accepted", () => {
    const schema = createInvoiceSchema(fixedNow, testTokens);
    const validDueDate = new Date("2026-05-05T00:00:00Z");

    const baseInvoice = {
      amount: "50",
      dueDate: validDueDate,
      description: "Currency test",
      category: "Logistics" as const,
      tags: [],
    };

    // "USD" rejected (not a C... address)
    const usdResult = schema.safeParse({ ...baseInvoice, currency: "USD" });
    expect(usdResult.success).toBe(false);

    // Configured C... address accepted
    const validResult = schema.safeParse({
      ...baseInvoice,
      currency: validTokenAddress,
    });
    expect(validResult.success).toBe(true);

    // Unconfigured C... address rejected
    const unconfiguredAddress =
      "CBBNKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZAA2KM";
    const unconfiguredResult = schema.safeParse({
      ...baseInvoice,
      currency: unconfiguredAddress,
    });
    expect(unconfiguredResult.success).toBe(false);
  });

  it("amount '0' and '-1' are rejected, and '170141183460469231731687303715884105727' (i128 max, above MAX_INVOICE_AMOUNT) is rejected", () => {
    const schema = createInvoiceSchema(fixedNow, testTokens);
    const validDueDate = new Date("2026-05-05T00:00:00Z");

    const baseInvoice = {
      currency: validTokenAddress,
      dueDate: validDueDate,
      description: "Amount test",
      category: "Goods" as const,
      tags: [],
    };

    // "0" rejected
    const zeroResult = schema.safeParse({ ...baseInvoice, amount: "0" });
    expect(zeroResult.success).toBe(false);

    // "-1" rejected
    const negativeResult = schema.safeParse({ ...baseInvoice, amount: "-1" });
    expect(negativeResult.success).toBe(false);

    // i128 max as string: "170141183460469231731687303715884105727"
    // Since MAX_INVOICE_AMOUNT is i128::MAX / 10_000, i128::MAX is above the ceiling and must be rejected
    const i128Max = "170141183460469231731687303715884105727";
    const overflowResult = schema.safeParse({
      ...baseInvoice,
      amount: i128Max,
    });
    expect(overflowResult.success).toBe(false);

    // Valid positive amount is accepted
    const validAmountResult = schema.safeParse({
      ...baseInvoice,
      amount: "100.5",
    });
    expect(validAmountResult.success).toBe(true);
  });
});
