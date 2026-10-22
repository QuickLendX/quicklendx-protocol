import { toPublicInvoice, ApiInvoice, truncateAddress } from "../view-model";

describe("toPublicInvoice", () => {
  const fixtureWithMetadata: ApiInvoice = {
    id: "0xabcdef1234567890",
    business: "GXXXXX_BUSINESS_ADDR",
    amount: "50000000000",
    currency: "USDC",
    due_date: 1780000000,
    status: "Verified",
    description: "Invoice for Widget Supplies",
    category: "Manufacturing",
    tags: ["tech", "hardware"],
    metadata: {
      customer_name: "Acme Debtor LLC",
      customer_address: "123 Main Street, Metropolis",
      tax_id: "US-99-8877665",
    },
    created_at: 1770000000,
    updated_at: 1770500000,
  };

  it("has exactly the 9 allowed keys", () => {
    const result = toPublicInvoice(fixtureWithMetadata);
    const keys = Object.keys(result).sort();
    const expectedKeys = [
      "amount",
      "category",
      "created_at",
      "currency",
      "description",
      "due_date",
      "id",
      "status",
      "tags",
    ].sort();

    expect(keys).toEqual(expectedKeys);
    expect(keys.length).toBe(9);
  });

  it("JSON.stringify(result) does not contain customer_name or tax_id", () => {
    const result = toPublicInvoice(fixtureWithMetadata);
    const jsonStr = JSON.stringify(result);

    expect(jsonStr).not.toContain("Acme Debtor LLC");
    expect(jsonStr).not.toContain("US-99-8877665");
    expect(jsonStr).not.toContain("123 Main Street");
    expect(jsonStr).not.toContain("customer_name");
    expect(jsonStr).not.toContain("tax_id");
  });

  it("correctly copies values of allowed fields", () => {
    const result = toPublicInvoice(fixtureWithMetadata);
    expect(result.id).toBe("0xabcdef1234567890");
    expect(result.amount).toBe("50000000000");
    expect(result.currency).toBe("USDC");
    expect(result.due_date).toBe(1780000000);
    expect(result.status).toBe("Verified");
    expect(result.description).toBe("Invoice for Widget Supplies");
    expect(result.category).toBe("Manufacturing");
    expect(result.tags).toEqual(["tech", "hardware"]);
    expect(result.created_at).toBe(1770000000);
  });

  it("truncateAddress formats addresses correctly", () => {
    expect(truncateAddress("GBSXABCDEFGH12345678907C5K")).toBe("GBSX…7C5K");
    expect(truncateAddress("SHORT")).toBe("SHORT");
  });
});
