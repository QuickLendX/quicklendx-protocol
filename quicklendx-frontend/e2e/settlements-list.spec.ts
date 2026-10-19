import AxeBuilder from "@axe-core/playwright";
import { expect, test } from "@playwright/test";
import type { Settlement } from "../app/business/settlements/_lib/data";

const connectedAddress = "GCONNECTEDWALLET";

function fixtureRows(count: number): Settlement[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `receipt-${index + 1}`,
    invoice_id: `invoice-${index + 1}`,
    amount: `${(index + 1) * 100}`,
    payer: connectedAddress,
    recipient: `GRECIPIENT${index}`,
    timestamp: Math.floor(Date.now() / 1000) - index * 60,
    status: "Paid",
  }));
}

async function useWallet(page: import("@playwright/test").Page) {
  await page.addInitScript((address) => {
    window.__QLX_TEST_WALLET__ = address;
  }, connectedAddress);
}

for (const viewport of [
  { label: "mobile-375", width: 375, height: 812 },
  { label: "desktop-1280", width: 1280, height: 900 },
]) {
  test.describe(`${viewport.label} screenshots`, () => {
    test.use({ viewport: { width: viewport.width, height: viewport.height } });

    test("loaded state", async ({ page }, testInfo) => {
      await useWallet(page);
      await page.route("**/api/v1/settlements*", (route) =>
        route.fulfill({
          json: { data: fixtureRows(3), freshness: { indexLagSeconds: 10 } },
        })
      );
      await page.goto("/business/settlements");
      await expect(
        page.getByRole("heading", { name: "Settlement receipts" })
      ).toBeVisible();
      await expect(page.getByTestId("settlement-row")).toHaveCount(3);
      await page.screenshot({
        path: testInfo.outputPath(`loaded-${viewport.label}.png`),
        fullPage: true,
      });
    });

    test("empty state", async ({ page }, testInfo) => {
      await useWallet(page);
      await page.route("**/api/v1/settlements*", (route) =>
        route.fulfill({
          json: { data: [], freshness: { indexLagSeconds: 10 } },
        })
      );
      await page.goto("/business/settlements");
      await expect(page.getByText("No settlement history")).toBeVisible();
      await page.screenshot({
        path: testInfo.outputPath(`empty-${viewport.label}.png`),
        fullPage: true,
      });
    });

    test("error state", async ({ page }, testInfo) => {
      await useWallet(page);
      await page.route("**/api/v1/settlements*", (route) =>
        route.fulfill({ status: 503, json: { error: "unavailable" } })
      );
      await page.goto("/business/settlements");
      await expect(page.getByRole("alert")).toBeVisible();
      await page.screenshot({
        path: testInfo.outputPath(`error-${viewport.label}.png`),
        fullPage: true,
      });
    });
  });
}

test("View opens the receipt and the loaded list passes axe serious/critical checks", async ({
  page,
}) => {
  await useWallet(page);
  await page.route("**/api/v1/settlements*", (route) =>
    route.fulfill({
      json: { data: fixtureRows(1), freshness: { indexLagSeconds: 10 } },
    })
  );
  await page.goto("/business/settlements");
  await page.getByRole("link", { name: "View" }).click();
  await expect(
    page.getByRole("heading", { name: "Settlement Receipt" })
  ).toBeVisible();
  await page.getByRole("link", { name: "Back to settlements" }).click();
  await expect(page).toHaveURL(/\/business\/settlements$/);

  const results = await new AxeBuilder({ page }).analyze();
  expect(
    results.violations.filter((violation) =>
      ["serious", "critical"].includes(violation.impact ?? "")
    )
  ).toEqual([]);
});
