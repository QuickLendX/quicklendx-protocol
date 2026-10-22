import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { InvoiceCard } from "@/app/components/InvoiceCard";
import {
  INVOICE_STATUSES,
  INVOICE_STATUS_MAP,
  type InvoiceStatus,
} from "@/app/components/invoice-status";
import { computeImpliedReturn } from "@/app/business/invoices/_lib/math";
import BusinessInvoicesPage from "@/app/business/invoices/page";
import BusinessInvoiceDetailPage from "@/app/business/invoices/[id]/page";
import * as endpoints from "@/app/lib/api/endpoints";
import { WalletProvider } from "@/app/lib/wallet/WalletProvider";

const mockNotFound = jest.fn(() => {
  throw new Error("NEXT_NOT_FOUND");
});
const mockReplace = jest.fn();
let mockParams: Record<string, string> = {};
let mockSearchParams = new URLSearchParams();

jest.mock("next/navigation", () => ({
  useParams: () => mockParams,
  useRouter: () => ({
    replace: mockReplace,
    push: jest.fn(),
  }),
  usePathname: () => "/business/invoices",
  useSearchParams: () => mockSearchParams,
  notFound: () => mockNotFound(),
}));

jest.mock("@/app/lib/api/endpoints", () => {
  const actual = jest.requireActual("@/app/lib/api/endpoints");
  return {
    ...actual,
    listInvoices: jest.fn(),
    getInvoice: jest.fn(),
    getRankedBids: jest.fn(),
    listInvoiceDisputes: jest.fn(),
    listSettlements: jest.fn(),
  };
});

const mockedListInvoices = endpoints.listInvoices as jest.MockedFunction<
  typeof endpoints.listInvoices
>;
const mockedGetInvoice = endpoints.getInvoice as jest.MockedFunction<
  typeof endpoints.getInvoice
>;
const mockedGetRankedBids = endpoints.getRankedBids as jest.MockedFunction<
  typeof endpoints.getRankedBids
>;
const mockedListInvoiceDisputes =
  endpoints.listInvoiceDisputes as jest.MockedFunction<
    typeof endpoints.listInvoiceDisputes
  >;
const mockedListSettlements = endpoints.listSettlements as jest.MockedFunction<
  typeof endpoints.listSettlements
>;

const OWNER_ADDRESS =
  "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K";
const OTHER_ADDRESS =
  "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const VALID_HEX_ID =
  "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";

function makeSampleInvoice(
  overrides: Partial<endpoints.Invoice> = {}
): endpoints.Invoice {
  return {
    id: VALID_HEX_ID,
    business: OWNER_ADDRESS,
    amount: "50000000000",
    currency: "USDC",
    due_date: 1778803200,
    status: "Verified",
    description: "Logistics freight invoice #101",
    category: "Services",
    tags: ["freight", "q2"],
    created_at: 1776211200,
    updated_at: 1776297600,
    ...overrides,
  };
}

describe("InvoiceCard & status mapping", () => {
  it.each(INVOICE_STATUSES)(
    "renders exact status label and aria-hidden icon for %s",
    (status: InvoiceStatus) => {
      const invoice = makeSampleInvoice({ status });
      const { container } = render(<InvoiceCard invoice={invoice} />);

      const expectedLabel = INVOICE_STATUS_MAP[status].label;
      expect(screen.getByText(expectedLabel)).toBeInTheDocument();

      const hiddenIcon = container.querySelector('[aria-hidden="true"]');
      expect(hiddenIcon).not.toBeNull();

      expect(container.querySelector("a a, a button")).toBeNull();
    }
  );

  it("renders exact digits for large i128 amount '123456789012345678901' with 7 decimals", () => {
    const invoice = makeSampleInvoice({
      amount: "123456789012345678901",
    });
    const { container } = render(
      <InvoiceCard invoice={invoice} decimals={7} />
    );

    expect(screen.getByText("12345678901234.5678901")).toBeInTheDocument();
    expect(container.querySelector("a a, a button")).toBeNull();
  });
});

describe("computeImpliedReturn", () => {
  it("computes '5.00%' for bid '1000000000' and expected '1050000000'", () => {
    expect(computeImpliedReturn("1000000000", "1050000000")).toBe("5.00%");
  });

  it("returns '—' when bid is '0' (no division by zero)", () => {
    expect(computeImpliedReturn("0", "1050000000")).toBe("—");
  });
});

describe("BusinessInvoicesPage (list view)", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockSearchParams = new URLSearchParams();
  });

  it("calls listInvoices with { business, limit: 20 }, appends on Load more with cursor, hides button when has_more is false, and resets cursor when selecting 'Funded' tab", async () => {
    const user = userEvent.setup();

    const firstInvoice = makeSampleInvoice({
      id: "0xaaaa1111",
      description: "First page invoice",
    });
    const secondInvoice = makeSampleInvoice({
      id: "0xbbbb2222",
      description: "Second page invoice",
    });
    const fundedInvoice = makeSampleInvoice({
      id: "0xcccc3333",
      status: "Funded",
      description: "Funded invoice",
    });

    mockedListInvoices
      .mockResolvedValueOnce({
        data: [firstInvoice],
        next_cursor: "cursor_page_2",
        has_more: true,
      })
      .mockResolvedValueOnce({
        data: [secondInvoice],
        next_cursor: null,
        has_more: false,
      })
      .mockResolvedValueOnce({
        data: [fundedInvoice],
        next_cursor: null,
        has_more: false,
      });

    render(
      <WalletProvider initialState="CONNECTED" initialAddress={OWNER_ADDRESS}>
        <BusinessInvoicesPage />
      </WalletProvider>
    );

    await waitFor(() => {
      expect(mockedListInvoices).toHaveBeenCalledTimes(1);
    });
    expect(mockedListInvoices).toHaveBeenNthCalledWith(1, {
      business: OWNER_ADDRESS,
      limit: 20,
    });

    expect(await screen.findByText("0xaaaa1111")).toBeInTheDocument();

    const loadMoreBtn = screen.getByRole("button", { name: /load more/i });
    expect(loadMoreBtn).toBeInTheDocument();

    await user.click(loadMoreBtn);

    await waitFor(() => {
      expect(mockedListInvoices).toHaveBeenCalledTimes(2);
    });
    expect(mockedListInvoices).toHaveBeenNthCalledWith(2, {
      business: OWNER_ADDRESS,
      limit: 20,
      cursor: "cursor_page_2",
    });

    // Appends second page while keeping first page
    expect(await screen.findByText("0xbbbb2222")).toBeInTheDocument();
    expect(screen.getByText("0xaaaa1111")).toBeInTheDocument();

    // When has_more is false, "Load more" is hidden
    expect(
      screen.queryByRole("button", { name: /load more/i })
    ).not.toBeInTheDocument();

    // Selecting "Funded" tab calls listInvoices with status: "Funded" and resets cursor
    const fundedTab = screen.getByRole("tab", { name: "Funded" });
    await user.click(fundedTab);

    await waitFor(() => {
      expect(mockedListInvoices).toHaveBeenCalledTimes(3);
    });
    expect(mockedListInvoices).toHaveBeenNthCalledWith(3, {
      business: OWNER_ADDRESS,
      limit: 20,
      status: "Funded",
    });
    expect(await screen.findByText("0xcccc3333")).toBeInTheDocument();
  });
});

describe("BusinessInvoiceDetailPage (detail view)", () => {
  beforeEach(() => {
    jest.clearAllMocks();
    mockParams = { id: VALID_HEX_ID };
  });

  it("owner view: renders ranked bids in API order with ranks 1..n, dispute reason, and settlement link to /business/settlements/", async () => {
    mockedGetInvoice.mockResolvedValueOnce(
      makeSampleInvoice({
        id: VALID_HEX_ID,
        business: OWNER_ADDRESS,
        status: "Funded",
      })
    );

    mockedGetRankedBids.mockResolvedValueOnce({
      data: [
        {
          bid_id: "0xbid1",
          invoice_id: VALID_HEX_ID,
          investor: OTHER_ADDRESS,
          bid_amount: "1000000000",
          expected_return: "1050000000",
          timestamp: 1776220000,
          status: "Placed",
          expiration_timestamp: 1777000000,
        },
        {
          bid_id: "0xbid2",
          invoice_id: VALID_HEX_ID,
          investor: OTHER_ADDRESS,
          bid_amount: "1000000000",
          expected_return: "1080000000",
          timestamp: 1776221000,
          status: "Placed",
          expiration_timestamp: 1777000000,
        },
      ],
    });

    mockedListInvoiceDisputes.mockResolvedValueOnce({
      data: [
        {
          id: "0xdispute1",
          invoice_id: VALID_HEX_ID,
          initiator: OTHER_ADDRESS,
          reason: "Goods arrived with damaged packaging",
          status: "UnderReview",
          created_at: 1776250000,
        },
      ],
      next_cursor: null,
      has_more: false,
    });

    mockedListSettlements.mockResolvedValueOnce({
      data: [
        {
          id: "0xsettle999",
          invoice_id: VALID_HEX_ID,
          amount: "50000000000",
          payer: OWNER_ADDRESS,
          recipient: OTHER_ADDRESS,
          timestamp: 1776300000,
          status: "Paid",
        },
      ],
    });

    render(
      <WalletProvider initialState="CONNECTED" initialAddress={OWNER_ADDRESS}>
        <BusinessInvoiceDetailPage />
      </WalletProvider>
    );

    expect(
      await screen.findByText("Goods arrived with damaged packaging")
    ).toBeInTheDocument();

    const rows = screen.getAllByRole("row");
    // Header row + 2 bid rows
    expect(rows).toHaveLength(3);
    expect(rows[1]).toHaveTextContent("#1");
    expect(rows[1]).toHaveTextContent("5.00%");
    expect(rows[2]).toHaveTextContent("#2");
    expect(rows[2]).toHaveTextContent("8.00%");

    const settlementLink = screen.getByRole("link", {
      name: /view settlement receipt/i,
    });
    expect(settlementLink).toHaveAttribute(
      "href",
      "/business/settlements/0xsettle999"
    );
  });

  it("non-owner view: renders ownership message and does not call getRankedBids or listInvoiceDisputes", async () => {
    mockedGetInvoice.mockResolvedValueOnce(
      makeSampleInvoice({
        id: VALID_HEX_ID,
        business: OWNER_ADDRESS,
      })
    );

    render(
      <WalletProvider initialState="CONNECTED" initialAddress={OTHER_ADDRESS}>
        <BusinessInvoiceDetailPage />
      </WalletProvider>
    );

    expect(
      await screen.findByText("This invoice belongs to another business.")
    ).toBeInTheDocument();

    expect(mockedGetRankedBids).not.toHaveBeenCalled();
    expect(mockedListInvoiceDisputes).not.toHaveBeenCalled();
    expect(
      screen.queryByRole("heading", { name: "Bids" })
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("heading", { name: "Disputes" })
    ).not.toBeInTheDocument();
  });

  it("renders 'Invoice not found' when getInvoice rejects with a 404", async () => {
    mockedGetInvoice.mockRejectedValueOnce(
      new endpoints.ApiHttpError("Invoice not found", 404, "INVOICE_NOT_FOUND")
    );

    render(
      <WalletProvider initialState="CONNECTED" initialAddress={OWNER_ADDRESS}>
        <BusinessInvoiceDetailPage />
      </WalletProvider>
    );

    expect(
      await screen.findByRole("heading", { name: "Invoice not found" })
    ).toBeInTheDocument();
  });

  it("triggers notFound() when given a non-hex id", () => {
    const errorSpy = jest.spyOn(console, "error").mockImplementation(() => {});
    mockParams = { id: "INV-8201-not-hex" };

    expect(() =>
      render(
        <WalletProvider initialState="CONNECTED" initialAddress={OWNER_ADDRESS}>
          <BusinessInvoiceDetailPage />
        </WalletProvider>
      )
    ).toThrow("NEXT_NOT_FOUND");

    expect(mockNotFound).toHaveBeenCalled();
    errorSpy.mockRestore();
  });
});
