import { test, expect, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const INVOICE_ID =
  "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
const BUSINESS_ADDRESS =
  "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K";
const BEST_INVESTOR = "GBSXABCDEFGH12345678907C5K";
const RUNNER_UP_INVESTOR = "GAXMYZ1234567890ABCDEFGH7C5K";

const INVOICE = {
  id: INVOICE_ID,
  business: BUSINESS_ADDRESS,
  amount: "50000000000",
  currency: "USDC",
  due_date: 1780000000,
  status: "Verified",
  description: "Enterprise software license financing",
  category: "Software",
  tags: ["tech", "enterprise"],
  created_at: 1770000000,
  updated_at: 1770000000,
};

const BEST_BID = {
  bid_id: "bid-best-1",
  invoice_id: INVOICE_ID,
  investor: BEST_INVESTOR,
  bid_amount: "48500000000",
  expected_return: "1500000000",
  timestamp: 1770500000,
  status: "Placed",
  expiration_timestamp: 1780000000,
};

const RANKED_BIDS = {
  data: [
    BEST_BID,
    {
      bid_id: "bid-runner-up-2",
      invoice_id: INVOICE_ID,
      investor: RUNNER_UP_INVESTOR,
      bid_amount: "48000000000",
      expected_return: "2000000000",
      timestamp: 1770400000,
      status: "Placed",
      expiration_timestamp: 1780000000,
    },
  ],
};

const FRESHNESS = {
  lastIndexedLedger: 1042857,
  indexLagSeconds: 45,
  lastUpdatedAt: "2026-03-15T10:22:41Z",
  cursor: "1042857_0",
};

async function expectNoSeriousOrCriticalAxeViolations(
  page: Page
) {
  const results = await new AxeBuilder({ page }).analyze();
  expect(
    results.violations.filter(
      (violation) =>
        violation.impact === "serious" || violation.impact === "critical"
    )
  ).toEqual([]);
}

test.describe("Investor invoice detail E2E", () => {
  test("shows the best bid, ranked order and freshness at mobile and desktop widths", async ({
    page,
  }) => {
    await page.route("**/api/v1/invoices?*", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          data: [INVOICE],
          next_cursor: null,
          has_more: false,
          freshness: FRESHNESS,
        }),
      });
    });

    await page.route(`**/api/v1/invoices/${INVOICE_ID}`, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(INVOICE),
      });
    });

    await page.route(
      `**/api/v1/bids/${INVOICE_ID}/best`,
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({ data: BEST_BID }),
        });
      }
    );

    await page.route(
      `**/api/v1/bids/${INVOICE_ID}/ranked`,
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(RANKED_BIDS),
        });
      }
    );

    // The detail page's freshness banner is part of the opt-in bid-flow UI.
    // The verification command builds with NEXT_PUBLIC_ENABLE_BID_FLOW=true.
    await page.setViewportSize({ width: 375, height: 812 });
    await page.goto("/invest");

    const invoiceCard = page.locator(
      `[data-testid="invoice-card-${INVOICE_ID}"]`
    );
    await expect(invoiceCard).toBeVisible();
    await expect(invoiceCard.getByText("Best Bid:")).toBeVisible();
    await expect(invoiceCard.getByText("48,500,000,000")).toBeVisible();
    await expect(
      page.getByText("Data may be up to 45 s behind the network")
    ).toBeVisible();

    await invoiceCard.getByRole("link", { name: "View Details" }).click();
    await expect(page).toHaveURL(
      new RegExp(`/invest/invoices/${INVOICE_ID}$`)
    );
    await expect(page.locator("h1")).toHaveText(INVOICE_ID);

    const rows = page.locator("tbody tr");
    await expect(rows).toHaveCount(2);
    await expect(rows.nth(0)).toContainText("#1 Best");
    await expect(rows.nth(0)).toContainText("GBSX…7C5K");
    await expect(rows.nth(1)).toContainText("#2");
    await expect(rows.nth(1)).toContainText("GAXM…7C5K");
    await expect(
      page.getByText("Data may be up to 45 s behind the network")
    ).toBeVisible();

    await expectNoSeriousOrCriticalAxeViolations(page);

    await page.setViewportSize({ width: 1280, height: 800 });
    await expect(rows.nth(0)).toContainText("#1 Best");
    await expect(
      page.getByText("Data may be up to 45 s behind the network")
    ).toBeVisible();
    await expectNoSeriousOrCriticalAxeViolations(page);
  });
});
