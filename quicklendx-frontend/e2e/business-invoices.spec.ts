import { test, expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const OWNER_ADDRESS =
  "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K";
const OTHER_ADDRESS =
  "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
const INVOICE_ID_1 =
  "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
const INVOICE_ID_2 =
  "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";

const SAMPLE_INVOICE_1 = {
  id: INVOICE_ID_1,
  business: OWNER_ADDRESS,
  amount: "50000000000",
  currency: "USDC",
  due_date: 1778803200,
  status: "Verified",
  description: "Logistics freight invoice — Q2 shipment",
  category: "Services",
  tags: ["freight", "priority"],
  created_at: 1776211200,
  updated_at: 1776297600,
};

const SAMPLE_INVOICE_2 = {
  id: INVOICE_ID_2,
  business: OWNER_ADDRESS,
  amount: "125000000000",
  currency: "USDC",
  due_date: 1781395200,
  status: "Funded",
  description: "Warehouse equipment procurement",
  category: "Manufacturing",
  tags: ["equipment"],
  created_at: 1776124800,
  updated_at: 1776211200,
};

const SAMPLE_BIDS = {
  data: [
    {
      bid_id: "0xbid001",
      invoice_id: INVOICE_ID_1,
      investor: OTHER_ADDRESS,
      bid_amount: "48000000000",
      expected_return: "50000000000",
      timestamp: 1776220000,
      status: "Placed",
      expiration_timestamp: 1777500000,
    },
    {
      bid_id: "0xbid002",
      invoice_id: INVOICE_ID_1,
      investor: OTHER_ADDRESS,
      bid_amount: "47500000000",
      expected_return: "50000000000",
      timestamp: 1776225000,
      status: "Placed",
      expiration_timestamp: 1777500000,
    },
  ],
};

const SAMPLE_DISPUTES = {
  data: [
    {
      id: "0xdisp001",
      invoice_id: INVOICE_ID_1,
      initiator: OTHER_ADDRESS,
      reason: "Delivery schedule variance under review",
      status: "UnderReview",
      created_at: 1776250000,
    },
  ],
  next_cursor: null,
  has_more: false,
};

const SAMPLE_SETTLEMENTS = {
  data: [
    {
      id: "0xsettle001",
      invoice_id: INVOICE_ID_1,
      amount: "50000000000",
      payer: OWNER_ADDRESS,
      recipient: OTHER_ADDRESS,
      timestamp: 1776300000,
      status: "Paid",
    },
  ],
};

test.describe("Business Invoices E2E", () => {
  test("list → click first card → detail shows the same id and passes axe scan at 375px and 1280px", async ({
    page,
  }) => {
    await page.addInitScript((address: string) => {
      window.__QUICKLENDX_TEST_WALLET__ = {
        state: "CONNECTED",
        address,
      };
    }, OWNER_ADDRESS);

    await page.route("**/api/v1/invoices?**", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          data: [SAMPLE_INVOICE_1, SAMPLE_INVOICE_2],
          next_cursor: null,
          has_more: false,
        }),
      });
    });

    await page.route(`**/api/v1/invoices/${INVOICE_ID_1}`, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(SAMPLE_INVOICE_1),
      });
    });

    await page.route(`**/api/v1/bids/${INVOICE_ID_1}/ranked`, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(SAMPLE_BIDS),
      });
    });

    await page.route(
      `**/api/v1/invoices/${INVOICE_ID_1}/disputes`,
      async (route) => {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify(SAMPLE_DISPUTES),
        });
      }
    );

    await page.route("**/api/v1/settlements?**", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify(SAMPLE_SETTLEMENTS),
      });
    });

    // 1. Desktop (1280px) list view + axe scan
    await page.setViewportSize({ width: 1280, height: 800 });
    await page.goto("/business/invoices");

    const firstCard = page.locator(
      `a[href="/business/invoices/${INVOICE_ID_1}"]`
    );
    await expect(firstCard).toBeVisible();
    await expect(firstCard).toContainText(INVOICE_ID_1);

    const listDesktopAxe = await new AxeBuilder({ page }).analyze();
    const listDesktopSeriousOrCritical = listDesktopAxe.violations.filter(
      (v) => v.impact === "serious" || v.impact === "critical"
    );
    expect(listDesktopSeriousOrCritical).toEqual([]);

    // 2. Mobile (375px) list view + axe scan
    await page.setViewportSize({ width: 375, height: 812 });
    await expect(firstCard).toBeVisible();
    const listMobileAxe = await new AxeBuilder({ page }).analyze();
    const listMobileSeriousOrCritical = listMobileAxe.violations.filter(
      (v) => v.impact === "serious" || v.impact === "critical"
    );
    expect(listMobileSeriousOrCritical).toEqual([]);

    // 3. Click first card → navigate to detail page
    await firstCard.click();
    await expect(page).toHaveURL(
      new RegExp(`/business/invoices/${INVOICE_ID_1}$`)
    );
    await expect(page.locator("h1")).toHaveText(INVOICE_ID_1);
    await expect(
      page.getByText("Delivery schedule variance under review")
    ).toBeVisible();

    // 4. Mobile (375px) detail axe scan
    const detailMobileAxe = await new AxeBuilder({ page }).analyze();
    const detailMobileSeriousOrCritical = detailMobileAxe.violations.filter(
      (v) => v.impact === "serious" || v.impact === "critical"
    );
    expect(detailMobileSeriousOrCritical).toEqual([]);

    // 5. Desktop (1280px) detail axe scan
    await page.setViewportSize({ width: 1280, height: 800 });
    await expect(page.locator("h1")).toHaveText(INVOICE_ID_1);
    const detailDesktopAxe = await new AxeBuilder({ page }).analyze();
    const detailDesktopSeriousOrCritical = detailDesktopAxe.violations.filter(
      (v) => v.impact === "serious" || v.impact === "critical"
    );
    expect(detailDesktopSeriousOrCritical).toEqual([]);
  });
});
