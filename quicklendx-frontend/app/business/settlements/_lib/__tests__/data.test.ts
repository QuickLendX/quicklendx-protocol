import { ZodError } from "zod";
import { apiClient } from "@/app/lib/api-client";
import { listSettlements } from "../data";

jest.mock("@/app/lib/api-client", () => ({
  apiClient: {
    get: jest.fn(),
  },
}));

const mockGet = jest.mocked(apiClient.get);

describe("listSettlements", () => {
  beforeEach(() => {
    mockGet.mockReset();
  });

  it("fetches and parses the settlements response", async () => {
    mockGet.mockResolvedValue({
      data: [
        {
          id: "settlement-1",
          invoice_id: "invoice-1",
          amount: "9007199254740993",
          payer: "GABC",
          recipient: "GXYZ",
          timestamp: 1767225600,
          status: "Paid",
        },
      ],
      freshness: { indexLagSeconds: 12 },
    });

    await expect(listSettlements()).resolves.toEqual({
      data: [
        {
          id: "settlement-1",
          invoice_id: "invoice-1",
          amount: "9007199254740993",
          payer: "GABC",
          recipient: "GXYZ",
          timestamp: 1767225600,
          status: "Paid",
        },
      ],
      freshness: { indexLagSeconds: 12 },
    });
    expect(mockGet).toHaveBeenCalledWith("/v1/settlements");
  });

  it("propagates fetch errors", async () => {
    const error = new Error("network unavailable");
    mockGet.mockRejectedValue(error);

    await expect(listSettlements()).rejects.toBe(error);
  });

  it("rejects malformed settlement responses", async () => {
    mockGet.mockResolvedValue({
      data: [
        {
          id: "settlement-1",
          invoice_id: "invoice-1",
          amount: "not-a-number",
          payer: "GABC",
          recipient: "GXYZ",
          timestamp: -1,
          status: "Unknown",
        },
      ],
      freshness: { indexLagSeconds: -1 },
    });

    await expect(listSettlements()).rejects.toBeInstanceOf(ZodError);
  });
});
