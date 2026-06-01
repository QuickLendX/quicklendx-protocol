# Explorer Link Component Specification

This document defines the UI/UX pattern for block explorer links (Stellar Expert / Horizon). Following the principles outlined in `docs/ux/review-notes.md` and `docs/ux/visual-direction.md`, visual links to block explorers are "First Class" components and must not be hidden in sub-menus. This transparent, accessible pattern directly reinforces the "Immutable Proof" trust pillar.

*Note: This design specification will be implemented in the future as a reusable React component at `app/components/ExplorerLink.tsx`.*

**Figma source:** [Design — Explorer Link Component](https://www.figma.com/design/B4vJUfYOinfSPFTub9yVaA/Design-explorer-link-component?node-id=0-1&t=UJ8kANcf3LFHgybv-1)

## Anatomy & Affordance

The explorer link must be a distinct, self-contained component providing a consistent "view on explorer" affordance for transaction hashes, addresses, and settlement events.

- **Typography:** The hash or address must use the standard **Data Mono** typography token. This ensures design consistency with existing mono-address tokens.
- **External Link Semantics:**
  - Must open in a new tab.
  - Must include `rel="noopener noreferrer"` for security.
  - Must append an external-link icon directly following the text to indicate off-site navigation.
- **Accessibility:**
  - Use descriptive link text (e.g., "View transaction [hash] on Stellar Expert"). Never use non-descriptive phrases like "click here".
  - The element must have clear focus visibility states for keyboard navigation.

## Truncation & Copy-to-Clipboard Rules

To accommodate varying screen sizes while maintaining the Immutable Proof principle:

- **Hash Truncation:** Long transaction hashes and addresses must be truncated in the middle (e.g., first 5-6 characters, an ellipsis, and the last 4-5 characters) to fit the UI gracefully.
- **Copy Action:** 
  - The component must include a mechanism to copy the full, un-truncated hash or address to the clipboard.
  - This is typically handled by a "copy" icon button adjacent to the link.
  - Visual feedback (like a "Copied!" tooltip or toast) should be triggered upon a successful copy action.

## Placement Matrix

The explorer link must be placed consistently across the following surfaces:

| Surface | Placement |
| --- | --- |
| **Bid** | Displayed near the bid details or transaction summary, allowing users to easily verify the bid's on-chain record. |
| **Settlement Receipt** | Placed prominently as a foundational element of the receipt view to prove settlement finality. |
| **Dispute Timeline** | Embedded within individual timeline event entries so users can independently audit each recorded action. |

## QA Guidelines

- **Visual QA:** Component layout, truncation, and copy icon alignment must be verified at both **375px** (mobile) and **1280px** (desktop) viewports.
- Ensure all states (hover, focus, active) meet accessibility contrast and focus ring guidelines.
