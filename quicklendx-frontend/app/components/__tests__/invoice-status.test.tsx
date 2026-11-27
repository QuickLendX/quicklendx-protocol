/**
 * Invoice status rendering tests — Issue #3147
 *
 * `InvoiceCard` and `invoice-status.ts` drive the marketplace and business
 * invoice lists, so the status badge text, the decorative status icon and the
 * detail link target are all part of the card's contract.
 *
 * Covered here:
 * 1. `INVOICE_STATUSES` is exhaustive and duplicate-free.
 * 2. Every status renders its mapped badge label.
 * 3. The status icon is hidden from assistive tech (`aria-hidden`), so the
 *    badge's accessible name is exactly the label text.
 * 4. Every status renders a link to the invoice detail route, with the id
 *    percent-encoded.
 */

import React from "react";
import { render, screen, within } from "@testing-library/react";
import { InvoiceCard, StatusIcon } from "../InvoiceCard";
import {
  INVOICE_STATUSES,
  INVOICE_STATUS_MAP,
  getInvoiceStatusMeta,
  type InvoiceStatus,
} from "../invoice-status";
import { toPublicInvoice } from "../../invest/_lib/view-model";

const INVOICE_ID = "0xabc123";

function makeInvoice(
  status: InvoiceStatus,
  overrides: Record<string, unknown> = {}
) {
  return toPublicInvoice({
    id: INVOICE_ID,
    business: "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K",
    amount: "50000000000",
    currency: "USDC",
    due_date: 1778803200,
    status,
    description: "Freight shipment invoice #42",
    category: "Logistics",
    tags: ["freight"],
    created_at: 1776211200,
    updated_at: 1776297600,
    ...overrides,
  });
}

describe("INVOICE_STATUSES", () => {
  it("lists every status in INVOICE_STATUS_MAP exactly once", () => {
    expect(INVOICE_STATUSES).toHaveLength(
      Object.keys(INVOICE_STATUS_MAP).length
    );
    expect(new Set(INVOICE_STATUSES).size).toBe(INVOICE_STATUSES.length);
    expect([...INVOICE_STATUSES].sort()).toEqual(
      Object.keys(INVOICE_STATUS_MAP).sort()
    );
  });

  it("gives every status a non-empty, unique label", () => {
    const labels = INVOICE_STATUSES.map(
      (status) => getInvoiceStatusMeta(status).label
    );

    expect(labels.every((label) => label.trim().length > 0)).toBe(true);
    expect(new Set(labels).size).toBe(labels.length);
  });
});

describe("InvoiceCard status badge", () => {
  it.each(INVOICE_STATUSES)(
    "renders the %s label and a decorative icon",
    (status: InvoiceStatus) => {
      render(<InvoiceCard invoice={makeInvoice(status)} />);

      const meta = getInvoiceStatusMeta(status);
      const label = screen.getByText(meta.label);
      expect(label).toBeVisible();

      // The icon is `aria-hidden`, so it contributes no accessible name: the
      // badge is announced as its label text only.
      const badge = label.closest("span");
      expect(badge).not.toBeNull();
      const icon = badge!.querySelector("svg");
      expect(icon).not.toBeNull();
      expect(icon).toHaveAttribute("aria-hidden", "true");
      expect(icon).not.toHaveAttribute("aria-label");
      expect(icon).not.toHaveAttribute("role", "img");
      expect(within(badge as HTMLElement).getByText(meta.label)).toBe(label);

      // No status icon leaks into the accessibility tree as an image.
      expect(screen.queryByRole("img")).not.toBeInTheDocument();
    }
  );

  it.each(INVOICE_STATUSES)(
    "links the %s card to its invoice detail route",
    (status: InvoiceStatus) => {
      render(<InvoiceCard invoice={makeInvoice(status)} />);

      expect(
        screen.getByRole("link", { name: "View Details" })
      ).toHaveAttribute("href", `/invest/invoices/${INVOICE_ID}`);
    }
  );

  it("percent-encodes invoice ids with reserved characters in the href", () => {
    render(
      <InvoiceCard invoice={makeInvoice("Verified", { id: "INV 42/2026" })} />
    );

    expect(
      screen.getByRole("link", { name: "View Details" })
    ).toHaveAttribute("href", "/invest/invoices/INV%2042%2F2026");
  });
});

describe("StatusIcon", () => {
  it.each(INVOICE_STATUSES)(
    "renders the %s icon as a decorative, hidden svg",
    (status: InvoiceStatus) => {
      const { container } = render(<StatusIcon status={status} />);

      const icon = container.querySelector("svg");
      expect(icon).not.toBeNull();
      expect(icon).toHaveAttribute("aria-hidden", "true");
      expect(icon).not.toHaveAttribute("aria-label");
      expect(screen.queryByRole("img")).not.toBeInTheDocument();
    }
  );

  it("draws a different shape for each status", () => {
    const shapes = INVOICE_STATUSES.map((status) => {
      const { container, unmount } = render(<StatusIcon status={status} />);
      const markup = container.querySelector("svg")!.innerHTML;
      unmount();
      return markup;
    });

    expect(new Set(shapes).size).toBe(shapes.length);
  });
});
