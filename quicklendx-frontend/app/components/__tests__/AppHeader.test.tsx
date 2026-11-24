import React from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { usePathname } from "next/navigation";
import { AppHeader } from "../AppHeader";

jest.mock("next/navigation");
jest.mock("../WalletConnect", () => ({
  WalletConnect: () => <button type="button">Connect Wallet</button>,
}));

describe("AppHeader", () => {
  beforeEach(() => {
    jest.mocked(usePathname).mockReturnValue("/business/invoices/123");
  });

  it("renders primary links and marks the active section", () => {
    render(<AppHeader />);

    expect(
      screen.getByRole("navigation", { name: "Primary" })
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Invest" })).toHaveAttribute(
      "href",
      "/invest"
    );
    expect(
      screen.getByRole("link", { name: "Business invoices" })
    ).toHaveAttribute("aria-current", "page");
    expect(
      screen.getByRole("link", { name: "Settlements" })
    ).not.toHaveAttribute("aria-current");
    expect(
      screen.getByRole("button", { name: "Connect Wallet" })
    ).toBeInTheDocument();
  });

  it("toggles the compact menu and closes it after selecting a link", () => {
    render(<AppHeader />);

    const menuButton = screen.getByRole("button", { name: "Menu" });
    expect(menuButton).toHaveAttribute("aria-expanded", "false");

    fireEvent.click(menuButton);
    expect(menuButton).toHaveAttribute("aria-expanded", "true");

    fireEvent.click(screen.getByRole("link", { name: "Invest" }));
    expect(menuButton).toHaveAttribute("aria-expanded", "false");
  });
});
