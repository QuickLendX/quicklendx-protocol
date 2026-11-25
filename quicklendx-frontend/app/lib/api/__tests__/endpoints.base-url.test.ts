import {
  getApiBaseUrl,
  fetchInvoices,
  fetchInvoiceById,
  fetchBestBid,
  fetchRankedBids,
} from "../endpoints";

describe("endpoints base URL configuration", () => {
  const originalApiUrl = process.env.NEXT_PUBLIC_API_URL;
  const originalFetch = global.fetch;

  beforeEach(() => {
    delete process.env.NEXT_PUBLIC_API_URL;
  });

  afterEach(() => {
    if (originalApiUrl === undefined) {
      delete process.env.NEXT_PUBLIC_API_URL;
    } else {
      process.env.NEXT_PUBLIC_API_URL = originalApiUrl;
    }
    global.fetch = originalFetch;
    jest.restoreAllMocks();
  });

  describe("getApiBaseUrl()", () => {
    it("defaults to http://localhost:3001/api/v1 when NEXT_PUBLIC_API_URL is unset", () => {
      expect(getApiBaseUrl()).toBe("http://localhost:3001/api/v1");
    });

    it("returns NEXT_PUBLIC_API_URL when set without trailing slash", () => {
      process.env.NEXT_PUBLIC_API_URL = "https://api.quicklendx.io/api/v1";
      expect(getApiBaseUrl()).toBe("https://api.quicklendx.io/api/v1");
    });

    it("trims trailing slashes from NEXT_PUBLIC_API_URL", () => {
      process.env.NEXT_PUBLIC_API_URL = "https://api.quicklendx.io/api/v1///";
      expect(getApiBaseUrl()).toBe("https://api.quicklendx.io/api/v1");
    });
  });

  describe("endpoint fetchers base URL resolution", () => {
    it("calls /api/v1/invoices by default when NEXT_PUBLIC_API_URL is unset", async () => {
      const fetchMock = jest.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: async () => ({
          data: [],
          next_cursor: null,
          has_more: false,
        }),
      });
      global.fetch = fetchMock as unknown as typeof fetch;

      await fetchInvoices({ status: "Verified", limit: 10 });

      expect(fetchMock).toHaveBeenCalledTimes(1);
      const requestedUrl = String(fetchMock.mock.calls[0][0]);
      expect(requestedUrl).toContain("/api/v1/invoices");
      expect(requestedUrl).toBe(
        "http://localhost:3001/api/v1/invoices?status=Verified&limit=10"
      );
    });

    it("honours NEXT_PUBLIC_API_URL override and strips trailing slashes across endpoint helpers", async () => {
      process.env.NEXT_PUBLIC_API_URL = "https://staging.quicklendx.io/api/v1/";

      const fetchMock = jest
        .fn()
        .mockResolvedValueOnce({
          ok: true,
          status: 200,
          json: async () => ({ data: [], next_cursor: null, has_more: false }),
        })
        .mockResolvedValueOnce({
          ok: true,
          status: 200,
          json: async () => ({
            id: "inv_1",
            amount: "1000",
            due_date: 1700000000,
            status: "Verified",
          }),
        })
        .mockResolvedValueOnce({
          ok: false,
          status: 404,
          json: async () => ({}),
        })
        .mockResolvedValueOnce({
          ok: true,
          status: 200,
          json: async () => ({ data: [] }),
        });
      global.fetch = fetchMock as unknown as typeof fetch;

      await fetchInvoices();
      await fetchInvoiceById("inv_1");
      await fetchBestBid("inv_1");
      await fetchRankedBids("inv_1");

      expect(fetchMock.mock.calls[0][0]).toBe(
        "https://staging.quicklendx.io/api/v1/invoices?"
      );
      expect(fetchMock.mock.calls[1][0]).toBe(
        "https://staging.quicklendx.io/api/v1/invoices/inv_1"
      );
      expect(fetchMock.mock.calls[2][0]).toBe(
        "https://staging.quicklendx.io/api/v1/bids/inv_1/best"
      );
      expect(fetchMock.mock.calls[3][0]).toBe(
        "https://staging.quicklendx.io/api/v1/bids/inv_1/ranked"
      );
    });
  });
});
