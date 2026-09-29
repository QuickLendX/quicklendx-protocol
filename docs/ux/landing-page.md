# Landing Page Redesign Specification

## Purpose

This specification defines a production-ready marketing and entry experience
for the QuickLendX landing page. It replaces the placeholder messaging in
`quicklendx-frontend/app/page.tsx` with a trust-first, role-based page grounded
in the visual language direction, design tokens, and dashboard UX docs.

Scope is UI/UX specification only. This document does not implement the page.

## Source Files and References

- Current page to replace: `quicklendx-frontend/app/page.tsx`
- Brand asset: `quicklendx-frontend/public/quicklendx.png`
- Visual north star: [visual-direction.md](./visual-direction.md)
- Design tokens: [design-tokens.md](./design-tokens.md)
- Business role model: [business-dashboard.md](./business-dashboard.md)
- Trust and safety rules: [trust-safety-pattern-library.md](./trust-safety-pattern-library.md)
- Copy rules: [content-style-guide.md](./content-style-guide.md)

## Current Page Assessment

`app/page.tsx` currently presents QuickLendX with a centered placeholder page:

- Uses generic `from-blue-50 to-indigo-100` gradient styling.
- Shows `quicklendx.png` as a large centered logo.
- Lists basic platform features in two cards.
- Ends with "Frontend application is under active development" and "Smart
  contracts are deployed and ready for integration."

### Sections to Replace

| Current section in `app/page.tsx` | Replacement |
| :--- | :--- |
| Full-page blue/indigo gradient wrapper | Neutral institutional surface using `color-neutral-50`, `color-primary-900`, and restrained blue/teal accents. |
| Centered logo-only hero | Header plus hero with role-based entry and a protocol status panel. |
| "Decentralized Invoice Financing Platform" tagline | Production-ready value proposition focused on invoice financing, transparency, and role clarity. |
| Generic feature cards | Business and Investor path cards with distinct CTAs and responsibilities. |
| "Frontend application is under active development" | Remove entirely. Replace with trust signals and risk-aware disclosure. |
| "Smart contracts are deployed and ready for integration" | Replace with verifiable trust-signal copy that does not imply guarantees. |

## Design Direction

The page should feel like an institutional financial product, not a launch
placeholder. Use a dense but breathable layout with clear hierarchy, exact copy,
and credible proof points.

### Visual Language

- Background: `color-neutral-50` with white content surfaces and subtle borders.
- Primary text: `color-neutral-900` and `color-primary-900`.
- Primary CTA: `color-primary-600`.
- Investor accent: `color-secondary-500`.
- Risk/precaution accents: reserve `color-warning-500` for disclosure only.
- Avoid decorative gradient orbs, oversized illustrative fluff, and generic
  DeFi neon treatment.
- Use `quicklendx.png` in the header and hero proof area, not as the only first
  viewport signal.

### Tone

Copy must be calm, direct, and specific. Do not promise funding, returns,
settlement, insurance payout, or safety.

Use:

- "Expected Yield"
- "Estimated timeline"
- "Transparent invoice financing"
- "Review fees and status before signing"

Avoid:

- "Guaranteed Return"
- "Risk-free"
- "Instant funding"
- "Funds are safe"
- "No hidden risk"

## Page Architecture

1. Header
2. Hero with role split and protocol proof panel
3. Business path section
4. Investor path section
5. Trust-signal inventory
6. Risk-aware footer disclosure

## Desktop Wireframe: 1280px

```text
┌────────────────────────────────────────────────────────────────────────────┐
│ Header                                                                     │
│ [quicklendx logo] QuickLendX          Business  Investor  Docs  [Launch app]│
├────────────────────────────────────────────────────────────────────────────┤
│ Hero                                                                       │
│                                                                            │
│ ┌────────────────────────────────────────┐ ┌─────────────────────────────┐ │
│ │ H1: Invoice financing with on-chain    │ │ Protocol status panel       │ │
│ │ transparency                           │ │                             │ │
│ │                                        │ │ Network: Stellar Soroban    │ │
│ │ Businesses access liquidity from       │ │ Settlement: On-chain        │ │
│ │ approved invoice opportunities.        │ │ Data: Last synced [time]    │ │
│ │ Investors review invoice risk, fees,   │ │ Risk: Smart contract and    │ │
│ │ and Expected Yield before bidding.     │ │ liquidity risks apply.      │ │
│ │                                        │ │                             │ │
│ │ [Start as business] [Explore investing]│ │ [View protocol docs]        │ │
│ └────────────────────────────────────────┘ └─────────────────────────────┘ │
├────────────────────────────────────────────────────────────────────────────┤
│ Role entry                                                                 │
│ ┌──────────────────────────────┐ ┌───────────────────────────────────────┐ │
│ │ For businesses               │ │ For investors                         │ │
│ │ Upload invoices, compare     │ │ Browse invoice opportunities, review  │ │
│ │ bids, and track settlement   │ │ Expected Yield, and place bids with   │ │
│ │ status from one dashboard.   │ │ visible fees and risk context.        │ │
│ │ [Upload invoice] [View flow] │ │ [Browse invoices] [Review risks]      │ │
│ └──────────────────────────────┘ └───────────────────────────────────────┘ │
├────────────────────────────────────────────────────────────────────────────┤
│ Trust signals                                                              │
│ [On-chain audit trail] [Fee breakdown before signing] [Role-based privacy] │
│ [Data freshness shown] [Terminal states disclosed]                         │
├────────────────────────────────────────────────────────────────────────────┤
│ Footer disclosure                                                          │
│ Risk: Returns are estimates. You could lose funds. Smart contract, default,│
│ liquidity, and operational risks apply.                                    │
└────────────────────────────────────────────────────────────────────────────┘
```

### Desktop Annotations

- Header remains compact and functional; no marketing-heavy nav.
- H1 names the category and trust differentiator, not a vague slogan.
- Primary and secondary CTAs route by role.
- Protocol status panel gives credibility without claiming audits or uptime not
  proven in this repo.
- Role cards mirror the Business Dashboard split and keep business and investor
  data concerns separate.
- Footer disclosure is visible without dominating the page.

## Mobile Wireframe: 375px

```text
┌───────────────────────────────┐
│ [logo] QuickLendX      [Menu] │
├───────────────────────────────┤
│ H1: Invoice financing with    │
│ on-chain transparency         │
│                               │
│ Businesses access liquidity   │
│ from invoice opportunities.   │
│ Investors review risk, fees,  │
│ and Expected Yield before     │
│ bidding.                      │
│                               │
│ [Start as business]           │
│ [Explore investing]           │
├───────────────────────────────┤
│ Protocol status               │
│ Network: Stellar Soroban      │
│ Settlement: On-chain          │
│ Data: Last synced [time]      │
│ Warning: Risks apply          │
├───────────────────────────────┤
│ For businesses                │
│ Upload invoices, compare bids,│
│ and track settlement status.  │
│ [Upload invoice]              │
│ [View business flow]          │
├───────────────────────────────┤
│ For investors                 │
│ Review invoice opportunities, │
│ fees, and Expected Yield.     │
│ [Browse invoices]             │
│ [Review investor risks]       │
├───────────────────────────────┤
│ Trust signals                 │
│ - Fee breakdown before signing│
│ - On-chain audit trail        │
│ - Data freshness shown        │
│ - Role-based privacy          │
├───────────────────────────────┤
│ Risk: Returns are estimates.  │
│ You could lose funds.         │
└───────────────────────────────┘
```

### Mobile Annotations

- CTAs stack with at least 48px touch target height.
- The protocol status panel moves below hero CTAs to keep role routing above
  the fold.
- Trust signals become a compact text list; do not use icon-only labels.
- Disclosure remains readable and cannot be hidden behind a collapsed accordion.

## Copy Deck

### Header

- Brand: "QuickLendX"
- Nav links: "Business", "Investor", "Docs"
- Primary header CTA: "Launch app"

### Hero

H1:

> Invoice financing with on-chain transparency

Body:

> QuickLendX connects businesses seeking invoice liquidity with investors who
> review invoice status, fees, and Expected Yield before bidding.

Primary CTA:

> Start as business

Secondary CTA:

> Explore investing

Supporting link:

> View protocol docs

### Protocol Status Panel

Title:

> Protocol visibility

Rows:

- "Network: Stellar Soroban"
- "Activity: Bids, escrow, settlement, and disputes recorded on-chain"
- "Data freshness: Last synced [Month Day, Year, HH:MM UTC]"
- "Risk: Smart contract, default, liquidity, and operational risks apply"

### Business Path

Title:

> For businesses

Body:

> Upload invoices, compare bids, and track funding, settlement, and dispute
> status from a dedicated business dashboard.

Value props:

- "Invoice pipeline from upload to settlement"
- "Bid counts and funding progress at a glance"
- "Net payout and fee breakdown before action"
- "Dispute and default states surfaced early"

CTAs:

- Primary: "Upload invoice"
- Secondary: "View business flow"

### Investor Path

Title:

> For investors

Body:

> Browse invoice opportunities, review fees and Expected Yield, and place bids
> with risk context before signing.

Value props:

- "Expected Yield clearly labeled as an estimate"
- "Invoice status and due-date context"
- "Fees shown before wallet signing"
- "On-chain audit trail after confirmation"

CTAs:

- Primary: "Browse invoices"
- Secondary: "Review investor risks"

### Footer Risk Disclosure

> Risk: Returns are estimates. You could lose funds. Smart contract, default,
> liquidity, and operational risks apply.

## CTA Map

| CTA | Priority | Destination intent | Notes |
| :--- | :--- | :--- | :--- |
| Launch app | Header primary | Authenticated app entry | Use only when app route is available; otherwise route to role selection, not placeholder copy. |
| Start as business | Hero primary | Business onboarding or invoice upload | Aligns with Business Dashboard empty-state CTA. |
| Explore investing | Hero secondary | Investor marketplace or investor education | Must not imply guaranteed yield. |
| View protocol docs | Hero tertiary | Documentation entry | Useful for trust-oriented visitors. |
| Upload invoice | Business primary | Invoice upload flow | Medium-risk action; later screens need review confirmation. |
| View business flow | Business secondary | Business dashboard overview | Education path without signing. |
| Browse invoices | Investor primary | Invoice marketplace | Must show risk and freshness before bid actions. |
| Review investor risks | Investor secondary | Risk disclosure content | Reinforces informed consent. |

## Trust-Signal Inventory

Use these as compact proof points. Each trust signal must be factual and must
not imply absence of risk.

| Signal | Landing copy | Source rationale |
| :--- | :--- | :--- |
| On-chain audit trail | "Bids, escrow, settlement, and disputes are recorded on-chain." | Visual direction requires immutable proof links for settlements and bids. |
| Fee transparency | "Review fee breakdowns before signing." | Business Dashboard requires net payout and fee visibility. |
| Data freshness | "Last synced [timestamp]." | Visual direction and content guide require freshness for invoice data. |
| Role-based privacy | "Business and investor views keep sensitive details separated." | Business Dashboard specifies separate data visibility rules. |
| Risk disclosure | "Returns are estimates. You could lose funds." | Trust Safety library prohibits misleading guarantees. |
| Terminal-state clarity | "Default, dispute, and settlement states are shown explicitly." | Business Dashboard status model and trust library require consequence clarity. |

Do not use unsupported claims such as "audited", "bank-grade", "insured",
"guaranteed", or "risk-free" unless separate evidence and legal approval exist.

## Accessibility Specification

### Heading Order

- One `h1`: "Invoice financing with on-chain transparency"
- `h2`: "Protocol visibility"
- `h2`: "For businesses"
- `h2`: "For investors"
- `h2`: "Why teams use QuickLendX" or "Trust signals"
- Footer disclosure should not be a heading unless it introduces a longer risk
  section.

### CTA Contrast

Use token pairings that satisfy WCAG AA:

- Primary CTA: white text on `color-primary-600` (`#2563EB`).
- Secondary CTA: `color-primary-900` text on white with `color-neutral-200`
  border.
- Investor accent links may use `color-secondary-500` only with sufficient
  contrast; prefer `color-primary-900` for body text.
- Warning disclosure must include "Risk:" or "Warning:" text; do not rely on
  amber color alone.

### Interaction and Reading Order

- Keyboard order: header nav, hero primary CTA, hero secondary CTA, protocol
  docs link, business card CTAs, investor card CTAs, footer links.
- Mobile tap targets: minimum 48px high with 16px spacing between stacked CTAs.
- Logo alt text: "QuickLendX" or "QuickLendX home"; avoid "QuickLendX Logo" if
  adjacent text already names the brand.
- The risk disclosure must be visible text, not only tooltip content.

## Visual QA Plan

Because this is a specification-only task, QA is proposed for the later
implementation PR.

### 375px Mobile

- Header brand and menu fit on one row without text overlap.
- H1 wraps to 2-3 readable lines.
- Primary and secondary hero CTAs stack and remain at least 48px tall.
- Protocol status panel appears after hero CTAs.
- Role cards stack in Business then Investor order.
- Trust-signal list is readable without horizontal scroll.
- Risk disclosure remains visible and readable.

### 1280px Desktop

- Header content aligns to a constrained content width.
- Hero uses a two-column layout with the role copy dominant and protocol status
  panel secondary.
- Business and Investor cards have equal visual weight.
- No section uses the old blue/indigo placeholder gradient.
- CTA hierarchy is visually clear: one primary hero CTA, one secondary hero CTA.
- Trust-signal inventory fits without crowding or nested-card treatment.

## Review Notes and Decisions

| Decision | Rationale |
| :--- | :--- |
| Remove placeholder development copy | First impression must feel production-ready and credible. |
| Use role-based hero CTAs | Business and investor users have distinct goals and risk contexts. |
| Keep trust signals factual | Prevents unsupported guarantees while still communicating transparency. |
| Use "Expected Yield" | Aligns with content rules and avoids "Guaranteed Return." |
| Include data freshness in the landing concept | Reinforces trust-through-transparency from the first screen. |
| Preserve risk disclosure on mobile | Risk messaging must not disappear on small screens. |

## Implementation Handoff Notes

When this spec is implemented, replace the existing placeholder structure in
`app/page.tsx` rather than layering new content over it. The implementation
should use `public/quicklendx.png`, the documented design tokens, and the copy
deck above as the source of truth.
