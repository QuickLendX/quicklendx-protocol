import React from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ErrorFallback } from "../ErrorBoundary";
import { AuthenticationError } from "../../lib/errors";

describe("ErrorFallback", () => {
  it("offers a wallet reconnection action for authentication errors", async () => {
    const user = userEvent.setup();
    const onReauthenticate = jest.fn();

    render(
      <ErrorFallback
        error={new AuthenticationError("Session expired")}
        resetErrorBoundary={jest.fn()}
        onReauthenticate={onReauthenticate}
      />
    );

    const button = screen.getByRole("button", { name: /reconnect wallet/i });
    expect(button).toBeInTheDocument();

    await user.click(button);

    expect(onReauthenticate).toHaveBeenCalledTimes(1);
  });

  it("never links to the non-existent /login route", () => {
    render(
      <ErrorFallback
        error={new AuthenticationError("Session expired")}
        resetErrorBoundary={jest.fn()}
        onReauthenticate={jest.fn()}
      />
    );

    expect(document.body.innerHTML).not.toContain("/login");
    expect(
      screen.queryByRole("link", { name: /login/i })
    ).not.toBeInTheDocument();
  });

  it("renders the error type as a heading and hides the decorative artwork", () => {
    const { container } = render(
      <ErrorFallback
        error={new AuthenticationError("Session expired")}
        resetErrorBoundary={jest.fn()}
      />
    );

    expect(
      screen.getByRole("heading", { name: "Authentication Error" })
    ).toBeInTheDocument();

    const artwork = container.querySelector("svg");
    expect(artwork).not.toBeNull();
    expect(artwork).toHaveAttribute("aria-hidden", "true");
  });
});
