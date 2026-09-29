import {
  hexStringSchema,
  stellarAddressSchema,
  positiveAmountSchema,
  paginationSchema,
  getInvoicesQuerySchema,
  invoiceIdParamSchema,
  getBidsQuerySchema,
  getSettlementsQuerySchema,
  businessFilterSchema,
} from "../src/validators/shared";

describe("Shared Validators", () => {
  describe("hexStringSchema", () => {
    it("should accept valid hex strings", () => {
      const result = hexStringSchema.safeParse("0xabcdef1234567890");
      expect(result.success).toBe(true);
    });

    it("should reject non-hex characters", () => {
      const result = hexStringSchema.safeParse("0xxyz123");
      expect(result.success).toBe(false);
    });

    it("should reject strings without 0x prefix", () => {
      const result = hexStringSchema.safeParse("abcdef1234567890");
      expect(result.success).toBe(false);
    });
  });

  describe("stellarAddressSchema", () => {
    it("should accept valid Stellar public keys", () => {
      const result = stellarAddressSchema.safeParse(
        "GDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B"
      );
      expect(result.success).toBe(true);
    });

    it("should reject addresses not starting with G", () => {
      const result = stellarAddressSchema.safeParse(
        "XDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B"
      );
      expect(result.success).toBe(false);
    });

    it("should reject addresses with special characters", () => {
      const result = stellarAddressSchema.safeParse("G$pecial@Address12345678901234567890123");
      expect(result.success).toBe(false);
    });

    it("should reject short addresses", () => {
      const result = stellarAddressSchema.safeParse("G1234");
      expect(result.success).toBe(false);
    });
  });

  describe("positiveAmountSchema", () => {
    it("should accept positive numbers as strings", () => {
      const result = positiveAmountSchema.safeParse("1000000");
      expect(result.success).toBe(true);
    });

    it("should reject non-numeric strings", () => {
      const result = positiveAmountSchema.safeParse("abc");
      expect(result.success).toBe(false);
    });
  });

  describe("paginationSchema", () => {
    it("should accept valid pagination params", () => {
      const result = paginationSchema.safeParse({ page: 1, limit: 20 });
      expect(result.success).toBe(true);
    });

    it("should apply defaults", () => {
      const result = paginationSchema.safeParse({});
      if (result.success) {
        expect(result.data.page).toBe(1);
        expect(result.data.limit).toBe(20);
      }
    });
  });

  describe("getInvoicesQuerySchema", () => {
    it("should accept valid query params", () => {
      const result = getInvoicesQuerySchema.safeParse({
        business: "GDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B",
        status: "Pending",
        page: "1",
        limit: "20",
      });
      expect(result.success).toBe(true);
    });

    it("should accept query with no params", () => {
      const result = getInvoicesQuerySchema.safeParse({});
      expect(result.success).toBe(true);
    });
  });

  describe("invoiceIdParamSchema", () => {
    it("should accept valid hex invoice ID", () => {
      const result = invoiceIdParamSchema.safeParse({
        id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
      });
      expect(result.success).toBe(true);
    });

    it("should reject invalid ID", () => {
      const result = invoiceIdParamSchema.safeParse({ id: "invalid" });
      expect(result.success).toBe(false);
    });
  });

  describe("getBidsQuerySchema", () => {
    it("should accept valid bid query params", () => {
      const result = getBidsQuerySchema.safeParse({
        invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        investor: "GDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B",
        page: "1",
        limit: "20",
      });
      expect(result.success).toBe(true);
    });

    it("should accept partial params", () => {
      const result = getBidsQuerySchema.safeParse({
        invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
      });
      expect(result.success).toBe(true);
    });
  });

  describe("getSettlementsQuerySchema", () => {
    it("should accept valid settlement query params", () => {
      const result = getSettlementsQuerySchema.safeParse({
        invoice_id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
        page: "1",
        limit: "20",
      });
      expect(result.success).toBe(true);
    });
  });

  describe("businessFilterSchema", () => {
    it("should accept valid Stellar address", () => {
      const result = businessFilterSchema.safeParse({
        business: "GDRXE2BQUC3AZNPVFSJEZIXZZDZSMTLBVWN4HZ5SAPHP2R3C3YHS6M2B",
      });
      expect(result.success).toBe(true);
    });

    it("should reject invalid Stellar address", () => {
      const result = businessFilterSchema.safeParse({
        business: "INVALID_ADDRESS",
      });
      expect(result.success).toBe(false);
    });
  });
});
