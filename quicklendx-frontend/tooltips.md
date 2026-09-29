# Tooltip & Inline Help System UX Specification

**Status**: Active  
**Target Components**: Future `app/components/Tooltip.tsx`  
**Purpose**: Supports the "Trust through Transparency" UX pillar by ensuring financial definitions, formulas, and data freshness details are easily discoverable without cluttering the UI.

---

## 1. Trigger Affordances

Users must clearly understand when an element has a tooltip. Triggers should never look like standard text or primary action buttons.

- **Info Icon (`ⓘ`)**: Used next to section headers, metric cards, or standalone data points.
- **Warning/Estimate Icon (`⚠️` / `(est.)`)**: Used when a value is an estimate, requires precaution, or indicates a dynamic protocol fee.
- **Dotted Underline**: Used for inline text (e.g., <span style="border-bottom: 1px dotted #888">Expected Payout</span>). Never use solid underlines, as those denote navigation links.

---

## 2. Interaction Design & Delays

To prevent UI flashing while the user moves their cursor across the dashboard, tooltips require deliberate interaction timings.

### Pointer (Mouse) Behavior

- **Open Delay**: `300ms` hover delay before the tooltip appears.
- **Close Delay**: `100ms` delay on mouse-out to allow the user to move their cursor into the tooltip if it contains a clickable link.
- **Dismissal**: Dismisses immediately upon clicking the trigger, clicking outside, moving the mouse out of the trigger/tooltip area (after 100ms), or pressing the `Escape` key.

### Touch (Mobile/Tablet) Behavior

- **Open Action**: Tap the trigger to open. There is no delay.
- **Dismissal**: Tap anywhere outside the tooltip (backdrop tap) or tap the trigger again to close.
- **Conflict Prevention**: Inline text triggers (dotted underlines) should have a minimum touch target area of `44x44px`. If placed tightly together, prefer Info Icons spaced appropriately.

### Keyboard Behavior

- **Open Action**: Tooltip opens immediately when the trigger receives keyboard `Focus`.
- **Dismissal**: Closes when focus moves away (`Blur`) or when the user presses `Escape`.

---

## 3. Positioning & Layout

- **Default Position**: `Top` (centered above the trigger).
- **Offset**: `8px` gap between the trigger element and the tooltip arrow/bubble.
- **Collision Detection**: Auto-flip to `Bottom`, `Left`, or `Right` if the tooltip would be clipped by the viewport boundaries (critical for 375px mobile views).
- **Max Width**: `280px` for desktop, `calc(100vw - 32px)` on mobile screens to ensure readability.

---

## 4. Content Pattern

Every financial or metric tooltip must follow a consistent, receipt-style template.

### Standard Template

1. **Definition**: Clear, jargon-free explanation of the metric or term.
2. **Formula / Variables** (if applicable): The math behind the number.
3. **Last Updated**: Real-time data freshness stamp.
4. **Documentation Link** (Optional): A "Learn More" link to full protocol docs.

### Example: "Expected Payout" Tooltip

```text
[Definition]
The net cash you will receive after all protocol and investor fees are deducted.

[Formula]
Formula: SUM(funded_amount) × (1 - fees)

[Data Freshness]
Last Updated: Apr 28, 3:45 PM

[Link]
→ View fee schedule
```

---

## 5. Accessibility (WCAG 2.1 AA)

Tooltips must act as accessible descriptions to the element they decorate.

- **Role**: The tooltip container must have `role="tooltip"`.
- **Association**: The trigger element must use `aria-describedby="[tooltip-id]"` referencing the ID of the tooltip container.
- **Focusable Triggers**: Icons or text acting as triggers must be wrapped in a `<button type="button">` or have `tabindex="0"` to receive keyboard focus.
- **Color Contrast**: Tooltip background should be high-contrast (e.g., dark slate/black) with white text, meeting the `4.5:1` minimum contrast ratio.

### Example Implementation Structure

```html
<button
  type="button"
  class="tooltip-trigger"
  aria-describedby="expected-payout-tooltip"
>
  Expected Payout <span class="info-icon">ⓘ</span>
</button>

<div id="expected-payout-tooltip" role="tooltip" class="tooltip-content">
  <!-- Content Template Goes Here -->
</div>
```

---

## 6. Future Implementation Notes

- **Component Architecture**: This specification will be implemented as a reusable `<Tooltip />` component in `app/components/Tooltip.tsx`.
- **Recommended Tech**: Use headless UI primitives (e.g., Radix UI Tooltip or Floating UI) to handle the complex positioning, portal rendering, touch event listeners, and `aria` attribute associations out of the box.
- **Visual QA Requirement**: Before merging the frontend component, it must be visually tested at `375px` (mobile viewport edge clipping) and `1280px` (desktop standard).
