# Metric Card Component Spec

**Version**: 1.0  
**Status**: Active  
**Last Updated**: 2026-06-01  
**Document Owner**: Product & Design Team

---

## Purpose

Define the canonical KPI card used across Business, Investor, and Admin dashboards. This spec covers the shared anatomy, value treatment, trend semantics, estimate labeling, hover/focus tooltip behavior, drill-down affordance, and accessibility requirements.

This document complements [business-dashboard.md](business-dashboard.md) Section 1 and provides the reusable pattern for future implementation in `app/components/MetricCard.tsx`.

---

## Core Principles

1. **Scanability**: Users should identify the metric, value, and trend in a single glance.
2. **Consistency**: The same structure, semantics, and motion apply across all dashboard surfaces.
3. **Clarity**: Numeric values use mono-number treatment; labels stay sentence case and plain language.
4. **Transparency**: Formulas and data freshness are discoverable on hover or keyboard focus.
5. **Actionability**: The full card is a drill-down target, not just the CTA text.

---

## Anatomy

Recommended vertical stack:

1. **Eyebrow / Label**
2. **Primary Value**
3. **Secondary Context Line**
4. **Trend Indicator**
5. **Tooltip Trigger**
6. **Drill-down affordance**

### Layout sketch

```text
┌─────────────────────────────────────┐
│ Total funded              ⓘ         │
│ $87,500                              │
│ 8.2% vs last week  ↑                 │
│ 12 invoices included                │
│ [View breakdown]                     │
└─────────────────────────────────────┘
```

### Anatomy notes

- The **entire card** is focusable and clickable.
- The tooltip trigger may be an info icon beside the label or metric name.
- The drill-down affordance should be visually secondary to the value, but clearly discoverable.

---

## Value Treatment

### Primary value

- Use the mono-number token style for all KPI values.
- Apply tabular numerals where available to stabilize digit width.
- Preserve currency symbols, percentage signs, and units inline with the value.
- Keep the number as the strongest visual weight on the card.

### Formatting rules

- Use compact notation only when the dashboard spec permits it. Otherwise show exact values.
- Keep labels sentence case.
- Do not force the value into all caps or use decorative numerals.
- If the metric is a ratio or percentage, keep the percent sign attached to the value.

### Example values

- `$87,500`
- `42%`
- `4.2 days`
- `1,284`

---

## Trend Semantics

Trend is shown as a compact secondary signal next to or below the value.

### Direction rules

- **Up arrow + green** means the metric increased relative to the comparison period.
- **Down arrow + red** means the metric decreased relative to the comparison period.
- **Neutral / flat** is allowed when the metric changed by less than the defined threshold or has no meaningful direction.

### Comparison text

- Prefer explicit comparison text such as `vs last week`, `vs last month`, or `since yesterday`.
- Keep the delta concise and factual.
- When the trend is paired with an operational metric, the comparison period must be visible in the tooltip.

### Interpretation guidance

- For favorable growth metrics, upward trend is positive.
- For risk or latency metrics, upward trend may be negative. In those cases, the semantic color still follows the meaning of the metric, not the arrow itself.
- The visual direction should never imply certainty or a guarantee; it only describes movement in the data.

### Example trend strings

- `↑ 12% vs last week`
- `↓ 5.2% vs last month`
- `→ Flat vs yesterday`

---

## Estimate Labeling

Metrics derived from forecasted, inferred, or incomplete data must be explicitly labeled with `(est.)`.

### Rules

- Place `(est.)` directly in the metric label or adjacent to the value when the entire metric is estimated.
- Do not hide estimation status in a tooltip only; it must be visible in the card.
- Use the same label treatment across dashboards so users can compare cards consistently.
- If a metric has mixed precision, the tooltip must explain which part is estimated.

### Examples

- `Expected payout (est.)`
- `$82,950 (est.)`
- `Pending funding (est.)`

### Tooltip requirement for estimates

- Explain the inputs, assumptions, and the freshness of the latest data.
- If the estimate is based on partial settlement or incomplete funding data, say so plainly.

---

## Secondary Context Line

The secondary context line gives the user one extra fact that helps interpret the value.

### Preferred content

- Count of included records
- Comparison period
- Due window or settlement horizon
- Short status qualifier

### Examples

- `12 invoices included`
- `Due in next 14 days`
- `Updated 3 minutes ago`
- `Fees deducted`

### Rules

- Keep the line to one short phrase.
- Do not repeat the primary label verbatim.
- Use plain language first.
- If the metric is estimated, the secondary line should reinforce the estimate or freshness state.

---

## Tooltip Layout

Metric tooltips should follow the shared tooltip spec in [tooltips.md](tooltips.md) and include the following order:

1. **Definition**: one short sentence describing what the metric represents.
2. **Formula**: a compact formula or calculation statement.
3. **Last updated**: the most recent data ingestion or sync time.
4. **Learn more**: optional link to the relevant dashboard or data definition.

### Tooltip content example

```text
Definition: Expected payout is the projected net amount after fees.
Formula: sum(funded_amount × (1 - fees))
Last updated: 2026-06-01 10:15 UTC
Learn more: business-dashboard.md
```

### Tooltip behavior

- Hover: show after the shared 300ms delay.
- Keyboard focus: show immediately.
- Touch: tap to open and tap outside to dismiss.
- Keep the tooltip concise and non-interactive.

---

## Drill-down Behavior

The whole card is a navigation target.

### Interaction rules

- Clicking or pressing `Enter` / `Space` on the card opens the relevant detail view.
- The drill-down target should be announced clearly in the accessible name.
- If the card contains a separate CTA, the CTA must not be the only way to navigate.

### Drill-down destinations

- Business: invoice pipeline, settlement history, dispute detail, or payout schedule.
- Investor: expected yield, bid queue, portfolio status, or repayment view.
- Admin: risk review, monitoring queue, reconciliation detail, or system metrics.

---

## States

### Default

- Clean surface, strong value, secondary context, and optional trend.

### Hover

- Slight elevation or border emphasis.
- Tooltip trigger and click target remain obvious.

### Focus

- Visible keyboard focus ring using the primary focus token.
- Tooltip may open on focus when the info trigger is focused.

### Active / pressed

- Brief press feedback on the entire card.

### Loading

- Skeleton or muted placeholder for label, value, and context.

### Empty / unavailable

- Use `—` or a short unavailable state when data has not loaded yet.
- Avoid inventing placeholder numbers.

### Error

- Show a neutral error state with a retry action when the metric cannot load.

---

## Responsive Grid

The KPI card is designed to sit inside a responsive card grid.

### Grid behavior

- **375px**: single-column stack or two-up grid only if content remains readable.
- **768px**: two to three columns depending on label length.
- **1280px**: six-card dashboard row is acceptable when the labels remain scannable.

### Density rule

- The label must never wrap so aggressively that the value loses prominence.
- If a label is too long, shorten the label rather than shrinking the value below readability.

---

## Accessibility

- The whole card must be keyboard focusable.
- The accessible name should describe both the metric and its current value.
- Include a descriptive `aria-label` such as `Total funded, $87,500, up 12 percent from last week, view breakdown`.
- Tooltip trigger must be reachable by keyboard and use `aria-describedby`.
- Use `role="button"` only when the card behaves as a button; otherwise use a semantic link for navigation.
- Focus order should let the user reach the card, then the tooltip trigger, then the drill-down action if separate.

### Screen reader guidance

- Read the primary label and value first.
- Include trend direction in the accessible name.
- Include `(est.)` in the accessible label when present visually.

---

## Visual QA Checklist

- Verify the card grid at **375px** and **1280px**.
- Confirm the value uses mono-number treatment and remains visually dominant.
- Confirm trend arrows use the correct semantic color.
- Confirm `(est.)` is visible whenever the metric is estimated.
- Confirm the tooltip is reachable by pointer and keyboard.
- Confirm the whole card is focusable and has a descriptive `aria-label`.

---

## Future Implementation Note

Suggested component entry point:

- `app/components/MetricCard.tsx`

Suggested props shape:

- `label`
- `value`
- `trend`
- `secondaryLine`
- `tooltip`
- `href` or `onClick`
- `ariaLabel`
- `estimate`

The implementation should reuse the shared tooltip component spec and the design tokens in [design-tokens.md](design-tokens.md).
