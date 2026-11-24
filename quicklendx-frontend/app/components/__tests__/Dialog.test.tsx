/**
 * Dialog component tests — Issue #3171
 *
 * Covers the accessible-dialog contract used by the confirm and dispute flows:
 *  - initial focus moves into the dialog on open
 *  - Tab / Shift+Tab focus trapping (SC 2.1.1)
 *  - Escape closes (unless dismissible=false)
 *  - backdrop click closes (unless dismissible=false); panel clicks do not
 *  - body scroll lock while open, restored on close and unmount
 *  - focus returns to the trigger on every close path (SC 2.4.3)
 *  - background siblings are marked inert while open
 *  - aria wiring and the render-prop children API
 */

import React from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Dialog, DialogProps } from "../Dialog";

// ---------------------------------------------------------------------------
// jsdom layout shim
// ---------------------------------------------------------------------------
// jsdom performs no layout, so `offsetParent` is always null and the
// component's visibility filter would discard every focusable element.
// Emulate the browser semantics (visible element → positioned ancestor) for
// this suite only; the original descriptor is restored afterwards.

const offsetParentDescriptor = Object.getOwnPropertyDescriptor(
  HTMLElement.prototype,
  "offsetParent"
);

beforeAll(() => {
  Object.defineProperty(HTMLElement.prototype, "offsetParent", {
    configurable: true,
    get(this: HTMLElement) {
      return this.parentElement;
    },
  });
});

afterAll(() => {
  if (offsetParentDescriptor) {
    Object.defineProperty(
      HTMLElement.prototype,
      "offsetParent",
      offsetParentDescriptor
    );
  }
});

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function makeTrigger() {
  const trigger = document.createElement("button");
  trigger.setAttribute("data-test-trigger", "");
  trigger.textContent = "Open dialog";
  document.body.appendChild(trigger);
  const triggerRef = { current: trigger };
  return { trigger, triggerRef };
}

const defaultChildren = (
  <>
    <h2 id="dialog-title">Dialog title</h2>
    <p id="dialog-desc">Dialog description</p>
    <button type="button">First</button>
    <input aria-label="Amount" />
    <button type="button">Last</button>
  </>
);

function renderDialog(
  props: Partial<DialogProps> = {},
  children: React.ReactNode = defaultChildren
) {
  const onClose = props.onClose ?? jest.fn();
  const merged: DialogProps = {
    isOpen: true,
    onClose,
    labelId: "dialog-title",
    descriptionId: "dialog-desc",
    ...props,
    children,
  };
  const utils = render(<Dialog {...merged} />);
  return {
    onClose,
    ...utils,
    close: () => utils.rerender(<Dialog {...merged} isOpen={false} />),
  };
}

function getBackdrop(): HTMLElement {
  const backdrop = document.querySelector<HTMLElement>(
    '[data-dialog-portal] > div[aria-hidden="true"]'
  );
  if (!backdrop) throw new Error("Backdrop not rendered");
  return backdrop;
}

afterEach(() => {
  document.querySelectorAll("[data-test-trigger]").forEach((el) => el.remove());
  document.body.style.overflow = "";
});

// ---------------------------------------------------------------------------
// Initial focus & aria wiring
// ---------------------------------------------------------------------------

describe("Dialog — initial focus and aria", () => {
  it("moves focus to the first focusable element when opened", async () => {
    renderDialog();
    const first = screen.getByRole("button", { name: "First" });
    await waitFor(() => expect(document.activeElement).toBe(first));
  });

  it("focuses the panel itself when there are no focusable children", async () => {
    renderDialog({}, <p>No controls here</p>);
    const panel = screen.getByRole("dialog");
    await waitFor(() => expect(document.activeElement).toBe(panel));
  });

  it("wires role, aria-modal, aria-labelledby and aria-describedby", () => {
    renderDialog();
    const panel = screen.getByRole("dialog");
    expect(panel).toHaveAttribute("aria-modal", "true");
    expect(panel).toHaveAttribute("aria-labelledby", "dialog-title");
    expect(panel).toHaveAttribute("aria-describedby", "dialog-desc");
  });

  it("supports the alertdialog role for destructive confirmations", () => {
    renderDialog({ role: "alertdialog" });
    expect(screen.getByRole("alertdialog")).toHaveAttribute(
      "aria-modal",
      "true"
    );
  });

  it("renders nothing while closed", () => {
    renderDialog({ isOpen: false });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Focus trapping
// ---------------------------------------------------------------------------

describe("Dialog — focus trapping", () => {
  it("Tab from the last focusable element wraps to the first", async () => {
    const user = userEvent.setup();
    renderDialog();
    const first = screen.getByRole("button", { name: "First" });
    const last = screen.getByRole("button", { name: "Last" });
    await waitFor(() => expect(document.activeElement).toBe(first));

    last.focus();
    await user.tab();
    expect(document.activeElement).toBe(first);
  });

  it("Shift+Tab from the first focusable element wraps to the last", async () => {
    const user = userEvent.setup();
    renderDialog();
    const first = screen.getByRole("button", { name: "First" });
    const last = screen.getByRole("button", { name: "Last" });
    await waitFor(() => expect(document.activeElement).toBe(first));

    await user.tab({ shift: true });
    expect(document.activeElement).toBe(last);
  });

  it("Tab visits intermediate elements without leaving the dialog", async () => {
    const user = userEvent.setup();
    renderDialog();
    const panel = screen.getByRole("dialog");
    const first = screen.getByRole("button", { name: "First" });
    const input = screen.getByLabelText("Amount");
    const last = screen.getByRole("button", { name: "Last" });
    await waitFor(() => expect(document.activeElement).toBe(first));

    await user.tab();
    expect(document.activeElement).toBe(input);
    expect(panel.contains(document.activeElement)).toBe(true);

    await user.tab();
    expect(document.activeElement).toBe(last);
    expect(panel.contains(document.activeElement)).toBe(true);
  });

  it("Tab is swallowed when the dialog has no focusable children", async () => {
    const user = userEvent.setup();
    renderDialog({}, <p>No controls here</p>);
    const panel = screen.getByRole("dialog");
    await waitFor(() => expect(document.activeElement).toBe(panel));

    await user.tab();
    expect(document.activeElement).toBe(panel);
  });
});

// ---------------------------------------------------------------------------
// Escape
// ---------------------------------------------------------------------------

describe("Dialog — Escape key", () => {
  it("closes on Escape and returns focus to the trigger", async () => {
    const user = userEvent.setup();
    const { trigger, triggerRef } = makeTrigger();
    const { onClose, close } = renderDialog({ triggerRef });
    const first = screen.getByRole("button", { name: "First" });
    await waitFor(() => expect(document.activeElement).toBe(first));

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);

    close();
    await waitFor(() => expect(document.activeElement).toBe(trigger));
  });

  it("does not close on Escape when dismissible=false", async () => {
    const user = userEvent.setup();
    const { onClose } = renderDialog({ dismissible: false });
    const first = screen.getByRole("button", { name: "First" });
    await waitFor(() => expect(document.activeElement).toBe(first));

    await user.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("closes safely when no triggerRef is provided", async () => {
    const user = userEvent.setup();
    const { onClose, close } = renderDialog();
    const first = screen.getByRole("button", { name: "First" });
    await waitFor(() => expect(document.activeElement).toBe(first));

    await user.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledTimes(1);
    close();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Backdrop clicks
// ---------------------------------------------------------------------------

describe("Dialog — backdrop clicks", () => {
  it("closes on backdrop click and returns focus to the trigger", async () => {
    const user = userEvent.setup();
    const { trigger, triggerRef } = makeTrigger();
    const { onClose, close } = renderDialog({ triggerRef });
    await waitFor(() =>
      expect(document.activeElement).toBe(
        screen.getByRole("button", { name: "First" })
      )
    );

    await user.click(getBackdrop());
    expect(onClose).toHaveBeenCalledTimes(1);

    close();
    await waitFor(() => expect(document.activeElement).toBe(trigger));
  });

  it("does not close on backdrop click when dismissible=false", async () => {
    const user = userEvent.setup();
    const { onClose } = renderDialog({ dismissible: false });

    await user.click(getBackdrop());
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("does not close when clicking inside the panel", async () => {
    const user = userEvent.setup();
    const { onClose } = renderDialog();

    await user.click(screen.getByText("Dialog title"));
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Body scroll lock
// ---------------------------------------------------------------------------

describe("Dialog — body scroll lock", () => {
  it("locks body scroll while open", () => {
    renderDialog();
    expect(document.body.style.overflow).toBe("hidden");
  });

  it("restores the previous overflow value on close", () => {
    document.body.style.overflow = "scroll";
    const { close } = renderDialog();
    expect(document.body.style.overflow).toBe("hidden");

    close();
    expect(document.body.style.overflow).toBe("scroll");
  });

  it("restores the previous overflow value on unmount", () => {
    document.body.style.overflow = "auto";
    const { unmount } = renderDialog();
    expect(document.body.style.overflow).toBe("hidden");

    unmount();
    expect(document.body.style.overflow).toBe("auto");
  });

  it("does not touch body scroll while closed", () => {
    document.body.style.overflow = "scroll";
    renderDialog({ isOpen: false });
    expect(document.body.style.overflow).toBe("scroll");
  });
});

// ---------------------------------------------------------------------------
// Background inertness
// ---------------------------------------------------------------------------

describe("Dialog — background inertness", () => {
  it("marks background siblings inert while open and restores them on close", () => {
    const { trigger } = makeTrigger();
    const { close } = renderDialog();
    expect(trigger).toHaveAttribute("inert", "");

    close();
    expect(trigger).not.toHaveAttribute("inert");
  });
});

// ---------------------------------------------------------------------------
// Render-prop children
// ---------------------------------------------------------------------------

describe("Dialog — render-prop children", () => {
  it("provides a fallback labelId and a working handleClose", async () => {
    const user = userEvent.setup();
    const { trigger, triggerRef } = makeTrigger();
    const onClose = jest.fn();

    const renderProp = (({
      labelId,
      handleClose,
    }: {
      labelId: string;
      handleClose: () => void;
    }) => (
      <>
        <h2 id={labelId}>Render prop title</h2>
        <button type="button" onClick={handleClose}>
          Close
        </button>
      </>
    )) as unknown as React.ReactNode;

    const { rerender } = render(
      <Dialog isOpen onClose={onClose} triggerRef={triggerRef}>
        {renderProp}
      </Dialog>
    );

    const panel = screen.getByRole("dialog");
    const heading = screen.getByText("Render prop title");
    expect(panel).toHaveAttribute("aria-labelledby", heading.id);
    expect(heading.id).not.toBe("");

    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);

    rerender(
      <Dialog isOpen={false} onClose={onClose} triggerRef={triggerRef}>
        {renderProp}
      </Dialog>
    );
    await waitFor(() => expect(document.activeElement).toBe(trigger));
  });
});
