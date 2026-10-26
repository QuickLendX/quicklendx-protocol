/**
 * ConfirmAction component tests — Issue #2880 acceptance criteria
 *
 * Covers:
 * 1. Tab cycle visits all focusable elements and wraps (SC 2.1.1, SC 2.4.3)
 * 2. Shift+Tab visits them in reverse
 * 3. Focus returns to triggerRef after onConfirm resolves (success path)
 * 4. Focus returns to triggerRef after timeout "Dismiss" click
 * 5. Pressing Escape after re-rendering with a new onClose prop calls the
 *    new handler, not the stale one (fixes stale closure bug)
 */

import React, { createRef } from "react";
import { render, screen, act, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ConfirmAction, ConfirmActionProps } from "../../app/components/ConfirmAction";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const defaultFees = [
  {
    label: "Platform fee",
    amount: 0.0050000,
    isEstimate: false,
  },
];

function makeProps(overrides?: Partial<ConfirmActionProps>): ConfirmActionProps {
  return {
    isOpen: true,
    onClose: jest.fn(),
    onConfirm: jest.fn().mockResolvedValue(undefined),
    invoiceId: "INV-001",
    bidId: "BID-001",
    lockedAmountXlm: 1000,
    lockUntilDate: new Date(Date.now() + 86_400_000 * 7),
    fees: defaultFees,
    collateralisationRatio: 1.15,
    txHash: "a".repeat(64),
    ...overrides,
  };
}

it("does not render an explorer link for an invalid transaction hash", () => {
  render(<ConfirmAction {...makeProps({ txHash: "../../account/GBAD" })} />);

  expect(screen.queryByText("View on Stellar Explorer")).not.toBeInTheDocument();
});

// Mock react-hot-toast so it doesn't error in jsdom
jest.mock("react-hot-toast", () => ({
  __esModule: true,
  default: { success: jest.fn(), error: jest.fn() },
  toast: { success: jest.fn(), error: jest.fn() },
  success: jest.fn(),
  error: jest.fn(),
}));

// ---------------------------------------------------------------------------
// Test 1: Tab cycle wraps forward through all focusable elements
// ---------------------------------------------------------------------------

describe("ConfirmAction — Tab focus cycle", () => {
  it("Tab cycles through all focusable elements and wraps back to first", async () => {
    const user = userEvent.setup();
    render(<ConfirmAction {...makeProps()} />);

    // Dialog should be present in DOM
    const dialog = screen.getByRole("alertdialog");
    expect(dialog).toBeInTheDocument();

    // Get all focusable elements inside the dialog
    const getFocusable = () =>
      Array.from(
        dialog.querySelectorAll<HTMLElement>(
          'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
        )
      ).filter((el) => el.offsetParent !== null || el.tagName === "BUTTON");

    const focusable = getFocusable();
    expect(focusable.length).toBeGreaterThan(1);

    // Focus first element
    focusable[0].focus();
    expect(document.activeElement).toBe(focusable[0]);

    // Tab through all elements
    for (let i = 1; i < focusable.length; i++) {
      await user.tab();
      // After each Tab we expect to be on a focusable element inside dialog
      expect(dialog.contains(document.activeElement)).toBe(true);
    }

    // One more Tab should wrap back to first
    await user.tab();
    expect(document.activeElement).toBe(focusable[0]);
  });

  it("Shift+Tab cycles through focusable elements in reverse", async () => {
    const user = userEvent.setup();
    render(<ConfirmAction {...makeProps()} />);

    const dialog = screen.getByRole("alertdialog");

    const getFocusable = () =>
      Array.from(
        dialog.querySelectorAll<HTMLElement>(
          'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
        )
      ).filter((el) => el.offsetParent !== null || el.tagName === "BUTTON");

    const focusable = getFocusable();
    const last = focusable[focusable.length - 1];
    const first = focusable[0];

    // Start on first
    first.focus();
    expect(document.activeElement).toBe(first);

    // Shift+Tab from first should wrap to last
    await user.tab({ shift: true });
    expect(document.activeElement).toBe(last);
  });
});

// ---------------------------------------------------------------------------
// Test 2: Focus returns to triggerRef after onConfirm resolves
// ---------------------------------------------------------------------------

describe("ConfirmAction — focus return on close", () => {
  it("returns focus to triggerRef after onConfirm resolves through two-step button", async () => {
    const user = userEvent.setup();
    // Create a real DOM button as the trigger
    const triggerBtn = document.createElement("button");
    triggerBtn.textContent = "Open dialog";
    document.body.appendChild(triggerBtn);
    const triggerRef = createRef<HTMLElement>();
    (triggerRef as React.MutableRefObject<HTMLElement>).current = triggerBtn;

    const onConfirm = jest.fn().mockResolvedValue(undefined);
    const onClose = jest.fn();

    const { rerender } = render(
      <ConfirmAction
        {...makeProps({ onConfirm, onClose, triggerRef })}
      />
    );

    // Click "Confirm acceptance" twice with gap (two-step button)
    const twoStepBtn = screen.getByRole("button", { name: /confirm acceptance/i });
    await user.click(twoStepBtn);
    // Wait for the reset timer gap (500ms min)
    await act(async () => { await new Promise((r) => setTimeout(r, 600)); });
    await user.click(twoStepBtn);

    // Wait for onConfirm and close to run
    await waitFor(() => expect(onClose).toHaveBeenCalled());

    // Close the dialog by rerendering with isOpen=false
    rerender(
      <ConfirmAction
        {...makeProps({ onConfirm, onClose, triggerRef, isOpen: false })}
      />
    );

    await waitFor(() => {
      expect(document.activeElement).toBe(triggerBtn);
    });

    document.body.removeChild(triggerBtn);
  });

  it("returns focus to triggerRef after Dismiss click (timeout state)", async () => {
    jest.useFakeTimers();
    const user = userEvent.setup({ advanceTimers: jest.advanceTimersByTime });

    const triggerBtn = document.createElement("button");
    document.body.appendChild(triggerBtn);
    const triggerRef = createRef<HTMLElement>();
    (triggerRef as React.MutableRefObject<HTMLElement>).current = triggerBtn;

    // onConfirm never resolves — will hit SERVER_TIMEOUT_MS (10s)
    const onConfirm = jest.fn(() => new Promise<void>(() => {}));
    const onClose = jest.fn();

    const { rerender } = render(
      <ConfirmAction
        {...makeProps({ onConfirm, onClose, triggerRef })}
      />
    );

    // Trigger submission via two-step button
    const twoStepBtn = screen.getByRole("button", { name: /confirm acceptance/i });
    await user.click(twoStepBtn);
    await act(async () => { jest.advanceTimersByTime(600); });
    await user.click(twoStepBtn);

    // Advance past server timeout (10 000 ms)
    await act(async () => { jest.advanceTimersByTime(10_100); });

    // "Dismiss" button should now be visible
    const dismissBtn = await screen.findByRole("button", { name: /dismiss/i });
    await user.click(dismissBtn);

    await waitFor(() => expect(onClose).toHaveBeenCalled());

    rerender(
      <ConfirmAction
        {...makeProps({ onConfirm, onClose, triggerRef, isOpen: false })}
      />
    );

    await waitFor(() => {
      expect(document.activeElement).toBe(triggerBtn);
    });

    document.body.removeChild(triggerBtn);
    jest.useRealTimers();
  });
});

// ---------------------------------------------------------------------------
// Test 3: Escape key calls the *current* onClose, not a stale closure
// ---------------------------------------------------------------------------

describe("ConfirmAction — Escape key calls current onClose (no stale closure)", () => {
  it("pressing Escape after re-rendering with a new onClose calls the new handler", async () => {
    const user = userEvent.setup();
    const firstOnClose = jest.fn();
    const secondOnClose = jest.fn();

    const { rerender } = render(
      <ConfirmAction {...makeProps({ onClose: firstOnClose })} />
    );

    // Re-render with a different onClose before pressing Escape
    rerender(<ConfirmAction {...makeProps({ onClose: secondOnClose })} />);

    await user.keyboard("{Escape}");

    expect(secondOnClose).toHaveBeenCalledTimes(1);
    expect(firstOnClose).not.toHaveBeenCalled();
  });
});
