import React from "react";
import { render, screen } from "@testing-library/react";
import RootLayout from "../layout";

// Jest does not transform CSS; the layout's stylesheet is irrelevant to the
// landmark assertions below.
jest.mock("../globals.css", () => ({}));

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(", ");

describe("RootLayout", () => {
  it("renders the skip link as the first element and first tab stop in the body", () => {
    const HeaderStub = () => (
      <header>
        <a href="/dashboard">Dashboard</a>
      </header>
    );

    render(
      <RootLayout>
        <HeaderStub />
        <p>Page content</p>
      </RootLayout>,
      // The layout owns <html>/<body>, so React 19 mounts them as document
      // singletons rather than inside Testing Library's default <div>.
      { container: document as unknown as HTMLElement }
    );

    const skipLink = screen.getByRole("link", {
      name: "Skip to main content",
    });

    expect(document.body.firstElementChild).toBe(skipLink);
    expect(skipLink).toHaveAttribute("href", "#main-content");
    expect(skipLink).toHaveClass("sr-only", "skip-link");

    const focusable = Array.from(
      document.body.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)
    );
    expect(focusable[0]).toBe(skipLink);
  });

  it("wraps page content in a single main landmark targeted by the skip link", () => {
    render(
      <RootLayout>
        <p>Page content</p>
      </RootLayout>,
      { container: document as unknown as HTMLElement }
    );

    const mains = screen.getAllByRole("main");
    expect(mains).toHaveLength(1);

    const main = mains[0];
    expect(main).toHaveAttribute("id", "main-content");
    expect(main).toHaveAttribute("tabindex", "-1");
    expect(main).toHaveTextContent("Page content");

    const skipLink = screen.getByRole("link", {
      name: "Skip to main content",
    });
    expect(skipLink.nextElementSibling).toBe(main);
  });
});
