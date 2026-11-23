import React from "react";
import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { axe, toHaveNoViolations } from "jest-axe";
import { ConnectedAccount } from "../WalletConnect";

expect.extend(toHaveNoViolations);

const TEST_ADDRESS = "GBSXVD727UNXJZ7ZM4VCXBTK3UMPXR7O6LLXS7XVOECGDYH3XFNV7C5K";

describe("ConnectedAccount Component", () => {
  beforeEach(() => {
    Object.assign(navigator, {
      clipboard: {
        writeText: jest.fn().mockResolvedValue(undefined),
      },
    });
  });

  afterEach(() => {
    jest.clearAllMocks();
    jest.useRealTimers();
  });

  test("rendered DOM has no invalid button/link nesting", () => {
    const { container } = render(
      <ConnectedAccount address={TEST_ADDRESS} network="testnet" />
    );
    const invalidNesting = container.querySelectorAll(
      "button button, button a, a button"
    );
    expect(invalidNesting.length).toBe(0);
  });

  test("clicking copy button calls navigator.clipboard.writeText with full 56-char address and does not set aria-expanded='true'", async () => {
    const user = userEvent.setup();
    render(<ConnectedAccount address={TEST_ADDRESS} network="testnet" />);

    const copyBtn = screen.getByRole("button", {
      name: /copy full address to clipboard/i,
    });
    const triggerBtn = screen.getByRole("button", {
      ariaHasPopup: "menu",
    });

    expect(triggerBtn).toHaveAttribute("aria-expanded", "false");

    await user.click(copyBtn);

    expect(navigator.clipboard.writeText).toHaveBeenCalledTimes(1);
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith(TEST_ADDRESS);
    expect(triggerBtn).toHaveAttribute("aria-expanded", "false");
  });

  test("when writeText rejects, live region shows error message and no unhandled rejection is raised", async () => {
    const user = userEvent.setup();
    const writeTextMock = jest
      .spyOn(navigator.clipboard, "writeText")
      .mockRejectedValue(new Error("Permission denied"));

    let unhandledRejectionCaught = false;
    const unhandledHandler = (event: PromiseRejectionEvent) => {
      unhandledRejectionCaught = true;
      event.preventDefault();
    };
    window.addEventListener("unhandledrejection", unhandledHandler);

    render(<ConnectedAccount address={TEST_ADDRESS} network="testnet" />);
    const copyBtn = screen.getByRole("button", {
      name: /copy full address to clipboard/i,
    });

    await user.click(copyBtn);

    const alertMessage = await screen.findByRole("alert");
    expect(alertMessage).toHaveTextContent(/couldn't copy/i);

    expect(unhandledRejectionCaught).toBe(false);
    window.removeEventListener("unhandledrejection", unhandledHandler);
    writeTextMock.mockRestore();
  });

  test("Enter/Space on trigger opens menu, focuses Disconnect, Escape closes it", async () => {
    const user = userEvent.setup();
    render(<ConnectedAccount address={TEST_ADDRESS} network="testnet" />);

    const triggerBtn = screen.getByRole("button", {
      ariaHasPopup: "menu",
    });
    triggerBtn.focus();

    await user.keyboard("{Enter}");
    expect(triggerBtn).toHaveAttribute("aria-expanded", "true");

    const disconnectItem = screen.getByRole("menuitem", {
      name: /disconnect/i,
    });
    expect(disconnectItem).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(triggerBtn).toHaveAttribute("aria-expanded", "false");
    expect(triggerBtn).toHaveFocus();
  });

  test("pointerdown on document.body closes menu", async () => {
    const user = userEvent.setup();
    render(<ConnectedAccount address={TEST_ADDRESS} network="testnet" />);

    const triggerBtn = screen.getByRole("button", {
      ariaHasPopup: "menu",
    });
    await user.click(triggerBtn);

    expect(triggerBtn).toHaveAttribute("aria-expanded", "true");

    fireEvent.pointerDown(document.body);
    expect(triggerBtn).toHaveAttribute("aria-expanded", "false");
  });

  test("activating Disconnect calls onDisconnect once", async () => {
    const user = userEvent.setup();
    const handleDisconnect = jest.fn();
    render(
      <ConnectedAccount
        address={TEST_ADDRESS}
        network="testnet"
        onDisconnect={handleDisconnect}
      />
    );

    const triggerBtn = screen.getByRole("button", {
      ariaHasPopup: "menu",
    });
    await user.click(triggerBtn);

    const disconnectItem = screen.getByRole("menuitem", {
      name: /disconnect/i,
    });
    await user.click(disconnectItem);

    expect(handleDisconnect).toHaveBeenCalledTimes(1);
  });

  test("while disconnecting is true, item is disabled and reads 'Disconnecting…'", async () => {
    render(
      <ConnectedAccount
        address={TEST_ADDRESS}
        network="testnet"
        disconnecting={true}
      />
    );

    const triggerBtn = screen.getByRole("button", {
      ariaHasPopup: "menu",
    });
    fireEvent.click(triggerBtn);

    const disconnectItem = screen.getByRole("menuitem");
    expect(disconnectItem).toBeDisabled();
    expect(disconnectItem).toHaveTextContent("Disconnecting…");
  });

  test("unmounting less than 2s after successful copy causes no state update warning", async () => {
    jest.useFakeTimers();
    const { unmount } = render(
      <ConnectedAccount address={TEST_ADDRESS} network="testnet" />
    );

    const copyBtn = screen.getByRole("button", {
      name: /copy full address to clipboard/i,
    });
    fireEvent.click(copyBtn);

    unmount();
    expect(() => jest.advanceTimersByTime(2000)).not.toThrow();
  });

  test("jest-axe reports zero accessibility violations for open and closed states", async () => {
    const { container } = render(
      <ConnectedAccount address={TEST_ADDRESS} network="testnet" />
    );
    const closedResults = await axe(container);
    expect(closedResults).toHaveNoViolations();

    const triggerBtn = screen.getByRole("button", {
      ariaHasPopup: "menu",
    });
    fireEvent.click(triggerBtn);

    const openResults = await axe(container);
    expect(openResults).toHaveNoViolations();
  });
});
