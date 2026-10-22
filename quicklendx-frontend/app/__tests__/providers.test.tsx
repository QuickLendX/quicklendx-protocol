import React from "react";
import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Providers } from "../providers";
import { ErrorToastManager } from "../components/ErrorToast";

/**
 * The Toaster is the only place `react-hot-toast` renders toasts. Without it
 * every `toast.*` call — including `ErrorToastManager.showSuccess()` — is
 * silently dropped.
 */
describe("Providers", () => {
  afterEach(() => {
    act(() => {
      ErrorToastManager.getInstance().dismissAll();
    });
  });

  it("renders a toast emitted through the error toast manager", async () => {
    const user = userEvent.setup();

    const Btn: React.FC = () => (
      <button
        type="button"
        onClick={() => ErrorToastManager.getInstance().showSuccess("Saved")}
      >
        Save
      </button>
    );

    render(
      <Providers>
        <Btn />
      </Providers>
    );

    await user.click(screen.getByRole("button", { name: "Save" }));

    expect(await screen.findByText("Saved")).toBeInTheDocument();
  });
});
