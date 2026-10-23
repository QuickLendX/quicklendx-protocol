import { test, expect } from "@playwright/test";
import * as fs from "fs";

test.describe("Settlement Receipt Page", () => {
  const settlementId = "0xabc";
  const invoiceId = "0xinv123";
  const ownerAddress = "GBBXKXTNVC3QDNK7LVHRWCNVMCLK4QNKR4IQHZXR4IQHZXR4KZ";

  test("owner sees receipt and Download JSON contains exact schema without fabricated fields", async ({
    page,
  }) => {
    // Inject test wallet into localStorage before navigation
    await page.addInitScript((address) => {
      window.localStorage.setItem("wallet_address", address);
      (
        window as unknown as { __TEST_WALLET_ADDRESS__: string }
      ).__TEST_WALLET_ADDRESS__ = address;
    }, ownerAddress);

    // Mock API routes
    await page.route(/.*\/api\/v1\/settlements\/.*/, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        headers: { "access-control-allow-origin": "*" },
        body: JSON.stringify({
          id: settlementId,
          invoice_id: invoiceId,
          amount: "50000000000",
          payer: "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K",
          recipient: ownerAddress,
          timestamp: 1777390800,
          status: "Paid",
        }),
      });
    });

    await page.route(/.*\/api\/v1\/invoices\/.*/, async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        headers: { "access-control-allow-origin": "*" },
        body: JSON.stringify({
          id: invoiceId,
          business: ownerAddress,
          amount: "50000000000",
          currency: "USDC",
          due_date: 1778860800,
          status: "Paid",
          description: "Professional Services, April 2026",
          category: "Consulting",
          created_at: 1776000000,
          updated_at: 1777390800,
        }),
      });
    });

    await page.goto(`/business/settlements/${settlementId}`);

    // Receipt header and details should render
    await expect(page.locator("h1")).toContainText("Settlement Receipt");
    await expect(page.getByText("✓ Settled")).toBeVisible();
    await expect(
      page.getByText("Professional Services, April 2026")
    ).toBeVisible();
    await expect(page.getByText("50000000000 USDC")).toBeVisible();

    // Trigger download and inspect JSON
    const downloadPromise = page.waitForEvent("download");
    await page
      .getByRole("button", {
        name: `Download settlement receipt ${settlementId} as JSON`,
      })
      .click();
    const download = await downloadPromise;

    const downloadPath = await download.path();
    expect(downloadPath).not.toBeNull();
    const fileContent = fs.readFileSync(downloadPath!, "utf-8");
    const parsed = JSON.parse(fileContent);

    expect(parsed.receipt).toBeDefined();
    expect(parsed.receipt.id).toBe(settlementId);
    expect(parsed.receipt.status).toBe("Paid");
    expect(parsed.receipt.invoice.id).toBe(invoiceId);

    // Verify fabricated fields are NOT present
    const receiptKeys = Object.keys(parsed.receipt);
    expect(receiptKeys).not.toContain("debtor");
    expect(receiptKeys).not.toContain("account_last4");
    expect(receiptKeys).not.toContain("fees");
    expect(receiptKeys).not.toContain("funding");
    expect(receiptKeys).not.toContain("calculation");
    expect(receiptKeys).not.toContain("transfer");
  });
});
