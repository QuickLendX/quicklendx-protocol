/**
 * Tooltip component tests — covers the interaction rules in
 * quicklendx-frontend/tooltips.md sections 2 and 5:
 *
 *  1. Mouse hover opens after 300ms and not before
 *  2. Mouse-out closes after the 100ms close delay; leaving before the open delay cancels it
 *  3. Focus opens immediately (no delay) and blur closes
 *  4. Escape dismisses and focus stays on the trigger
 *  5. Touch tap toggles with no delay; a tap outside closes
 *  6. aria-describedby links the trigger to a role="tooltip" node while open
 */

import React from "react";
import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Tooltip } from "../Tooltip";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Dispatches a native pointer-family event with a pointerType. jsdom (jest 29)
 * has no PointerEvent constructor, so pointerType is attached explicitly.
 * React's onPointerEnter/onPointerLeave are derived from pointerover/pointerout.
 */
function dispatchPointer(
  target: Element | Document,
  type: "pointerover" | "pointerout" | "pointerdown",
  pointerType: "mouse" | "touch"
) {
  const event = new Event(type, { bubbles: true, cancelable: true });
  Object.defineProperty(event, "pointerType", { value: pointerType });
  act(() => {
    target.dispatchEvent(event);
  });
}

function hoverIn(trigger: Element) {
  dispatchPointer(trigger, "pointerover", "mouse");
}

function hoverOut(trigger: Element) {
  dispatchPointer(trigger, "pointerout", "mouse");
}

function renderTooltip() {
  render(
    <Tooltip content="Net cash after fees." variant="underline">
      Expected payout
    </Tooltip>
  );
  return screen.getByRole("button", { name: "Expected payout" });
}

function queryTooltip() {
  return screen.queryByRole("tooltip");
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

beforeEach(() => {
  jest.useFakeTimers();
});

afterEach(() => {
  jest.useRealTimers();
});

// ---------------------------------------------------------------------------
// 1 & 2: Pointer hover
// ---------------------------------------------------------------------------

describe("Tooltip — mouse hover", () => {
  it("opens only after the 300ms hover delay", () => {
    const trigger = renderTooltip();

    hoverIn(trigger);
    act(() => {
      jest.advanceTimersByTime(299);
    });
    expect(queryTooltip()).not.toBeInTheDocument();

    act(() => {
      jest.advanceTimersByTime(1);
    });
    expect(screen.getByRole("tooltip")).toHaveTextContent("Net cash after fees.");
  });

  it("closes 100ms after the pointer leaves the trigger", () => {
    const trigger = renderTooltip();

    hoverIn(trigger);
    act(() => {
      jest.advanceTimersByTime(300);
    });
    expect(queryTooltip()).toBeInTheDocument();

    hoverOut(trigger);
    act(() => {
      jest.advanceTimersByTime(99);
    });
    expect(queryTooltip()).toBeInTheDocument();

    act(() => {
      jest.advanceTimersByTime(1);
    });
    expect(queryTooltip()).not.toBeInTheDocument();
  });

  it("cancels the pending open when the pointer leaves before the delay", () => {
    const trigger = renderTooltip();

    hoverIn(trigger);
    act(() => {
      jest.advanceTimersByTime(150);
    });
    hoverOut(trigger);
    act(() => {
      jest.advanceTimersByTime(1000);
    });
    expect(queryTooltip()).not.toBeInTheDocument();
  });

  it("stays open while the pointer moves from the trigger into the tooltip", () => {
    const trigger = renderTooltip();

    hoverIn(trigger);
    act(() => {
      jest.advanceTimersByTime(300);
    });

    hoverOut(trigger);
    const tooltip = screen.getByRole("tooltip");
    dispatchPointer(tooltip, "pointerover", "mouse");

    act(() => {
      jest.advanceTimersByTime(500);
    });
    expect(queryTooltip()).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// 3 & 4: Keyboard focus and Escape
// ---------------------------------------------------------------------------

describe("Tooltip — keyboard focus", () => {
  it("opens immediately on focus with no delay", async () => {
    const user = userEvent.setup({ advanceTimers: jest.advanceTimersByTime });
    renderTooltip();

    await user.tab();

    expect(screen.getByRole("tooltip")).toHaveTextContent("Net cash after fees.");
  });

  it("closes when focus moves away (blur)", async () => {
    const user = userEvent.setup({ advanceTimers: jest.advanceTimersByTime });
    const trigger = renderTooltip();

    await user.tab();
    expect(queryTooltip()).toBeInTheDocument();

    fireEvent.blur(trigger);
    expect(queryTooltip()).not.toBeInTheDocument();
  });

  it("Escape dismisses the tooltip and focus stays on the trigger", async () => {
    const user = userEvent.setup({ advanceTimers: jest.advanceTimersByTime });
    const trigger = renderTooltip();

    await user.tab();
    expect(queryTooltip()).toBeInTheDocument();

    await user.keyboard("{Escape}");

    expect(queryTooltip()).not.toBeInTheDocument();
    expect(document.activeElement).toBe(trigger);
  });

  it("Escape also dismisses a tooltip opened by hover", () => {
    const trigger = renderTooltip();

    hoverIn(trigger);
    act(() => {
      jest.advanceTimersByTime(300);
    });
    expect(queryTooltip()).toBeInTheDocument();

    fireEvent.keyDown(trigger, { key: "Escape" });

    expect(queryTooltip()).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// 5: Touch
// ---------------------------------------------------------------------------

describe("Tooltip — touch", () => {
  it("a tap opens the tooltip with no hover delay", () => {
    const trigger = renderTooltip();

    dispatchPointer(trigger, "pointerdown", "touch");
    fireEvent.click(trigger, { detail: 1 });

    expect(screen.getByRole("tooltip")).toHaveTextContent("Net cash after fees.");
  });

  it("a second tap on the trigger closes it", () => {
    const trigger = renderTooltip();

    dispatchPointer(trigger, "pointerdown", "touch");
    fireEvent.click(trigger, { detail: 1 });
    expect(queryTooltip()).toBeInTheDocument();

    dispatchPointer(trigger, "pointerdown", "touch");
    fireEvent.click(trigger, { detail: 1 });
    expect(queryTooltip()).not.toBeInTheDocument();
  });

  it("a tap outside closes the tooltip", () => {
    const trigger = renderTooltip();

    dispatchPointer(trigger, "pointerdown", "touch");
    fireEvent.click(trigger, { detail: 1 });
    expect(queryTooltip()).toBeInTheDocument();

    dispatchPointer(document.body, "pointerdown", "touch");
    expect(queryTooltip()).not.toBeInTheDocument();
  });

  it("touch pointerover does not start the hover-open timer", () => {
    const trigger = renderTooltip();

    dispatchPointer(trigger, "pointerover", "touch");
    act(() => {
      jest.advanceTimersByTime(1000);
    });
    expect(queryTooltip()).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// 6: ARIA association
// ---------------------------------------------------------------------------

describe("Tooltip — ARIA", () => {
  it("links the trigger to the role=tooltip node via aria-describedby while open", () => {
    const trigger = renderTooltip();

    expect(trigger).not.toHaveAttribute("aria-describedby");

    hoverIn(trigger);
    act(() => {
      jest.advanceTimersByTime(300);
    });

    const tooltip = screen.getByRole("tooltip");
    expect(tooltip.id).not.toBe("");
    expect(trigger).toHaveAttribute("aria-describedby", tooltip.id);
  });

  it("renders the trigger as a type=button element", () => {
    const trigger = renderTooltip();
    expect(trigger.tagName).toBe("BUTTON");
    expect(trigger).toHaveAttribute("type", "button");
  });
});
