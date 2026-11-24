/**
 * Integration tests for the place-bid flow on the invoice detail view (#3176):
 * open the form, place a bid, and see the ranked list and freshness banner update.
 *
 * Run with: npx jest app/invest/_components/__tests__/InvoiceDetailView.bid-flow.test.tsx
 */

import React from "react";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@testing-library/jest-dom";
import { InvoiceDetailView } from "../InvoiceDetailView";
import * as endpoints from "../../../lib/api/endpoints";
import { NetworkError } from "@/app/lib/errors";

jest.mock("../../../lib/api/endpoints");

const ADDRESS = "GINVESTOR1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ2345";
const NOW_S = Math.floor(Date.now() / 1000);

// A real hex id: the form refuses ids the bid API would reject.
const invoice = {
  id: "0xdeadbeef1234",
  business: "GBUSINESS",
  amount: "15000000000",
  currency: "USDC",
  due_date: NOW_S + 90 * 86_400,
  status: "Verified" as const,
  description: "Hardware invoice",
  category: "Hardware",
  tags: [],
  created_at: NOW_S - 1000,
  updated_at: NOW_S - 1000,
};

// Existing bid: profit 500
const existingBid = {
  bid_id: "0xexisting",
  invoice_id: invoice.id,
  investor: "GBSXABCDEFGH12345678907C5K",
  bid_amount: "1000",
  expected_return: "1500",
  timestamp: NOW_S - 1000,
  status: "Placed" as const,
  expiration_timestamp: NOW_S + 86_400,
};

// The bid the test places: profit 200, so it ranks second
const newBid = {
  bid_id: "0xnewbid",
  invoice_id: invoice.id,
  investor: ADDRESS,
  bid_amount: "1000",
  expected_return: "1200",
  timestamp: NOW_S,
  status: "Placed" as const,
  expiration_timestamp: NOW_S + 86_400,
};

const freshness = (indexLagSeconds: number) => ({
  lastIndexedLedger: 500,
  indexLagSeconds,
  lastUpdatedAt: new Date().toISOString(),
  cursor: "c",
});

const mocked = endpoints as jest.Mocked<typeof endpoints>;

function renderView(bidFlowEnabled = true) {
  return render(
    <InvoiceDetailView
      params={Promise.resolve({ id: invoice.id })}
      bidFlowEnabled={bidFlowEnabled}
    />
  );
}

async function openForm(user: ReturnType<typeof userEvent.setup>) {
  await screen.findByText(invoice.id);
  await user.click(screen.getByRole("button", { name: /place a bid/i }));
  return screen.findByRole("region", { name: /place a bid/i });
}

async function fillAndSubmit(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByLabelText(/^bid amount/i), "1000");
  await user.type(screen.getByLabelText(/^expected return/i), "1200");
  await user.click(screen.getByRole("button", { name: /submit bid/i }));
}

/** The bidder column of the ranked table, top to bottom. */
function rankedBidders(): string[] {
  const table = screen.getByRole("table");
  return within(table)
    .getAllByRole("row")
    .slice(1)
    .map((row) => row.textContent ?? "");
}

describe("InvoiceDetailView place-bid flow", () => {
  beforeEach(() => {
    jest.resetAllMocks();
    window.__QUICKLENDX_TEST_WALLET__ = {
      state: "CONNECTED",
      address: ADDRESS,
    };
    mocked.fetchInvoiceById.mockResolvedValue(invoice);
    mocked.fetchRankedBids.mockResolvedValue([existingBid]);
    mocked.fetchInvoices.mockResolvedValue({
      data: [],
      next_cursor: null,
      has_more: false,
      freshness: freshness(2),
    });
    mocked.placeBid.mockResolvedValue(newBid);
  });

  afterEach(() => {
    delete window.__QUICKLENDX_TEST_WALLET__;
  });

  describe("opening the form", () => {
    it("is hidden until the Place a bid button is pressed, and toggles", async () => {
      const user = userEvent.setup();
      renderView();
      await screen.findByText(invoice.id);

      const toggle = screen.getByRole("button", { name: /place a bid/i });
      expect(toggle).toHaveAttribute("aria-expanded", "false");
      expect(
        screen.queryByRole("region", { name: /place a bid/i })
      ).not.toBeInTheDocument();

      await user.click(toggle);
      expect(toggle).toHaveAttribute("aria-expanded", "true");
      expect(
        await screen.findByRole("region", { name: /place a bid/i })
      ).toBeInTheDocument();

      await user.click(toggle);
      expect(
        screen.queryByRole("region", { name: /place a bid/i })
      ).not.toBeInTheDocument();
    });

    it("closes from the form's own Close button", async () => {
      const user = userEvent.setup();
      renderView();
      await openForm(user);
      await user.click(screen.getByRole("button", { name: /^close$/i }));
      expect(
        screen.queryByRole("region", { name: /place a bid/i })
      ).not.toBeInTheDocument();
    });

    it("passes the invoice's own id, amount ceiling and currency to the form", async () => {
      const user = userEvent.setup();
      renderView();
      await openForm(user);

      expect(screen.getByLabelText(/bid amount \(usdc\)/i)).toBeInTheDocument();
      expect(
        screen.getByText(/at most the invoice amount \(15,000,000,000\)/i)
      ).toBeInTheDocument();
    });

    it("does not exist when the flow is off, and does not fetch freshness", async () => {
      renderView(false);
      await screen.findByText(invoice.id);

      expect(
        screen.queryByRole("button", { name: /place a bid/i })
      ).not.toBeInTheDocument();
      expect(mocked.fetchInvoices).not.toHaveBeenCalled();
      expect(screen.queryByText(/data as of/i)).not.toBeInTheDocument();
    });
  });

  describe("placing a bid", () => {
    it("posts the exact payload", async () => {
      const user = userEvent.setup();
      renderView();
      await openForm(user);
      await fillAndSubmit(user);

      await waitFor(() => expect(mocked.placeBid).toHaveBeenCalledTimes(1));
      const payload = mocked.placeBid.mock.calls[0][0];
      expect(Object.keys(payload).sort()).toEqual([
        "bid_amount",
        "expected_return",
        "expiration_timestamp",
        "invoice_id",
      ]);
      expect(payload).toMatchObject({
        invoice_id: invoice.id,
        bid_amount: "1000",
        expected_return: "1200",
      });
      expect(typeof payload.expiration_timestamp).toBe("number");
      // default expiry: about a day from now
      expect(payload.expiration_timestamp).toBeGreaterThan(
        NOW_S + 86_400 - 120
      );
      expect(payload.expiration_timestamp).toBeLessThan(NOW_S + 86_400 + 120);
    });

    it("shows the new bid in its ranked position and updates the count immediately", async () => {
      const user = userEvent.setup();
      // Server list never changes: the bid on screen has to come from the optimistic update
      renderView();
      await openForm(user);
      expect(screen.getByText("1 bid")).toBeInTheDocument();

      await fillAndSubmit(user);

      await screen.findByText("2 bids");
      const rows = rankedBidders();
      expect(rows).toHaveLength(2);
      expect(rows[0]).toContain("GBSX…7C5K"); // profit 500
      expect(rows[1]).toContain("GINV…2345"); // profit 200
    });

    it("then refreshes the ranked bids from the server", async () => {
      const user = userEvent.setup();
      renderView();
      await openForm(user);
      expect(mocked.fetchRankedBids).toHaveBeenCalledTimes(1);

      await fillAndSubmit(user);

      await waitFor(() =>
        expect(mocked.fetchRankedBids).toHaveBeenCalledTimes(2)
      );
      expect(mocked.fetchRankedBids).toHaveBeenLastCalledWith(invoice.id);
    });

    it("adopts the server's ranking when the refresh contains the new bid, without duplicating it", async () => {
      const user = userEvent.setup();
      const rival = {
        ...existingBid,
        bid_id: "0xrival",
        investor: "GRIVALAAAA1111BBBB2222",
        expected_return: "1900",
      };
      mocked.fetchRankedBids
        .mockResolvedValueOnce([existingBid])
        .mockResolvedValue([rival, existingBid, newBid]);
      renderView();
      await openForm(user);
      await fillAndSubmit(user);

      await screen.findByText("3 bids");
      const rows = rankedBidders();
      expect(rows).toHaveLength(3);
      expect(rows[0]).toContain("GRIV…2222");
      expect(rows.filter((r) => r.includes("GINV…2345"))).toHaveLength(1);
    });

    it("keeps the new bid on screen when the refresh trails the write", async () => {
      const user = userEvent.setup();
      // The refresh answers without the new bid (replication lag)
      mocked.fetchRankedBids.mockResolvedValue([existingBid]);
      renderView();
      await openForm(user);
      await fillAndSubmit(user);

      await waitFor(() =>
        expect(mocked.fetchRankedBids).toHaveBeenCalledTimes(2)
      );
      await screen.findByText("2 bids");
      expect(rankedBidders().join(" ")).toContain("GINV…2345");
    });

    it("keeps the list when the refresh itself fails", async () => {
      const user = userEvent.setup();
      mocked.fetchRankedBids
        .mockResolvedValueOnce([existingBid])
        .mockRejectedValue(new Error("boom"));
      renderView();
      await openForm(user);
      await fillAndSubmit(user);

      await waitFor(() =>
        expect(mocked.fetchRankedBids).toHaveBeenCalledTimes(2)
      );
      expect(await screen.findByText("2 bids")).toBeInTheDocument();
    });

    it("shows a success message and leaves the form usable", async () => {
      const user = userEvent.setup();
      renderView();
      await openForm(user);
      await fillAndSubmit(user);

      expect(await screen.findByText(/bid placed/i)).toBeInTheDocument();
    });

    it("then blocks a second bid, because the investor now has an active one", async () => {
      const user = userEvent.setup();
      renderView();
      await openForm(user);
      await fillAndSubmit(user);

      await screen.findByText("2 bids");
      expect(
        await screen.findByText(/already have an active bid/i)
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /submit bid/i })
      ).toBeDisabled();
    });
  });

  describe("freshness", () => {
    it("shows the banner once the freshness is known", async () => {
      renderView();
      await screen.findByText(invoice.id);
      expect(await screen.findByText(/data as of/i)).toBeInTheDocument();
      expect(mocked.fetchInvoices).toHaveBeenCalledWith({ limit: 1 });
    });

    it("refreshes the banner after a bid", async () => {
      const user = userEvent.setup();
      mocked.fetchInvoices
        .mockResolvedValueOnce({
          data: [],
          next_cursor: null,
          has_more: false,
          freshness: freshness(2),
        })
        .mockResolvedValue({
          data: [],
          next_cursor: null,
          has_more: false,
          freshness: freshness(45),
        });
      renderView();
      await openForm(user);
      expect(screen.queryByText(/up to 45 s behind/i)).not.toBeInTheDocument();

      await fillAndSubmit(user);

      expect(await screen.findByText(/up to 45 s behind/i)).toBeInTheDocument();
      expect(mocked.fetchInvoices).toHaveBeenCalledTimes(2);
    });

    it("pauses bidding and says so when the data is more than two minutes behind", async () => {
      const user = userEvent.setup();
      mocked.fetchInvoices.mockResolvedValue({
        data: [],
        next_cursor: null,
        has_more: false,
        freshness: freshness(300),
      });
      renderView();
      await openForm(user);

      expect(await screen.findAllByText(/bidding is paused/i)).not.toHaveLength(
        0
      );
      expect(
        screen.getByRole("button", { name: /submit bid/i })
      ).toBeDisabled();
      expect(screen.getByLabelText(/^bid amount/i)).toBeDisabled();
    });

    it("shows no banner, and does not pause bidding, when freshness cannot be loaded", async () => {
      const user = userEvent.setup();
      mocked.fetchInvoices.mockRejectedValue(new Error("down"));
      renderView();
      await openForm(user);

      expect(screen.queryByText(/data as of/i)).not.toBeInTheDocument();
      expect(screen.getByLabelText(/^bid amount/i)).toBeEnabled();
    });

    it("tolerates an invoice list response with no freshness", async () => {
      mocked.fetchInvoices.mockResolvedValue({
        data: [],
        next_cursor: null,
        has_more: false,
      });
      renderView();
      await screen.findByText(invoice.id);
      expect(screen.queryByText(/data as of/i)).not.toBeInTheDocument();
    });
  });

  describe("when the outcome of a bid is unknown", () => {
    it("refreshes the ranked bids so the investor can see whether it landed", async () => {
      const user = userEvent.setup();
      mocked.placeBid.mockRejectedValue(
        new NetworkError(
          "Request timed out. Please try again.",
          { action: "POST", additionalData: { timeout: 10000 } },
          0
        )
      );
      mocked.fetchRankedBids
        .mockResolvedValueOnce([existingBid])
        .mockResolvedValue([existingBid, newBid]);
      renderView();
      await openForm(user);
      await fillAndSubmit(user);

      expect(
        await screen.findByText(/check the ranked bids/i)
      ).toBeInTheDocument();
      // The bid did land: the refresh shows it
      await screen.findByText("2 bids");
      expect(mocked.placeBid).toHaveBeenCalledTimes(1);
    });
  });

  describe("a definite refusal", () => {
    it("leaves the list alone and does not refresh it", async () => {
      const user = userEvent.setup();
      const { AuthenticationError } = jest.requireActual("@/app/lib/errors");
      mocked.placeBid.mockRejectedValue(
        new AuthenticationError(
          "Authentication required",
          { action: "POST" },
          "UNAUTHORIZED"
        )
      );
      renderView();
      await openForm(user);
      await fillAndSubmit(user);

      expect(
        await screen.findByText(/not signed in for bidding/i)
      ).toBeInTheDocument();
      expect(screen.getByText("1 bid")).toBeInTheDocument();
      expect(mocked.fetchRankedBids).toHaveBeenCalledTimes(1);
    });
  });

  describe("without a connected wallet", () => {
    it("opens the form but nothing can be submitted", async () => {
      const user = userEvent.setup();
      window.__QUICKLENDX_TEST_WALLET__ = { state: "NOT_CONNECTED" };
      renderView();
      await openForm(user);

      expect(
        screen.getByText(/connect your wallet to place a bid/i)
      ).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: /submit bid/i })
      ).toBeDisabled();
      expect(mocked.placeBid).not.toHaveBeenCalled();
    });
  });
});
