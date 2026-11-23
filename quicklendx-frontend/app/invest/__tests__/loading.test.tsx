import { render, screen } from "@testing-library/react";
import Loading from "../loading";

describe("invest loading skeleton", () => {
  it("renders an accessible, non-interactive marketplace skeleton", () => {
    render(<Loading />);

    const status = screen.getByRole("status", {
      name: "Loading investor marketplace",
    });
    expect(status).toHaveAttribute("aria-busy", "true");

    const skeletonGrid = status.querySelector('[aria-hidden="true"]');
    expect(skeletonGrid?.children).toHaveLength(6);
    expect(
      status.querySelectorAll(
        "a[href], button, input, select, textarea, [tabindex], [contenteditable='true'], [role='button'], [role='link']"
      )
    ).toHaveLength(0);
  });
});
