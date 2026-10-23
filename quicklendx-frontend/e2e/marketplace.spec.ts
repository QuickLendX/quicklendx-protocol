import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

test.describe("Investor Marketplace E2E", () => {
  const invoiceFixture = {
    id: "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
    business: "GAXMFSADZVDXTJSLL3HZJFVYH4JGBQBMX3I2WGPKH3YKRXL7ZJXZK23Q",
    amount: "50000000000",
    currency: "USDC",
    due_date: 1780000000,
    status: "Verified",
    description: "Enterprise software license financing",
    category: "Software",
    tags: ["tech", "enterprise"],
    metadata: {
      customer_name: "Acme Debtor LLC",
      customer_address: "123 Debtor Way, Suite 400",
      tax_id: "US-99-8877665",
    },
    created_at: 1770000000,
    updated_at: 1770000000,
  };

  const bestBidFixture = {
    data: {
      bid_id: "bid-best-1",
      invoice_id: invoiceFixture.id,
      investor: "GBSXABCDEFGH12345678907C5K",
      bid_amount: "48500000000",
      expected_return: "1500000000",
      timestamp: 1770500000,
      status: "Placed",
      expiration_timestamp: 1780000000,
    },
  };

  const rankedBidsFixture = {
    data: [
      bestBidFixture.data,
      {
        bid_id: "bid-runner-2",
        invoice_id: invoiceFixture.id,
        investor: "GAXMYZ1234567890ABCDEFGH7C5K",
        bid_amount: "48000000000",
        expected_return: "2000000000",
        timestamp: 1770400000,
        status: "Placed",
        expiration_timestamp: 1780000000,
      },
    ],
  };

  test("browse -> open first invoice -> verify heading id, axe scan 0 violations, no debtor metadata", async ({
    page,
  }) => {
    // Intercept API routes
    await page.route("**/api/invoices?*", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          data: [invoiceFixture],
          next_cursor: null,
          has_more: false,
          freshness: {
            lastIndexedLedger: 1042857,
            indexLagSeconds: 12,
            lastUpdatedAt: "2026-03-15T10:22:41Z",
            cursor: "1042857_0",
          },
        }),
      });
    });

    await page.route(`**/api/bids/${invoiceFixture.id}/best`, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(bestBidFixture),
      });
    });

    await page.route(`**/api/invoices/${invoiceFixture.id}`, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(invoiceFixture),
      });
    });

    await page.route(
      `**/api/bids/${invoiceFixture.id}/ranked`,
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(rankedBidsFixture),
        });
      }
    );

    // 1. Browse marketplace
    await page.goto("/invest");

    // Wait for card to render
    const card = page.locator(
      `[data-testid="invoice-card-${invoiceFixture.id}"]`
    );
    await expect(card).toBeVisible();

    // Assert page text never contains debtor metadata
    const marketplaceBody = await page.textContent("body");
    expect(marketplaceBody).not.toContain("Acme Debtor LLC");
    expect(marketplaceBody).not.toContain("US-99-8877665");
    expect(marketplaceBody).not.toContain("123 Debtor Way");

    // Run axe scan on marketplace
    const marketplaceAxeResults = await new AxeBuilder({ page }).analyze();
    const seriousOrCriticalMarketplace =
      marketplaceAxeResults.violations.filter(
        (v) => v.impact === "serious" || v.impact === "critical"
      );
    expect(seriousOrCriticalMarketplace).toEqual([]);

    // 2. Open the first invoice
    await card.getByRole("link", { name: "View Details" }).click();

    // Verify detail URL
    await expect(page).toHaveURL(
      new RegExp(`/invest/invoices/${invoiceFixture.id}`)
    );

    // The detail heading shows the same id
    const heading = page.locator("h1");
    await expect(heading).toHaveText(invoiceFixture.id);

    // Ranked bids table visible
    await expect(page.locator("table")).toBeVisible();
    await expect(page.getByText("GBSX…7C5K")).toBeVisible();

    // Assert detail page text never contains debtor metadata
    const detailBody = await page.textContent("body");
    expect(detailBody).not.toContain("Acme Debtor LLC");
    expect(detailBody).not.toContain("US-99-8877665");
    expect(detailBody).not.toContain("123 Debtor Way");

    // Run axe scan on detail page
    const detailAxeResults = await new AxeBuilder({ page }).analyze();
    const seriousOrCriticalDetail = detailAxeResults.violations.filter(
      (v) => v.impact === "serious" || v.impact === "critical"
    );
    expect(seriousOrCriticalDetail).toEqual([]);
  });
});
