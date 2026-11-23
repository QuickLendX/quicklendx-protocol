import React from "react";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Providers } from "../../providers";
import { ErrorToastManager } from "../ErrorToast";
import { NetworkError } from "../../lib/errors";

describe("ErrorToast", () => {
  const manager = ErrorToastManager.getInstance();

  afterEach(() => {
    act(() => {
      manager.dismissAll();
    });
    jest.clearAllMocks();
  });

  it("renders a retry action and runs the retry callback", async () => {
    const user = userEvent.setup();
    const onRetry = jest.fn().mockResolvedValue(undefined);

    render(<Providers>{null}</Providers>);

    act(() => {
      manager.showError(new NetworkError("x"), { onRetry });
    });

    const retryButton = await screen.findByRole("button", { name: "Retry" });

    await user.click(retryButton);

    await waitFor(() => expect(onRetry).toHaveBeenCalledTimes(1));
  });

  it("does not render a retry action without an onRetry callback", async () => {
    render(<Providers>{null}</Providers>);

    act(() => {
      manager.showError(new NetworkError("x"));
    });

    expect(await screen.findByText("x")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Retry" })
    ).not.toBeInTheDocument();
  });

  it("shows a new toast when the retry rejects", async () => {
    const user = userEvent.setup();
    const onRetry = jest.fn().mockRejectedValue(new Error("boom"));

    render(<Providers>{null}</Providers>);

    act(() => {
      manager.showError(new NetworkError("x"), { onRetry });
    });

    const retryButton = await screen.findByRole("button", { name: "Retry" });
    await user.click(retryButton);

    expect(await screen.findByText("boom")).toBeInTheDocument();
    expect(onRetry).toHaveBeenCalledTimes(1);
  });

  it("does not show the fake 'Retrying operation...' success toast", async () => {
    const user = userEvent.setup();
    const onRetry = jest.fn().mockResolvedValue(undefined);

    render(<Providers>{null}</Providers>);

    act(() => {
      manager.showError(new NetworkError("x"), { onRetry });
    });

    await user.click(await screen.findByRole("button", { name: "Retry" }));

    await waitFor(() => expect(onRetry).toHaveBeenCalledTimes(1));
    expect(screen.queryByText("Retrying operation...")).not.toBeInTheDocument();
  });
});
