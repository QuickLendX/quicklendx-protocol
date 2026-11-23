import React from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ErrorBoundary } from "../ErrorBoundary";

/**
 * jsdom's `window.location.reload` is a non-writable own property, so it
 * cannot be spied on directly. The location accessor itself is configurable,
 * so the object is swapped for a copy whose `reload` is a mock.
 */
const installReloadSpy = () => {
  const reloadSpy = jest.fn();
  const originalLocation = window.location;

  delete (window as unknown as { location?: Location }).location;
  Object.defineProperty(window, "location", {
    configurable: true,
    writable: true,
    value: { ...originalLocation, reload: reloadSpy },
  });

  return {
    reloadSpy,
    restore: () => {
      delete (window as unknown as { location?: Location }).location;
      Object.defineProperty(window, "location", {
        configurable: true,
        writable: true,
        value: originalLocation,
      });
    },
  };
};

describe("ErrorBoundary", () => {
  it("re-renders the failed subtree on reset without reloading the page", async () => {
    const user = userEvent.setup();
    const { reloadSpy, restore } = installReloadSpy();

    let shouldThrow = true;

    const Boom: React.FC = () => {
      if (shouldThrow) {
        throw new Error("boom");
      }
      return <div>ok</div>;
    };

    render(
      <ErrorBoundary>
        <Boom />
      </ErrorBoundary>
    );

    expect(
      screen.getByText("Something went wrong. Please try again.")
    ).toBeInTheDocument();

    // The retry succeeds the second time around.
    shouldThrow = false;

    await user.click(screen.getByRole("button", { name: "Try Again" }));

    expect(await screen.findByText("ok")).toBeInTheDocument();
    expect(reloadSpy).not.toHaveBeenCalled();

    restore();
  });

  it("does not reload the page when the error is caught", () => {
    const { reloadSpy, restore } = installReloadSpy();

    const Boom: React.FC = () => {
      throw new Error("boom");
    };

    render(
      <ErrorBoundary>
        <Boom />
      </ErrorBoundary>
    );

    expect(reloadSpy).not.toHaveBeenCalled();

    restore();
  });
});
