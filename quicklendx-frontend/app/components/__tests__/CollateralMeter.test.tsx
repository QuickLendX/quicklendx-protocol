/**
 * CollateralMeter component tests — Issue #3148
 *
 * The meter maps a coverage ratio onto four risk bands documented in
 * `docs/ux/collateral-indicator.md`:
 *
 *   ratio <= 100        critical
 *   100 < ratio < 110   at-risk
 *   110 <= ratio <= 125 caution
 *   ratio > 125         healthy
 *
 * Covered here:
 * 1. Every boundary ratio (0, below 100%, exactly 100%, just above 100%,
 *    exactly 110%, exactly 125%, above 125%) plus an invalid/NaN input.
 * 2. The `role="meter"` contract: `aria-valuenow`, `aria-valuemin`,
 *    `aria-valuemax` and the composed `aria-label`.
 * 3. Bar colour/animation classes and the upper width clamp at 120%.
 * 4. The Live badge toggle and the screen-reader safety warning.
 */

import React from "react";
import { render, screen } from "@testing-library/react";
import "@testing-library/jest-dom";
import { CollateralMeter } from "../CollateralMeter";

/** Return the meter element and its inner bar. */
function renderMeter(ratio: number, isLive?: boolean) {
  render(<CollateralMeter ratio={ratio} isLive={isLive} />);
  const meter = screen.getByRole("meter");
  const bar = meter.firstElementChild as HTMLElement;
  return { meter, bar };
}

describe("CollateralMeter — risk bands", () => {
  it("treats a 0% ratio as critical", () => {
    const { meter, bar } = renderMeter(0);

    expect(meter).toHaveAttribute("aria-valuenow", "0");
    expect(meter).toHaveAttribute(
      "aria-label",
      "Collateralization Ratio: 0.0% (critical)"
    );
    expect(bar).toHaveClass("bg-rose-500");
    expect(bar.style.width).toBe("0%");
    // Critical band uses the emergency icon.
    expect(screen.getByText("🚨")).toBeInTheDocument();
  });

  it("treats a ratio below 100% as critical", () => {
    const { meter, bar } = renderMeter(85.5);

    expect(meter).toHaveAttribute(
      "aria-label",
      "Collateralization Ratio: 85.5% (critical)"
    );
    expect(bar).toHaveClass("bg-rose-500");
    expect(bar).not.toHaveClass("animate-pulse");
  });

  it("treats exactly 100% as critical (lower band is inclusive)", () => {
    const { meter, bar } = renderMeter(100);

    expect(meter).toHaveAttribute(
      "aria-label",
      "Collateralization Ratio: 100.0% (critical)"
    );
    expect(bar).toHaveClass("bg-rose-500");
    expect(bar.style.width).toBe("83.33333333333334%");
  });

  it("treats 100.1% as at-risk with the pulse treatment", () => {
    const { meter, bar } = renderMeter(100.1);

    expect(meter).toHaveAttribute(
      "aria-label",
      "Collateralization Ratio: 100.1% (at-risk)"
    );
    expect(bar).toHaveClass("bg-amber-500");
    expect(bar).toHaveClass("animate-pulse");
    expect(screen.getByText("⚠️")).toBeInTheDocument();
  });

  it("treats 109.9% as at-risk", () => {
    const { meter } = renderMeter(109.9);

    expect(meter).toHaveAttribute(
      "aria-label",
      "Collateralization Ratio: 109.9% (at-risk)"
    );
  });

  it("treats exactly 110% as caution (at-risk band is exclusive)", () => {
    const { meter, bar } = renderMeter(110);

    expect(meter).toHaveAttribute(
      "aria-label",
      "Collateralization Ratio: 110.0% (caution)"
    );
    expect(bar).toHaveClass("bg-amber-400");
    expect(bar).not.toHaveClass("animate-pulse");
    expect(screen.queryByText("⚠️")).not.toBeInTheDocument();
  });

  it("treats exactly 125% as caution (upper band is inclusive)", () => {
    const { meter, bar } = renderMeter(125);

    expect(meter).toHaveAttribute(
      "aria-label",
      "Collateralization Ratio: 125.0% (caution)"
    );
    expect(bar).toHaveClass("bg-amber-400");
  });

  it("treats a ratio above 125% as healthy", () => {
    const { meter, bar } = renderMeter(142);

    expect(meter).toHaveAttribute(
      "aria-label",
      "Collateralization Ratio: 142.0% (healthy)"
    );
    expect(bar).toHaveClass("bg-emerald-500");
    expect(bar).not.toHaveClass("animate-pulse");
    expect(screen.queryByText("🚨")).not.toBeInTheDocument();
    expect(screen.queryByText("⚠️")).not.toBeInTheDocument();
  });
});

describe("CollateralMeter — ARIA meter contract", () => {
  it("exposes valuemin=0 and valuemax=200 for every rendered ratio", () => {
    const { meter } = renderMeter(108);

    expect(meter).toHaveAttribute("role", "meter");
    expect(meter).toHaveAttribute("aria-valuemin", "0");
    expect(meter).toHaveAttribute("aria-valuemax", "200");
    expect(meter).toHaveAttribute("aria-valuenow", "108");
  });

  it("renders the numeric percentage with one decimal place", () => {
    render(<CollateralMeter ratio={98.44} />);

    expect(screen.getByText("98.4%")).toBeInTheDocument();
  });
});

describe("CollateralMeter — bar width clamping", () => {
  it("maps 60% coverage to half of the bar", () => {
    const { bar } = renderMeter(60);

    expect(bar.style.width).toBe("50%");
  });

  it("reaches the full bar exactly at the 120% visual ceiling", () => {
    const { bar } = renderMeter(120);

    expect(bar.style.width).toBe("100%");
  });

  it("clamps ratios above the visual ceiling to a full bar", () => {
    const { bar } = renderMeter(250);

    expect(bar.style.width).toBe("100%");
  });
});

describe("CollateralMeter — invalid input", () => {
  it("does not crash on NaN and degrades to the default healthy band", () => {
    const { meter, bar } = renderMeter(Number.NaN);

    // `NaN` fails every band comparison, so the component keeps its
    // "healthy" defaults rather than throwing.
    expect(meter).toHaveAttribute("aria-valuenow", "NaN");
    expect(meter).toHaveAttribute(
      "aria-label",
      "Collateralization Ratio: NaN% (healthy)"
    );
    expect(bar).toHaveClass("bg-emerald-500");
    expect(screen.getByText("NaN%")).toBeInTheDocument();
  });
});

describe("CollateralMeter — live badge and reduced-motion text", () => {
  it("shows the Live badge by default", () => {
    render(<CollateralMeter ratio={130} />);

    expect(screen.getByText("Live")).toBeInTheDocument();
  });

  it("hides the Live badge when isLive is false", () => {
    render(<CollateralMeter ratio={130} isLive={false} />);

    expect(screen.queryByText("Live")).not.toBeInTheDocument();
  });

  it("renders the safety-buffer warning only for the at-risk band", () => {
    const { unmount } = render(<CollateralMeter ratio={105} />);
    expect(
      screen.getByText("Warning: Ratio is below the 110% safety buffer.")
    ).toBeInTheDocument();
    unmount();

    render(<CollateralMeter ratio={115} />);
    expect(
      screen.queryByText("Warning: Ratio is below the 110% safety buffer.")
    ).not.toBeInTheDocument();
  });
});
