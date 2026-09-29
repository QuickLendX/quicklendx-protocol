# Invoice Card Component Specification

## Overview
The **InvoiceCard** component is the primary UI element used to display invoice information across the Marketplace, Dashboard, and Portfolio views. It follows the **Safety Rail** pattern defined in `docs/ux/visual-direction.md`, applying a colored left‑border that conveys the invoice lifecycle status.

---

## Anatomy
| Region | Description |
|---|---|
| **Left Safety‑Rail Border** | 4 px solid bar on the left side. Color varies by status (see *Status Mapping*). |
| **Header** | Displays the invoice ID (e.g., `#INV‑12345`) with a subtle typographic hierarchy. |
| **Body** | Shows **Amount** (formatted per design token `--amount-medium`), **Due Date**, and optional **Recipient** name. |
| **Status Indicator** | Icon + text label placed at top‑right of the card, mirroring the Safety‑Rail color. |
| **Actions** | Primary CTA (e.g., **View**, **Pay**) aligned to the bottom‑right; secondary links (e.g., **Details**) at bottom‑left. |

---

## Variants & Densities
| Variant | Use‑case | Layout |
|---|---|---|
| **Compact** (Table‑row) | Marketplace listing, Portfolio grid | Single‑line height, reduced padding, hidden secondary actions. |
| **Expanded** (Card) | Dashboard detail view, standalone page | Full padding, all actions visible, larger typography. |

### Density Rules
- **Compact**: `padding: 8px 12px`; font size `13px`; icon size `16px`.
- **Expanded**: `padding: 16px 24px`; font size `15px`; icon size `20px`.
- Breakpoints: Visual QA at **375 px** (mobile) and **1280 px** (desktop).

---

## Status Mapping
| Lifecycle Status | Border Color | Icon (SF Symbol) | Text Label |
|---|---|---|---|
| **PAID** | `emerald-500` (green) | `checkmark.circle.fill` | **Paid** |
| **DEFAULTED** | `rose-600` (red‑rose) | `exclamationmark.triangle.fill` | **Defaulted** |
| **PENDING** | `amber-400` (yellow) | `hourglass.bottomhalf.fill` | **Pending** |
| **DRAFT** | `gray-300` (neutral) | `pencil.circle.fill` | **Draft** |

*All status colors follow the token naming convention defined in `docs/ux/design-tokens.md`.*

---

## Accessibility
- The status is conveyed **both** by the left border **and** an icon + text label. Color alone is not sufficient.
- Ensure sufficient contrast ratios (≥ 4.5:1) for border and text.
- Interactive elements have clear focus outlines.

---

## Future Implementation
A future React component will be created at `frontend/app/components/InvoiceCard.tsx` that consumes the spec above.

---

*Design authored by the UI/UX team. Updated 2026‑05‑31.*
