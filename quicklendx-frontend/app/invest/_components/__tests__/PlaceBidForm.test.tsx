/**
 * Tests for PlaceBidForm (#3176).
 *
 * Run with: npx jest app/invest/_components/__tests__/PlaceBidForm.test.tsx
 */

import React from "react";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@testing-library/jest-dom";
import { PlaceBidForm, type PlaceBidFormProps } from "../PlaceBidForm";
import { WalletContext } from "@/app/lib/wallet/WalletProvider";
import {
  AuthenticationError,
  AuthorizationError,
  NetworkError,
  SystemError,
  ValidationError,
} from "@/app/lib/errors";
import type { Bid, FreshnessMetadata } from "../../_lib/view-model";

// A minute-aligned instant, so the default expiry (now + 24h) survives the
// datetime-local input's minute resolution exactly.
const NOW_MS = 1_770_000_000_000;
const NOW_S = NOW_MS / 1000;
const ADDRESS = "GINVESTOR1234567890ABCDEFGHIJKLMNOPQRSTUVWXYZ2345";

const existingBids: Bid[] = [
  {
    bid_id: "0xa",
    invoice_id: "0xabc123",
    investor: "GOTHERAAAA1111",
    bid_amount: "1000",
    expected_return: "1300",
    timestamp: NOW_S - 500,
    status: "Placed",
    expiration_timestamp: NOW_S + 86_400,
  },
  {
    bid_id: "0xb",
    invoice_id: "0xabc123",
    investor: "GOTHERBBBB2222",
    bid_amount: "1000",
    expected_return: "1100",
    timestamp: NOW_S - 400,
    status: "Placed",
    expiration_timestamp: NOW_S + 86_400,
  },
];

const placedBid: Bid = {
  bid_id: "0xnew",
  invoice_id: "0xabc123",
  investor: ADDRESS,
  bid_amount: "1000",
  expected_return: "1200",
  timestamp: NOW_S,
  status: "Placed",
  expiration_timestamp: NOW_S + 86_400,
};

function freshnessWithLag(indexLagSeconds: number): FreshnessMetadata {
  return {
    lastIndexedLedger: 100,
    indexLagSeconds,
    lastUpdatedAt: new Date(NOW_MS).toISOString(),
    cursor: "c",
  };
}

type Wallet = { state: "CONNECTED" | "NOT_CONNECTED"; address?: string };

function renderForm(
  overrides: Partial<PlaceBidFormProps> = {},
  wallet: Wallet = { state: "CONNECTED", address: ADDRESS }
) {
  const props: PlaceBidFormProps = {
    invoiceId: "0xabc123",
    invoiceAmount: "1000000",
    currency: "USDC",
    invoiceStatus: "Verified",
    rankedBids: existingBids,
    onBidPlaced: jest.fn(),
    submitBid: jest.fn().mockResolvedValue(placedBid),
    nowMs: () => NOW_MS,
    ...overrides,
  };
  const utils = render(
    <WalletContext.Provider value={wallet}>
      <PlaceBidForm {...props} />
    </WalletContext.Provider>
  );
  return { ...utils, props };
}

const amountInput = () => screen.getByLabelText(/^bid amount/i);
const returnInput = () => screen.getByLabelText(/^expected return/i);
const expiryInput = () => screen.getByLabelText(/^bid expires/i);
const submitButton = () =>
  screen.getByRole("button", { name: /submit bid|submitting/i });

async function fill(
  user: ReturnType<typeof userEvent.setup>,
  amount: string,
  ret: string
) {
  await user.clear(amountInput());
  if (amount) await user.type(amountInput(), amount);
  await user.clear(returnInput());
  if (ret) await user.type(returnInput(), ret);
}

describe("PlaceBidForm", () => {
  describe("when the wallet is not connected", () => {
    it("disables every field and the submit button, and says why", () => {
      renderForm({}, { state: "NOT_CONNECTED" });

      expect(screen.getByRole("status")).toHaveTextContent(
        /connect your wallet to place a bid/i
      );
      expect(amountInput()).toBeDisabled();
      expect(returnInput()).toBeDisabled();
      expect(expiryInput()).toBeDisabled();
      expect(submitButton()).toBeDisabled();
    });

    it("never submits, even when the form is submitted programmatically", () => {
      const { props, container } = renderForm({}, { state: "NOT_CONNECTED" });
      fireEvent.submit(container.querySelector("form")!);
      expect(props.submitBid).not.toHaveBeenCalled();
    });

    it("treats a connected state with no address as not connected", () => {
      renderForm({}, { state: "CONNECTED", address: undefined });
      expect(submitButton()).toBeDisabled();
      expect(screen.getByRole("status")).toHaveTextContent(
        /connect your wallet/i
      );
    });

    // An address alone is not "connected": the wallet may be mid-connect, on the
    // wrong network, or errored. The fields (not just the button, which is off for
    // an empty form anyway) must be locked and the investor told why.
    it.each(["CONNECTING", "WRONG_NETWORK", "ERROR", "DISCONNECTING"] as const)(
      "locks the form in the %s wallet state even though an address is known",
      (state) => {
        render(
          <WalletContext.Provider value={{ state, address: ADDRESS }}>
            <PlaceBidForm
              invoiceId="0xabc123"
              invoiceAmount="1000000"
              currency="USDC"
              invoiceStatus="Verified"
              rankedBids={[]}
              onBidPlaced={jest.fn()}
              nowMs={() => NOW_MS}
            />
          </WalletContext.Provider>
        );
        expect(amountInput()).toBeDisabled();
        expect(returnInput()).toBeDisabled();
        expect(screen.getByRole("status")).toHaveTextContent(
          /connect your wallet/i
        );
        expect(screen.queryByText(/bidding as/i)).not.toBeInTheDocument();
      }
    );
  });

  describe("when the wallet is connected", () => {
    it("enables the fields, shows who is bidding, and keeps submit off until the form is valid", () => {
      renderForm();

      expect(amountInput()).toBeEnabled();
      expect(screen.getByText(/bidding as/i)).toBeInTheDocument();
      expect(screen.getByText("GINV…2345")).toBeInTheDocument();
      expect(screen.queryByText(ADDRESS)).not.toBeInTheDocument();
      expect(submitButton()).toBeDisabled();
    });

    it("defaults the expiry to 24 hours out", () => {
      renderForm();
      expect(expiryInput()).toHaveValue(
        // YYYY-MM-DDTHH:mm of NOW + 86400, local time
        new Date((NOW_S + 86_400) * 1000)
          .toLocaleString("sv-SE", { hour12: false })
          .slice(0, 16)
          .replace(" ", "T")
      );
    });
  });

  describe("submitting a valid bid", () => {
    it("posts exactly the four API fields", async () => {
      const user = userEvent.setup();
      const { props } = renderForm();

      await fill(user, "1000", "1200");
      await user.click(submitButton());

      await waitFor(() => expect(props.submitBid).toHaveBeenCalledTimes(1));
      expect(props.submitBid).toHaveBeenCalledWith({
        invoice_id: "0xabc123",
        bid_amount: "1000",
        expected_return: "1200",
        expiration_timestamp: NOW_S + 86_400,
      });
    });

    it("sends a different expiry when the picker is changed", async () => {
      const user = userEvent.setup();
      const { props } = renderForm();

      await fill(user, "500", "600");
      fireEvent.change(expiryInput(), {
        target: {
          value: new Date((NOW_S + 3 * 86_400) * 1000)
            .toLocaleString("sv-SE", { hour12: false })
            .slice(0, 16)
            .replace(" ", "T"),
        },
      });
      await user.click(submitButton());

      await waitFor(() => expect(props.submitBid).toHaveBeenCalled());
      expect(props.submitBid).toHaveBeenCalledWith(
        expect.objectContaining({ expiration_timestamp: NOW_S + 3 * 86_400 })
      );
    });

    it("hands the server's bid to onBidPlaced and confirms", async () => {
      const user = userEvent.setup();
      const { props } = renderForm();

      await fill(user, "1000", "1200");
      await user.click(submitButton());

      await waitFor(() =>
        expect(props.onBidPlaced).toHaveBeenCalledWith(placedBid)
      );
      expect(await screen.findByRole("status")).toHaveTextContent(
        /bid placed/i
      );
    });

    it("clears the amounts after a success so the same bid is not re-sent by accident", async () => {
      const user = userEvent.setup();
      renderForm();

      await fill(user, "1000", "1200");
      await user.click(submitButton());
      await screen.findByText(/bid placed/i);

      expect(amountInput()).toHaveValue("");
      expect(returnInput()).toHaveValue("");
      expect(submitButton()).toBeDisabled();
    });

    it("shows a busy state while the request is in flight and ignores a second click", async () => {
      const user = userEvent.setup();
      let resolve!: (bid: Bid) => void;
      const pending = new Promise<Bid>((r) => {
        resolve = r;
      });
      const { props } = renderForm({
        submitBid: jest.fn().mockReturnValue(pending),
      });

      await fill(user, "1000", "1200");
      await user.click(submitButton());

      expect(
        await screen.findByRole("button", { name: /submitting/i })
      ).toBeDisabled();
      expect(amountInput()).toBeDisabled();

      // A double click, and Enter in a field, while the first request is pending.
      fireEvent.click(screen.getByRole("button", { name: /submitting/i }));
      fireEvent.submit(amountInput().closest("form")!);
      expect(props.submitBid).toHaveBeenCalledTimes(1);

      await act(async () => {
        resolve(placedBid);
      });
      await screen.findByText(/bid placed/i);
      expect(props.submitBid).toHaveBeenCalledTimes(1);
    });
  });

  describe("field validation", () => {
    it("shows no errors on a pristine form", () => {
      renderForm();
      expect(screen.queryByText(/enter a bid amount/i)).not.toBeInTheDocument();
    });

    it("explains a bad amount once the field has been left, and marks it invalid", async () => {
      const user = userEvent.setup();
      renderForm();

      await user.type(amountInput(), "10.5");
      await user.tab();

      expect(
        await screen.findByText(/whole numbers only/i)
      ).toBeInTheDocument();
      expect(amountInput()).toHaveAttribute("aria-invalid", "true");
      expect(amountInput()).toHaveAccessibleDescription(/whole numbers only/i);
    });

    it("refuses a bid above the invoice amount", async () => {
      const user = userEvent.setup();
      const { props } = renderForm();

      await fill(user, "1000001", "1000002");
      await user.tab();

      expect(
        await screen.findByText(/cannot exceed the invoice amount/i)
      ).toBeInTheDocument();
      expect(submitButton()).toBeDisabled();
      expect(props.submitBid).not.toHaveBeenCalled();
    });

    it("refuses an expected return below the bid", async () => {
      const user = userEvent.setup();
      renderForm();

      await fill(user, "1000", "900");
      await user.tab();

      expect(
        await screen.findByText(/at least the bid amount/i)
      ).toBeInTheDocument();
      expect(submitButton()).toBeDisabled();
    });

    it("refuses an expiry in the past", async () => {
      const user = userEvent.setup();
      renderForm();

      await fill(user, "1000", "1200");
      fireEvent.change(expiryInput(), {
        target: { value: "2020-01-01T00:00" },
      });
      fireEvent.blur(expiryInput());

      expect(
        await screen.findByText(/at least a minute from now/i)
      ).toBeInTheDocument();
      expect(submitButton()).toBeDisabled();
    });

    it("clears an error as soon as the value is fixed", async () => {
      const user = userEvent.setup();
      renderForm();

      await user.type(amountInput(), "abc");
      await user.tab();
      await screen.findByText(/whole numbers only/i);

      await user.clear(amountInput());
      await user.type(amountInput(), "100");
      await waitFor(() =>
        expect(
          screen.queryByText(/whole numbers only/i)
        ).not.toBeInTheDocument()
      );
    });
  });

  describe("preview", () => {
    it("shows the implied return from math.ts as the bid is typed", async () => {
      const user = userEvent.setup();
      renderForm();

      expect(screen.queryByTestId("implied-return")).not.toBeInTheDocument();

      await fill(user, "1000", "1050");
      // (1050 - 1000) * 10000 / 1000 = 500 bps = 5.00%
      expect(await screen.findByTestId("implied-return")).toHaveTextContent(
        "5.00%"
      );

      await user.clear(returnInput());
      await user.type(returnInput(), "1200");
      expect(screen.getByTestId("implied-return")).toHaveTextContent("20.00%");
    });

    it("previews where the bid would rank among the existing bids", async () => {
      const user = userEvent.setup();
      renderForm(); // existing profits: 300 and 100

      await fill(user, "1000", "1500"); // profit 500: first
      expect(await screen.findByTestId("rank-preview")).toHaveTextContent(
        "Would rank #1 of 3"
      );

      await user.clear(returnInput());
      await user.type(returnInput(), "1200"); // profit 200: between
      expect(screen.getByTestId("rank-preview")).toHaveTextContent(
        "Would rank #2 of 3"
      );

      await user.clear(returnInput());
      await user.type(returnInput(), "1050"); // profit 50: last
      expect(screen.getByTestId("rank-preview")).toHaveTextContent(
        "Would rank #3 of 3"
      );
    });

    it("shows no preview until both amounts are usable", async () => {
      const user = userEvent.setup();
      renderForm();

      await user.type(amountInput(), "1000");
      expect(
        screen.queryByRole("group", { name: /bid preview/i })
      ).not.toBeInTheDocument();

      await user.type(returnInput(), "900"); // below the bid: not a usable pair
      expect(
        screen.queryByRole("group", { name: /bid preview/i })
      ).not.toBeInTheDocument();
    });
  });

  describe("investor limit", () => {
    // 1,000,000 cap, 900,000 in use: 100,000 of headroom (micro-USD, par currency)
    const limit = { capUsd: "1000000", currentExposureUsd: "900000" };

    it("blocks a bid over the limit, explains it, and never submits", async () => {
      const user = userEvent.setup();
      const { props } = renderForm({ investorLimit: limit });

      await fill(user, "100001", "110000");

      const alert = await screen.findByRole("alert");
      expect(alert).toHaveTextContent(/over your exposure limit/i);
      expect(alert).toHaveTextContent("1 USD"); // limit 1,000,000 micro-USD = 1 USD
      expect(alert).toHaveTextContent("0.9 USD in use");
      expect(alert).toHaveTextContent("0.1 USD remains");
      expect(submitButton()).toBeDisabled();

      fireEvent.submit(amountInput().closest("form")!);
      expect(props.submitBid).not.toHaveBeenCalled();
    });

    it("allows a bid that lands exactly on the limit", async () => {
      const user = userEvent.setup();
      renderForm({ investorLimit: limit });

      await fill(user, "100000", "110000");

      expect(submitButton()).toBeEnabled();
      expect(
        screen.queryByText(/over your exposure limit/i)
      ).not.toBeInTheDocument();
    });

    it("allows the bid again once it is lowered under the limit", async () => {
      const user = userEvent.setup();
      renderForm({ investorLimit: limit });

      await fill(user, "100001", "110000");
      expect(submitButton()).toBeDisabled();

      await user.clear(amountInput());
      await user.type(amountInput(), "50000");
      await waitFor(() => expect(submitButton()).toBeEnabled());
    });

    it("does not block when the cap is 0 (disabled)", async () => {
      const user = userEvent.setup();
      renderForm({ investorLimit: { capUsd: "0", currentExposureUsd: "999" } });

      await fill(user, "500000", "600000");
      expect(submitButton()).toBeEnabled();
    });

    it("leaves a non-USD currency to the server", async () => {
      const user = userEvent.setup();
      renderForm({
        currency: "XLM",
        investorLimit: { capUsd: "1", currentExposureUsd: "0" },
      });

      await fill(user, "500000", "600000");
      expect(submitButton()).toBeEnabled();
    });

    it("does nothing when no limit is known", async () => {
      const user = userEvent.setup();
      renderForm({ investorLimit: null });

      await fill(user, "999999", "999999");
      expect(submitButton()).toBeEnabled();
    });
  });

  describe("other things that stop a bid", () => {
    it("disables the form when the investor already has an active bid", () => {
      renderForm({
        rankedBids: [...existingBids, { ...placedBid, bid_id: "0xmine" }],
      });
      expect(screen.getByRole("status")).toHaveTextContent(
        /already have an active bid/i
      );
      expect(amountInput()).toBeDisabled();
      expect(submitButton()).toBeDisabled();
    });

    it("does not count a withdrawn bid of the investor's own", () => {
      renderForm({
        rankedBids: [
          ...existingBids,
          { ...placedBid, bid_id: "0xold", status: "Withdrawn" },
        ],
      });
      expect(amountInput()).toBeEnabled();
    });

    it.each(["Pending", "Funded", "Paid", "Defaulted", "Cancelled"] as const)(
      "is closed for an invoice that is %s",
      (invoiceStatus) => {
        renderForm({ invoiceStatus });
        expect(screen.getByRole("status")).toHaveTextContent(
          /not open for bids/i
        );
        expect(submitButton()).toBeDisabled();
      }
    );

    it("pauses bidding when the indexed data is more than 120 seconds behind", () => {
      renderForm({ freshness: freshnessWithLag(121) });
      expect(screen.getByRole("status")).toHaveTextContent(
        /paused while the data catches up/i
      );
      expect(submitButton()).toBeDisabled();
    });

    it("allows bidding at exactly 120 seconds of lag (the boundary is inclusive)", async () => {
      const user = userEvent.setup();
      renderForm({ freshness: freshnessWithLag(120) });
      await fill(user, "1000", "1200");
      expect(submitButton()).toBeEnabled();
    });

    it("allows bidding when no freshness data is available", async () => {
      const user = userEvent.setup();
      renderForm({ freshness: null });
      await fill(user, "1000", "1200");
      expect(submitButton()).toBeEnabled();
    });
  });

  describe("failures from the server", () => {
    const ctx = (additionalData?: Record<string, unknown>) => ({
      action: "POST",
      additionalData: { url: "/bids", ...additionalData },
    });

    async function submitWith(
      error: unknown,
      extra: Partial<PlaceBidFormProps> = {}
    ) {
      const user = userEvent.setup();
      const utils = renderForm({
        submitBid: jest.fn().mockRejectedValue(error),
        ...extra,
      });
      await fill(user, "1000", "1200");
      await user.click(submitButton());
      await waitFor(() => expect(utils.props.submitBid).toHaveBeenCalled());
      return { user, ...utils };
    }

    it("401: tells the investor to sign in, keeps their input, and does not refresh the list", async () => {
      const onAmbiguousFailure = jest.fn();
      await submitWith(new AuthenticationError("x", ctx(), "UNAUTHORIZED"), {
        onAmbiguousFailure,
      });

      expect(await screen.findByRole("alert")).toHaveTextContent(
        /not signed in for bidding/i
      );
      expect(amountInput()).toHaveValue("1000");
      expect(returnInput()).toHaveValue("1200");
      expect(onAmbiguousFailure).not.toHaveBeenCalled();
    });

    it("401: explains a rejected signature", async () => {
      await submitWith(
        new AuthenticationError("x", ctx(), "INVALID_SIGNATURE")
      );
      expect(await screen.findByRole("alert")).toHaveTextContent(/signature/i);
    });

    it("403: explains the refusal", async () => {
      await submitWith(new AuthorizationError("x", ctx(), "FORBIDDEN"));
      expect(await screen.findByRole("alert")).toHaveTextContent(
        /not permitted/i
      );
    });

    it("429 exposure cap: explains the limit, remembers it, and refuses the next bid locally", async () => {
      const onLimitLearned = jest.fn();
      const details = {
        currentExposureUsd: "900000",
        attemptedUsd: "200000",
        capUsd: "1000000",
      };
      const { user, props } = await submitWith(
        new NetworkError(
          "cap",
          ctx({ retryAfter: undefined, details }),
          0,
          "EXPOSURE_CAP_EXCEEDED"
        ),
        { onLimitLearned }
      );

      expect(await screen.findByRole("alert")).toHaveTextContent(
        /over your exposure limit/i
      );
      expect(onLimitLearned).toHaveBeenCalledWith({
        capUsd: "1000000",
        currentExposureUsd: "900000",
      });

      // The next over-limit attempt is stopped before any request is sent.
      await user.clear(amountInput());
      await user.type(amountInput(), "150000");
      await user.clear(returnInput());
      await user.type(returnInput(), "160000");
      expect(submitButton()).toBeDisabled();
      expect(props.submitBid).toHaveBeenCalledTimes(1);

      // ...but a bid inside the remembered headroom goes through.
      await user.clear(amountInput());
      await user.type(amountInput(), "100000");
      await user.clear(returnInput());
      await user.type(returnInput(), "110000");
      expect(submitButton()).toBeEnabled();
    });

    describe("429 rate limit", () => {
      beforeEach(() => jest.useFakeTimers());
      afterEach(() => jest.useRealTimers());

      it("waits out Retry-After before the button comes back", async () => {
        const submitBid = jest
          .fn()
          .mockRejectedValue(
            new NetworkError(
              "slow down",
              ctx({ retryAfter: "5" }),
              0,
              "RATE_LIMITED"
            )
          );
        renderForm({ submitBid });

        fireEvent.change(amountInput(), { target: { value: "1000" } });
        fireEvent.change(returnInput(), { target: { value: "1200" } });
        await act(async () => {
          fireEvent.click(submitButton());
        });

        expect(screen.getByRole("alert")).toHaveTextContent(
          /try again in 5 seconds/i
        );
        expect(submitButton()).toBeDisabled();

        act(() => {
          jest.advanceTimersByTime(4_999);
        });
        expect(submitButton()).toBeDisabled();

        act(() => {
          jest.advanceTimersByTime(2);
        });
        expect(submitButton()).toBeEnabled();
        expect(submitBid).toHaveBeenCalledTimes(1);
      });
    });

    it("400: shows the server's reason for a duplicate bid", async () => {
      await submitWith(
        new ValidationError(
          "Investor G... already has an active bid on this invoice",
          ctx(),
          "INVALID_BID"
        )
      );
      expect(await screen.findByRole("alert")).toHaveTextContent(
        /already have an active bid/i
      );
    });

    it("400: attaches a server field complaint to the field and clears it on edit", async () => {
      const { user } = await submitWith(
        new ValidationError(
          "Expected return must be greater than or equal to bid amount",
          ctx(),
          "INVALID_BID"
        )
      );

      expect(
        await screen.findAllByText(/greater than or equal to bid amount/i)
      ).not.toHaveLength(0);
      expect(returnInput()).toHaveAttribute("aria-invalid", "true");

      await user.type(returnInput(), "0");
      await waitFor(() =>
        expect(returnInput()).not.toHaveAttribute("aria-invalid")
      );
    });

    it("400: maps schema issues onto the fields", async () => {
      await submitWith(
        new ValidationError(
          "Validation failed",
          ctx({
            details: [
              {
                field: "bid_amount",
                message: "Must be a positive numeric value as string",
                code: "x",
              },
            ],
          }),
          "VALIDATION_ERROR"
        )
      );
      expect(
        await screen.findByText(/highlighted fields/i)
      ).toBeInTheDocument();
      expect(
        screen.getByText(/positive numeric value as string/i)
      ).toBeInTheDocument();
      expect(amountInput()).toHaveAttribute("aria-invalid", "true");
    });

    it.each([
      [
        "a timeout",
        new NetworkError(
          "Request timed out. Please try again.",
          ctx({ timeout: 10000 }),
          0
        ),
      ],
      [
        "a dropped connection",
        new NetworkError("Network connection failed.", ctx()),
      ],
      ["a 500", new SystemError("Server error", ctx({ status: 500 }))],
      [
        "an unreadable success reply",
        new SystemError(
          "The bid may have been placed, but the server's reply was not understood.",
          ctx(),
          "BID_RESPONSE_INVALID"
        ),
      ],
    ])(
      "%s: says the outcome is unknown and asks the page to refresh the bids",
      async (_name, error) => {
        const onAmbiguousFailure = jest.fn();
        await submitWith(error, { onAmbiguousFailure });

        const alert = await screen.findByRole("alert");
        expect(alert.textContent).toMatch(
          /check the ranked bids|may have been placed/i
        );
        expect(onAmbiguousFailure).toHaveBeenCalledTimes(1);
        // Never offered as a plain "try again": the investor must look first.
        expect(alert).not.toHaveTextContent(/^try again/i);
      }
    );

    it("does not send the request a second time on its own after a failure", async () => {
      const { props } = await submitWith(
        new NetworkError("Network connection failed.", ctx())
      );
      await screen.findByRole("alert");
      await act(async () => {
        await new Promise((r) => setTimeout(r, 50));
      });
      expect(props.submitBid).toHaveBeenCalledTimes(1);
    });

    it("lets the investor try again after a definite refusal", async () => {
      const submitBid = jest
        .fn()
        .mockRejectedValueOnce(
          new ValidationError("Invoice not found: 0xabc", ctx(), "INVALID_BID")
        )
        .mockResolvedValueOnce(placedBid);
      const user = userEvent.setup();
      const { props } = renderForm({ submitBid });

      await fill(user, "1000", "1200");
      await user.click(submitButton());
      await screen.findByRole("alert");

      await user.click(submitButton());
      await waitFor(() =>
        expect(props.onBidPlaced).toHaveBeenCalledWith(placedBid)
      );
      expect(submitBid).toHaveBeenCalledTimes(2);
    });
  });

  describe("an invoice id the API would refuse", () => {
    it("says so and does not submit, instead of sending a request that is bound to fail", async () => {
      const user = userEvent.setup();
      const { props } = renderForm({ invoiceId: "0xdetail123456" }); // not hex

      await fill(user, "1000", "1200");

      expect(await screen.findByRole("alert")).toHaveTextContent(
        /id the bid api does not accept/i
      );
      expect(submitButton()).toBeDisabled();
      expect(props.submitBid).not.toHaveBeenCalled();
    });
  });

  describe("accessibility", () => {
    it("is a labelled region with a heading and labelled inputs", () => {
      renderForm();
      expect(
        screen.getByRole("region", { name: /place a bid/i })
      ).toBeInTheDocument();
      expect(amountInput()).toHaveAccessibleName(/bid amount/i);
      expect(returnInput()).toHaveAccessibleName(/expected return/i);
      expect(expiryInput()).toHaveAccessibleName(/bid expires/i);
    });

    it("offers a close control when given onCancel", async () => {
      const user = userEvent.setup();
      const onCancel = jest.fn();
      renderForm({ onCancel });
      await user.click(screen.getByRole("button", { name: /close/i }));
      expect(onCancel).toHaveBeenCalled();
    });
  });
});
