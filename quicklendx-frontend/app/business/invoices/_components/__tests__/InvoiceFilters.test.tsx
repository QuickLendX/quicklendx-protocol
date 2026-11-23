import React from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@testing-library/jest-dom";
import { InvoiceFilters } from "../InvoiceFilters";

describe("InvoiceFilters", () => {
  it("exposes tab semantics and calls onSelectStatus when changed", async () => {
    const user = userEvent.setup();
    const onSelectStatus = jest.fn();

    render(<InvoiceFilters selectedStatus="All" onSelectStatus={onSelectStatus} />);

    const tablist = screen.getByRole("tablist", { name: "Filter invoices by status" });
    expect(tablist).toBeInTheDocument();

    const allTab = screen.getByRole("tab", { name: "All" });
    expect(allTab).toHaveAttribute("aria-selected", "true");

    await user.click(screen.getByRole("tab", { name: "Verified" }));
    expect(onSelectStatus).toHaveBeenCalledWith("Verified");
  });

  it("supports keyboard activation", async () => {
    const user = userEvent.setup();
    const onSelectStatus = jest.fn();

    render(<InvoiceFilters selectedStatus="All" onSelectStatus={onSelectStatus} />);

    const fundedTab = screen.getByRole("tab", { name: "Funded" });
    fundedTab.focus();
    await user.keyboard("{Enter}");

    expect(onSelectStatus).toHaveBeenCalledWith("Funded");
  });
});
