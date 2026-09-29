# Collateralization Ratio Indicator UX Specification

## Purpose
The Collateralization Ratio Indicator provides a real-time visualization of the health of an invoice's collateral relative to its funded bids. It alerts users to market fluctuations that may put their investment or financing at risk.

## Audience
- **Businesses**: To monitor if their collateral needs topping up.
- **Investors**: To assess the risk level of their active investments.

## Component: `CollateralMeter`

### 1. Gauge Anatomy
The indicator consists of three layers of information:

1.  **Label**: "Collateral Ratio" with an info icon linking to a detailed explanation tooltip.
2.  **Numeric Value**: A large, high-contrast percentage (e.g., `142%`).
3.  **Visual Meter**: A horizontal segmented bar or circular gauge showing position relative to thresholds.

### 2. Threshold Mapping (Risk Bands)

| Ratio Range | UX Status | Color Token | Visual Treatment | Meaning |
| :--- | :--- | :--- | :--- | :--- |
| **> 125%** | Healthy | `color-success-500` | Solid Emerald | Low risk; well-collateralized. |
| **110% - 125%** | Caution | `color-warning-500` | Solid Amber | Monitoring required; approaching limit. |
| **101% - 109%** | **At-Risk** | `color-warning-500` | **Pulse Animation** | High urgency; collateral top-up recommended. |
| **<= 100%** | Critical | `color-danger-500` | Solid Rose + Static Glow | Under-collateralized; liquidation/default risk. |

### 3. "Pulse" & Color-Shift Behavior
When the ratio drops below **110%**, the UI triggers an "Attention Shift":

-   **Animation**: A subtle scale and opacity pulse (ease-in-out, 2s duration).
-   **Color Shift**: The indicator background or border shifts to a high-visibility Amber (`color-warning-500`).
-   **Rationale**: 110% is the protocol's "Soft Buffer" boundary (Assumption #3 in `visual-direction.md`).

### 4. Reduced-Motion Fallback
For users with `prefers-reduced-motion: reduce`:
-   **No Pulse**: The pulse animation is disabled.
-   **Visual Substitute**: A high-contrast thick border (2px solid) and a "⚠️" (Warning icon) are added next to the percentage value to signify urgency without movement.

### 5. Interaction & Tooltips
-   **Live Labeling**: Every instance of the ratio must be accompanied by a "Live" or "Last updated: HH:MM" label to clarify it is not a fixed historical value.
-   **Tooltip Content**: 
    > "This ratio represents the current market value of locked collateral divided by the total bid amount. Values fluctuate with asset prices."

### 6. Accessibility (WCAG 2.1 AA)
-   **Aria-Label**: The gauge must have an `aria-label` like `Collateralization Ratio: 108% (At-Risk)`.
-   **Role**: Use `role="meter"` with `aria-valuenow`, `aria-valuemin`, and `aria-valuemax`.
-   **Color Contrast**: Ensure text over semantic backgrounds maintains a 4.5:1 ratio.

---

## Technical Note
Future implementation should live in `app/components/CollateralMeter.tsx`. This component should accept `ratio: number` and `isLive: boolean` as primary props.

## Responsive Behavior
-   **Mobile (375px)**: Stacked layout; meter occupies full width below the numeric value.
-   **Desktop (1280px)**: Compact inline layout for use in tables or list cards; expanded side-by-side layout for detail pages.
