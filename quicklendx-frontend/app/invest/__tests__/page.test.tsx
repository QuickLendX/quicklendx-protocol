import React from "react";
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@testing-library/jest-dom";
import InvestMarketplacePage from "../page";
import * as endpoints from "../../lib/api/endpoints";

jest.mock("../../lib/api/endpoints");

const mockReplace = jest.fn();
jest.mock("next/navigation", () => ({
  useRouter: () => ({
    replace: mockReplace,
  }),
  useSearchParams: () => new URLSearchParams(),
}));

// Mock IntersectionObserver
type ObserverCallback = (entries: IntersectionObserverEntry[]) => void;
let observerInstances: Array<{
  callback: ObserverCallback;
  elements: Element[];
}> = [];

class MockIntersectionObserver {
  callback: ObserverCallback;
  elements: Element[] = [];

  constructor(callback: ObserverCallback) {
    this.callback = callback;
    observerInstances.push(this);
  }

  observe(element: Element) {
    this.elements.push(element);
  }

  unobserve(element: Element) {
    this.elements = this.elements.filter((el) => el !== element);
  }

  disconnect() {
    this.elements = [];
  }
}

(global as any).IntersectionObserver = MockIntersectionObserver;

describe("/invest Marketplace Page", () => {
  const fixtureWithMetadata = [
    {
      id: "0xinv_1",
      business: "GBUSINESS_1",
      amount: "5000000000",
      currency: "USDC",
      due_date: Math.floor(Date.now() / 1000) + 5 * 86400, // 5 days from now
      status: "Verified" as const,
      description: "Invoice 1 for services",
      category: "Software",
      tags: ["tech"],
      metadata: {
        customer_name: "Acme Debtor LLC",
        customer_address: "123 Debtor Lane",
        tax_id: "US-TAX-12345",
      },
      created_at: 1770000000,
      updated_at: 1770000000,
    },
    {
      id: "0xinv_2",
      business: "GBUSINESS_2",
      amount: "12000000000",
      currency: "USDC",
      due_date: Math.floor(Date.now() / 1000) + 15 * 86400, // 15 days from now
      status: "Verified" as const,
      description: "Invoice 2 for hardware",
      category: "Hardware",
      tags: ["tech"],
      metadata: {
        customer_name: "Acme Debtor LLC",
        customer_address: "123 Debtor Lane",
        tax_id: "US-TAX-12345",
      },
      created_at: 1770000000,
      updated_at: 1770000000,
    },
  ];

  beforeEach(() => {
    jest.clearAllMocks();
    observerInstances = [];
    (endpoints.fetchInvoices as jest.Mock).mockResolvedValue({
      data: fixtureWithMetadata,
      next_cursor: null,
      has_more: false,
      freshness: {
        lastIndexedLedger: 1000,
        indexLagSeconds: 15,
        lastUpdatedAt: "2026-03-15T12:00:00Z",
        cursor: "cur_1",
      },
    });
    (endpoints.fetchBestBid as jest.Mock).mockImplementation(
      (invoiceId: string) => {
        if (invoiceId === "0xinv_1") {
          return Promise.resolve({
            bid_id: "bid_1",
            invoice_id: "0xinv_1",
            investor: "GBSXABCDEFGH12345678907C5K",
            bid_amount: "4800000000",
            expected_return: "200000000",
            timestamp: 1770000000,
            status: "Placed",
            expiration_timestamp: 1780000000,
          });
        }
        return Promise.resolve(null); // 404 / no bids
      }
    );
  });

  it("renders cards from fixture and document.body contains neither Acme Debtor LLC nor tax id", async () => {
    render(<InvestMarketplacePage />);

    await waitFor(() => {
      expect(screen.getByText("0xinv_1")).toBeInTheDocument();
      expect(screen.getByText("0xinv_2")).toBeInTheDocument();
    });

    expect(document.body.textContent).not.toContain("Acme Debtor LLC");
    expect(document.body.textContent).not.toContain("US-TAX-12345");
    expect(document.body.textContent).not.toContain("123 Debtor Lane");
  });

  it("lazy loads best bids only when card intersects, 404 renders 'No bids yet', second intersection does not refetch", async () => {
    render(<InvestMarketplacePage />);

    await waitFor(() => {
      expect(screen.getByText("0xinv_1")).toBeInTheDocument();
    });

    // Initially fetchBestBid has not been called before intersection
    expect(endpoints.fetchBestBid).not.toHaveBeenCalled();

    // Trigger intersection on 0xinv_1
    act(() => {
      observerInstances.forEach((inst) => {
        if (inst.elements.length > 0) {
          inst.callback([
            {
              target: inst.elements[0],
              isIntersecting: true,
            } as unknown as IntersectionObserverEntry,
          ]);
        }
      });
    });

    await waitFor(() => {
      expect(endpoints.fetchBestBid).toHaveBeenCalledWith("0xinv_1");
      expect(screen.getByText("4,800,000,000")).toBeInTheDocument();
    });

    // Trigger intersection on 0xinv_2 (which returns null -> 404)
    act(() => {
      if (observerInstances[1] && observerInstances[1].elements.length > 0) {
        observerInstances[1].callback([
          {
            target: observerInstances[1].elements[0],
            isIntersecting: true,
          } as unknown as IntersectionObserverEntry,
        ]);
      }
    });

    await waitFor(() => {
      expect(endpoints.fetchBestBid).toHaveBeenCalledWith("0xinv_2");
      expect(screen.getByText("No bids yet")).toBeInTheDocument();
    });

    // Second intersection of card 0xinv_1 does not refetch
    const callCountBefore = (endpoints.fetchBestBid as jest.Mock).mock.calls
      .length;
    act(() => {
      if (observerInstances[0] && observerInstances[0].elements.length > 0) {
        observerInstances[0].callback([
          {
            target: observerInstances[0].elements[0],
            isIntersecting: true,
          } as unknown as IntersectionObserverEntry,
        ]);
      }
    });

    expect((endpoints.fetchBestBid as jest.Mock).mock.calls.length).toBe(
      callCountBefore
    );
  });

  it("filters 'Due within 30 days' plus amount >= 10000000000 narrow 10-item fixture and set ?due=30&min=10000000000", async () => {
    // 10-item fixture
    const tenItemsFixture = Array.from({ length: 10 }, (_, i) => ({
      id: `0xitem_${i}`,
      business: `GBUSINESS_${i}`,
      // Items 0..4 have amount 5000000000 (5B), items 5..9 have amount 15000000000 (15B)
      amount: i < 5 ? "5000000000" : "15000000000",
      currency: "USDC",
      // Items with even index due in 10 days (within 30 days), odd index due in 60 days
      due_date: Math.floor(Date.now() / 1000) + (i % 2 === 0 ? 10 : 60) * 86400,
      status: "Verified" as const,
      description: `Invoice item ${i}`,
      category: "Tech",
      tags: [],
      metadata: {
        customer_name: "Acme Debtor LLC",
        customer_address: "123 Debtor Lane",
        tax_id: "US-TAX-12345",
      },
      created_at: 1770000000,
      updated_at: 1770000000,
    }));

    (endpoints.fetchInvoices as jest.Mock).mockResolvedValue({
      data: tenItemsFixture,
      next_cursor: null,
      has_more: false,
    });

    const user = userEvent.setup();
    render(<InvestMarketplacePage />);

    await waitFor(() => {
      expect(screen.getByText("0xitem_0")).toBeInTheDocument();
      expect(screen.getByText("0xitem_9")).toBeInTheDocument();
    });

    // Select Due within 30 days
    const dueSelect = screen.getByLabelText(/due within/i);
    await user.selectOptions(dueSelect, "30");

    // Enter min amount 10000000000
    const minInput = screen.getByLabelText(/min amount/i);
    await user.type(minInput, "10000000000");

    // Check URL update
    expect(mockReplace).toHaveBeenCalledWith(
      expect.stringContaining("due=30"),
      expect.anything()
    );
    expect(mockReplace).toHaveBeenCalledWith(
      expect.stringContaining("min=10000000000"),
      expect.anything()
    );

    // Items >= 10B are 5, 6, 7, 8, 9.
    // Of those, even indices are 6 and 8 (due in 10 days <= 30 days).
    // Odd indices 5, 7, 9 are due in 60 days (> 30 days).
    expect(screen.getByText("0xitem_6")).toBeInTheDocument();
    expect(screen.getByText("0xitem_8")).toBeInTheDocument();

    expect(screen.queryByText("0xitem_0")).not.toBeInTheDocument();
    expect(screen.queryByText("0xitem_5")).not.toBeInTheDocument();
    expect(screen.queryByText("0xitem_7")).not.toBeInTheDocument();
    expect(screen.queryByText("0xitem_9")).not.toBeInTheDocument();
  });
});
