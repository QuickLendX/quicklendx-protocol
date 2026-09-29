# Skeleton Loader Library — UI/UX Specification

**Version**: 1.0
**Status**: Active
**Last Updated**: 2026-06-01
**Document Owner**: Product & Design Team
**Scope**: UI/UX only — shapes and animation rules. No frontend implementation.
**Related**:
[business-dashboard.md](business-dashboard.md) (§ Loading States & Skeletons),
[design-tokens.md](design-tokens.md),
[visual-direction.md](visual-direction.md),
[modals.md](modals.md),
[investor-journey.md](investor-journey.md)

---

## Table of Contents

1. [Purpose](#purpose)
2. [Principles](#principles)
3. [Tokens & Primitives](#tokens--primitives)
4. [Shimmer Animation](#shimmer-animation)
5. [Reduced-Motion Fallback](#reduced-motion-fallback)
6. [Accessibility](#accessibility)
7. [Skeleton Set 1 — MetricCard](#skeleton-set-1--metriccard)
8. [Skeleton Set 2 — Settlement Table Row](#skeleton-set-2--settlement-table-row)
9. [Skeleton Set 3 — InvoiceCard](#skeleton-set-3--invoicecard)
10. [Skeleton Set 4 — Detail Page](#skeleton-set-4--detail-page)
11. [Composition Rules](#composition-rules)
12. [Visual QA Matrix](#visual-qa-matrix)
13. [Future Implementation Notes](#future-implementation-notes)
14. [Acceptance Checklist](#acceptance-checklist)

---

## Purpose

[business-dashboard.md](business-dashboard.md) (§ Loading States & Skeletons) calls for skeleton loaders on any section that may take more than 500 ms to load, and requires that existing data stay visible while refreshing. The existing spec is high-level. This document is the concrete shape library so a future implementation (`quicklendx-frontend/app/components/Skeleton.tsx`) can be built without ambiguity.

The library has four families:

| Family | Where it appears |
|--------|------------------|
| **MetricCard skeleton** | At-a-glance KPI cards on Business Dashboard and Investor Portfolio |
| **Table row skeleton** | Settlement Receipts table, Disputes list, generic data tables |
| **InvoiceCard skeleton** | Marketplace listings, Funding Progress cards, Portfolio rows in card mode |
| **Detail page skeleton** | Invoice detail, Dispute detail, Settlement receipt detail |

All four share a single shimmer animation and a single reduced-motion fallback, defined once below.

---

## Principles

1. **Zero layout shift.** A skeleton occupies the *same* outer dimensions and the same paddings as the rendered component it stands in for. When real content arrives, only inner pixels change — never the bounding box.
2. **Existing data wins.** On refresh, keep the previously rendered values in place. Skeletons appear only on the *first* load of a section, or when transitioning to genuinely different data.
3. **Neutral tokens only.** Skeletons use `color-neutral-200` (base) and `color-neutral-50` (highlight) from [design-tokens.md](design-tokens.md). No brand color, no risk color. A skeleton must never imply a value (positive, negative, dispute, default).
4. **Honest about uncertainty.** A skeleton is a "we are loading" signal, not a "value is being computed" signal. If a value is genuinely indeterminate (estimate, pending model output), use the estimate `(est.)` pattern from [content-style-guide.md](content-style-guide.md), not a skeleton.
5. **Shapes echo content.** Bar widths approximate the typical width of the field they replace (e.g., a 4-digit count is narrower than a long invoice ID). Do not show a perfect grid of identical bars — it reads as "broken UI" rather than "loading".
6. **Pause at the right time.** Skeletons hold for as long as the network call holds, then cross-fade out over 120 ms when real content mounts. No flash, no bounce.

---

## Tokens & Primitives

### 3.1 Skeleton Color Tokens

| Token | Value | Role |
|-------|-------|------|
| `skeleton-base` | `color-neutral-200` (#E2E8F0) | Solid bar/box color |
| `skeleton-highlight` | `color-neutral-50` (#F8FAFC) | Shimmer highlight stripe |
| `skeleton-radius-sm` | `4px` | Inline text bars |
| `skeleton-radius-md` | `8px` | Block elements, card outlines |
| `skeleton-radius-full` | `9999px` | Avatars, icon stubs, status pills |

### 3.2 Skeleton Primitives

Three primitive shapes; every skeleton in the library is composed from these.

| Primitive | Shape | Default size | Notes |
|-----------|-------|--------------|-------|
| **`SkeletonBar`** | Rectangular bar | width: variable; height: 12 / 16 / 20 / 24 / 32 px | Heights map 1:1 to `text-caption`, `text-body`, line-height of `text-h2`, `text-h1` from [design-tokens.md](design-tokens.md) |
| **`SkeletonBlock`** | Filled block | variable | Charts, progress bars, image placeholders |
| **`SkeletonCircle`** | Circle | 16 / 24 / 32 / 48 px | Icons, avatars, status dots |

```
SkeletonBar         ▭▭▭▭▭▭▭▭▭▭         (radius 4px, fills width)
SkeletonBlock       ▮▮▮▮▮▮▮▮▮▮         (radius 8px, fills width × height)
SkeletonCircle      ●                  (radius 9999px)
```

### 3.3 Width Rhythm

To avoid the "broken grid" feel, vary bar widths inside any group of 2+ bars using this rhythm:

| Position | Width relative to container |
|----------|-----------------------------|
| 1st bar | 60–70% |
| 2nd bar | 40–50% |
| 3rd bar | 80–90% |
| 4th bar | 30–40% |

Apply consistently per skeleton — same width pattern every render — so layout is deterministic.

### 3.4 Spacing

All gaps between skeleton primitives use the existing spacing tokens from [design-tokens.md](design-tokens.md):

- Between two inline bars within one line: `spacing-sm` (8px)
- Between stacked bars within a group: `spacing-sm` (8px)
- Between groups within a card: `spacing-md` (16px)
- Between card and card: matches the live card's grid gap (`spacing-md` mobile, `spacing-lg` desktop)

---

## Shimmer Animation

A single shimmer rule applies to every skeleton in the library.

| Property | Value | Reasoning |
|----------|-------|-----------|
| Animation type | Linear gradient sweep, left → right | Cheap to render; works in any container width |
| Duration | **1.4 s** | Slow enough to read as "waiting"; fast enough to confirm liveness. Matches industry baseline (Material, Ant, GitHub) |
| Easing | `linear` | Eased shimmer looks like a moving spotlight; linear reads as steady progress |
| Iteration | `infinite` | Continues until skeleton unmounts |
| Gradient stops | `skeleton-base 0%, skeleton-highlight 50%, skeleton-base 100%` | Highlight is the lighter neutral; sweep peaks at center |
| Gradient width | 200% of container width | Ensures full sweep visibility before reset |
| Direction | LTR for LTR locales; RTL for RTL locales | Reading direction; flip with `dir` attribute |
| Stagger | None within a single component; 80 ms cascade across stacked cards | Avoid synchronized-strobe effect across a page |

### Pseudo-CSS (illustrative only — not implementation)

```
.skeleton {
  background-color: var(--skeleton-base);
  background-image: linear-gradient(
    90deg,
    var(--skeleton-base) 0%,
    var(--skeleton-highlight) 50%,
    var(--skeleton-base) 100%
  );
  background-size: 200% 100%;
  animation: skeleton-shimmer 1.4s linear infinite;
  border-radius: var(--skeleton-radius-md);
}

@keyframes skeleton-shimmer {
  0%   { background-position: 100% 0; }
  100% { background-position: -100% 0; }
}
```

### Mount/Unmount Behavior

| Phase | Behavior |
|-------|----------|
| Mount | Skeleton appears immediately when load begins; no delay |
| Hold | Skeleton stays visible while data is pending |
| Cross-fade | When real content mounts, skeleton opacity 1 → 0 over **120 ms** ease-out while content opacity 0 → 1; ends with skeleton removed from DOM |
| Bail-out | If load completes in <100 ms, do not show skeleton at all — render real content directly. Prevents skeleton "flicker" on fast cache hits |

The 100 ms bail-out and 120 ms cross-fade together ensure the user never sees a sub-half-second flash of skeleton geometry.

---

## Reduced-Motion Fallback

Per WCAG 2.3.3 (Animation from Interactions) and the `prefers-reduced-motion: reduce` media query, skeletons must offer a non-animated path.

### Rule

```
@media (prefers-reduced-motion: reduce) {
  .skeleton {
    animation: none;
    background-image: none;
    background-color: var(--skeleton-base);
  }
}
```

### Behavior

| Setting | Visual |
|---------|--------|
| Motion allowed (default) | Shimmer sweep, 1.4 s, infinite |
| Motion reduced | Static `skeleton-base` block, no shimmer, no opacity pulse |

The reduced-motion fallback is **not** a fade or opacity pulse — it is a fully static placeholder. Pulsing opacity is also a motion-triggered effect and is disallowed under reduced motion.

### Liveness Without Motion

Reduced-motion users still need to know "the page is loading, not broken". Two compensating signals:

1. `aria-busy="true"` on the skeleton's container region (see [Accessibility](#accessibility) below).
2. A persistent text or icon liveness indicator at the top of the page (e.g., the existing progress indicator in the top bar). This indicator is *not* a skeleton — it is the global load state and already exists in the design system.

---

## Accessibility

### ARIA Attributes

| Attribute | Where it goes | Why |
|-----------|---------------|-----|
| `role="status"` | On the skeleton's container region | Marks the area as a live status region |
| `aria-busy="true"` | On the same container while skeleton is mounted | Signals to assistive tech that the region is loading |
| `aria-live="polite"` | On the same container | Announces "loaded" or content arrival without interrupting |
| `aria-label="Loading <section name>"` | On the container | Provides a meaningful label (e.g., "Loading settlement receipts") |

When real content mounts:
- Remove `aria-busy` (or set to `false`)
- Remove `role="status"` if the live region is no longer needed
- The cross-fade triggers a polite announcement only if `aria-live="polite"` remains

### Screen Reader Behavior

- Individual `SkeletonBar` / `SkeletonBlock` / `SkeletonCircle` primitives must carry `aria-hidden="true"`. Decorative shapes should not be announced.
- Only the *container* announces. A skeleton page with 40 bars must not produce 40 announcements.

### Focus Management

- Skeleton regions are non-interactive: no `tabindex`, no focusable children.
- If a skeleton replaces a previously interactive region (e.g., a row with action buttons), the focus must not jump. If focus *was* inside the region before refresh, follow the rule in [Existing data wins](#principles): do not replace existing content with a skeleton on refresh. Skeletons are first-load only.

### Color Contrast

- `skeleton-base` (#E2E8F0) on `color-neutral-50` (#F8FAFC) background → ~1.1:1 contrast.
- This is intentional. Skeletons are decorative loading indicators, not content, and are excluded from WCAG 1.4.3 minimum contrast. The accessible signal is the `aria-busy` region and the global load indicator, *not* the skeleton's contrast.

---

## Skeleton Set 1 — MetricCard

### Source Layout

The live MetricCard from [business-dashboard.md](business-dashboard.md) (§ At-a-Glance Metrics):

```
┌─────────────────────┐
│ TOTAL INVOICES      │   ← Label    (text-caption, 12px, uppercase)
│ 156                 │   ← Value    (text-h1, 32px, bold)
│ ↑ 12 from last week │   ← Trend    (text-body, 16px)
│ [View All]          │   ← CTA      (text-body, 16px, link)
└─────────────────────┘
```

Card outer: width = card-grid-column, height ≥ 140px, padding `spacing-lg` (24px), `elevation-low`.

### Skeleton Shape

```
┌─────────────────────┐
│ ▭▭▭▭▭▭▭▭             │   ← SkeletonBar 12px tall, ~65% width
│                      │   ← spacing-sm (8px)
│ ▭▭▭▭▭▭                │   ← SkeletonBar 32px tall, ~45% width
│                      │   ← spacing-md (16px)
│ ● ▭▭▭▭▭▭▭▭▭▭▭▭        │   ← SkeletonCircle 16px + SkeletonBar 16px tall, ~70% width
│                      │   ← spacing-sm (8px)
│ ▭▭▭▭                  │   ← SkeletonBar 16px tall, ~30% width (CTA stub)
└─────────────────────┘
```

### Specifications

| Element | Primitive | Height | Width | Radius |
|---------|-----------|--------|-------|--------|
| Label bar | SkeletonBar | 12px | 65% | 4px |
| Value bar | SkeletonBar | 32px | 45% | 4px |
| Trend dot | SkeletonCircle | 16px | 16px | full |
| Trend bar | SkeletonBar | 16px | 70% (of remaining after dot + 8px gap) | 4px |
| CTA bar | SkeletonBar | 16px | 30% | 4px |

### Layout Notes

- Card outer dimensions, padding, border, and shadow are **identical** to the live card.
- No internal lines, dividers, or icons rendered in the skeleton.
- Six MetricCards in a row use the cascade stagger (80 ms × index) so the row reads as a wave, not a strobe.

### Reduced-Motion Variant

Same shape; all five elements render as static `skeleton-base` blocks. No shimmer. Card outer carries `aria-busy="true"` and `aria-label="Loading metric: <metric name if known, else 'metric'>"`.

---

## Skeleton Set 2 — Settlement Table Row

### Source Layout

The live Settlement Receipts table from [business-dashboard.md](business-dashboard.md) (§ Settlement Receipts):

```
Date       │ Invoice # │ Amount  │ Fees    │ Net Payout │ Status    │ Action
───────────┼──────────┼─────────┼─────────┼────────────┼───────────┼────────
4/28/2026  │ INV-8201  │ $5,000  │ $150    │ $4,850     │ ✓ Settled │ [View]
```

Row: height 48px, padding-x `spacing-md` (16px), padding-y `spacing-sm` (8px). Column widths come from the live table.

### Skeleton Shape

```
▭▭▭▭▭▭▭▭ │ ▭▭▭▭▭▭▭▭▭▭ │ ▭▭▭▭▭▭ │ ▭▭▭▭▭ │ ▭▭▭▭▭▭▭ │ ●▭▭▭▭▭ │ ▭▭▭▭
```

### Specifications

| Column | Primitive | Height | Width (within column) | Radius |
|--------|-----------|--------|------------------------|--------|
| Date | SkeletonBar | 16px | 70% | 4px |
| Invoice # | SkeletonBar | 16px | 80% | 4px |
| Amount | SkeletonBar | 16px | 60% | 4px |
| Fees | SkeletonBar | 16px | 50% | 4px |
| Net Payout | SkeletonBar | 16px | 65% | 4px |
| Status | SkeletonCircle 16px + SkeletonBar 16px @ 55% | 16px | combined | full / 4px |
| Action | SkeletonBar | 16px | 40% | 4px |

### Row Count

- Default skeleton render: **8 rows** matching the typical first page of the table.
- Cascade stagger: 60 ms × row index for shimmer onset.
- Header row: rendered as live (column titles) — header is static and known before the data fetch begins. Do **not** skeletonize column headers.

### Density Variants

| Variant | Row height | Padding | Use |
|---------|------------|---------|-----|
| Comfortable | 56px | `spacing-md` × `spacing-sm` | Default desktop |
| Compact | 40px | `spacing-sm` × 4px | Power-user / Marcus persona dense view |

Skeleton heights scale with the variant — bar height stays at 16px in both; row container changes.

### Mobile (375px) Behavior

At ≤768px the table collapses to card layout per [business-dashboard.md](business-dashboard.md) (§ Responsive Design / Table Display). In that mode, use the [InvoiceCard skeleton](#skeleton-set-3--invoicecard) shape, not the row skeleton.

### Reduced-Motion Variant

Same row shape; all bars and circles render as static `skeleton-base`. Cascade stagger removed (all rows mount simultaneously). Table `<tbody>` (or list region) carries `role="status"` and `aria-busy="true"`; `aria-label="Loading settlement receipts"`.

---

## Skeleton Set 3 — InvoiceCard

### Source Layout

Used in three places, with one shared skeleton shape:

- Marketplace listings ([investor-journey.md](investor-journey.md) § Marketplace Browse)
- Funding Progress cards ([business-dashboard.md](business-dashboard.md) § Funding Progress)
- Portfolio rows in card mode at narrow widths ([investor-journey.md](investor-journey.md) § Portfolio)

Live card outline (representative):

```
┌─ INV-8201 ─────────────────────────────────────┐
│ Amount: $5,000 · Term: 30 days · Currency: USDC │
│ Estimated yield (est.): 4.2% APY                │
│ Risk band: B    Funding progress: 60% ████░░    │
│ Bid window closes: Jun 2, 2026, 18:00 UTC      │
│ Active bids: 7                                  │
│ [View details]   [Place bid]                    │
└────────────────────────────────────────────────┘
```

Card outer: width = card-grid-column, padding `spacing-lg` (24px), `elevation-low`, optional safety-rail left border per [visual-direction.md](visual-direction.md) § Component Semantics. The safety-rail border is **not** rendered in the skeleton — status is unknown during load.

### Skeleton Shape

```
┌────────────────────────────────────────────────┐
│ ▭▭▭▭▭▭▭▭                                        │   Title bar (invoice ID)
│                                                 │
│ ▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭                              │   Line 1 (amount/term/currency)
│                                                 │
│ ▭▭▭▭▭▭▭▭▭▭                                       │   Line 2 (est. yield)
│                                                 │
│ ▭▭▭▭▭  ▮▮▮▮▮▮▮▮▮▮▮▮▮                            │   Risk band + progress block
│                                                 │
│ ▭▭▭▭▭▭▭▭▭▭▭▭▭▭                                   │   Line 3 (bid window)
│                                                 │
│ ▭▭▭▭                                            │   Line 4 (active bids count)
│                                                 │
│ ▭▭▭▭▭▭     ▭▭▭▭▭▭                                │   Two CTA stubs
└────────────────────────────────────────────────┘
```

### Specifications

| Element | Primitive | Height | Width | Radius |
|---------|-----------|--------|-------|--------|
| Title bar | SkeletonBar | 20px | 35% | 4px |
| Line 1 | SkeletonBar | 16px | 80% | 4px |
| Line 2 | SkeletonBar | 16px | 50% | 4px |
| Risk band stub | SkeletonBar | 16px | 20% | 4px |
| Progress block | SkeletonBlock | 12px | 50% | 8px |
| Line 3 | SkeletonBar | 16px | 70% | 4px |
| Line 4 | SkeletonBar | 16px | 25% | 4px |
| CTA stub × 2 | SkeletonBar | 32px | 30% each | 8px |

Gaps between groups: `spacing-md` (16px). Gaps within a group: `spacing-sm` (8px).

### Stacked List

When a list of InvoiceCards is loading (e.g., Marketplace), render **5 skeleton cards** by default. Cascade stagger: 80 ms × index.

### Reduced-Motion Variant

All elements static `skeleton-base`. Card region: `role="status"`, `aria-busy="true"`, `aria-label="Loading invoice"`. The stacked-list parent carries `aria-label="Loading marketplace listings"` and the individual cards drop their `role` to avoid duplicate announcements.

---

## Skeleton Set 4 — Detail Page

### Source Layouts

Three detail pages share one skeleton template (regions toggle on/off based on the route):

- **Invoice Detail** ([investor-journey.md](investor-journey.md) § Place Bid)
- **Dispute Detail** ([business-dashboard.md](business-dashboard.md) § Disputes & Defaults)
- **Settlement Receipt Detail** ([business-dashboard.md](business-dashboard.md) § Settlement Receipts)

Common structure:

```
┌─ Header ─────────────────────────────────────────┐
│  Title + Status pill                              │
│  Subtitle / metadata row                          │
└──────────────────────────────────────────────────┘
┌─ Primary block ──────────────────────────────────┐
│  Key/value grid (2 columns × 4–6 rows)            │
└──────────────────────────────────────────────────┘
┌─ Secondary block ────────────────────────────────┐
│  Timeline OR Evidence list OR Calculation table   │
└──────────────────────────────────────────────────┘
┌─ Actions footer ─────────────────────────────────┐
│  2–4 buttons                                      │
└──────────────────────────────────────────────────┘
```

### Skeleton Shape

```
┌──────────────────────────────────────────────────┐
│  ▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭   ●▭▭▭▭▭                │  Title + status pill
│  ▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭                              │  Subtitle
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│  ▭▭▭▭▭▭     ▭▭▭▭▭▭▭▭▭                            │  KV row 1
│  ▭▭▭▭▭▭     ▭▭▭▭▭▭▭▭                             │  KV row 2
│  ▭▭▭▭▭▭     ▭▭▭▭▭▭▭▭▭▭▭▭                          │  KV row 3
│  ▭▭▭▭▭▭     ▭▭▭▭▭▭▭                              │  KV row 4
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│  ▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭                                │  Section header
│  ●  ▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭                          │  Timeline node 1
│  ●  ▭▭▭▭▭▭▭▭▭▭▭▭▭▭                                │  Timeline node 2
│  ●  ▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭▭                        │  Timeline node 3
└──────────────────────────────────────────────────┘

┌──────────────────────────────────────────────────┐
│  ▭▭▭▭▭▭▭▭     ▭▭▭▭▭▭▭▭     ▭▭▭▭▭▭▭▭               │  Action footer
└──────────────────────────────────────────────────┘
```

### Specifications

#### Header region

| Element | Primitive | Height | Width | Radius |
|---------|-----------|--------|-------|--------|
| Title bar | SkeletonBar | 32px | 50% | 4px |
| Status pill | SkeletonCircle 16px + SkeletonBar 16px × 8 ch | 24px | combined | full / 4px |
| Subtitle bar | SkeletonBar | 16px | 35% | 4px |

#### Key/Value grid

4 rows. Two columns: label (30% width) and value (60% width), `spacing-md` gap.

| Element | Primitive | Height | Width | Radius |
|---------|-----------|--------|-------|--------|
| Label | SkeletonBar | 16px | 30% (of column) | 4px |
| Value | SkeletonBar | 16px | rhythm: 80% / 70% / 90% / 60% | 4px |

#### Timeline / Evidence block

| Element | Primitive | Height | Width | Radius |
|---------|-----------|--------|-------|--------|
| Section header | SkeletonBar | 20px | 25% | 4px |
| Node dot | SkeletonCircle | 12px | 12px | full |
| Node bar | SkeletonBar (× 3) | 16px | rhythm: 60% / 45% / 70% | 4px |

#### Action footer

| Element | Primitive | Height | Width | Radius |
|---------|-----------|--------|-------|--------|
| Button stub × 2–3 | SkeletonBar | 40px | 80px–120px each | 8px |

### Region Toggles

| Page | Header | KV grid | Timeline | Action footer |
|------|--------|---------|----------|---------------|
| Invoice Detail | yes | yes | no | yes |
| Dispute Detail | yes | yes | yes | yes |
| Settlement Receipt | yes | yes | yes (settlement timeline) | yes |

### Reduced-Motion Variant

Static `skeleton-base` throughout. Page-level region carries `role="status"`, `aria-busy="true"`, `aria-label` matching the route (e.g., "Loading invoice INV-8201" — populated from URL param so even the skeleton can use the known ID).

---

## Composition Rules

### When to use which skeleton

| Surface | Skeleton |
|---------|----------|
| Top-of-page KPI strip | MetricCard × N |
| Settlement Receipts table (≥768px) | Table row × 8 |
| Settlement Receipts on mobile (<768px) | InvoiceCard × 5 |
| Marketplace, Funding Progress, Portfolio list | InvoiceCard × 5 |
| Invoice / Dispute / Settlement detail route | Detail Page template |
| Modal that loads content | InvoiceCard or KV grid block — no full-page detail skeleton inside a modal |
| Inline action (e.g., "Place bid" button submitting) | Button spinner — **not** a skeleton |

### When *not* to use a skeleton

- Existing data is on screen and being refreshed → keep existing data, show a top-bar progress indicator only.
- Expected load <100 ms → render real content directly.
- Empty state → use the empty-state pattern from [business-dashboard.md](business-dashboard.md), not a skeleton.
- Error state → use error pattern from [content-style-guide.md](content-style-guide.md).
- Estimate or pending computation → label with `(est.)` per [content-style-guide.md](content-style-guide.md), not a skeleton.

### Mixing live and skeleton content

If part of a page loads from cache instantly and part is fetched, render live content for the cached parts and skeleton only the fetched parts. Do not gate the whole page on the slowest section.

---

## Visual QA Matrix

The required QA breakpoints per the issue are **375px** (mobile baseline) and **1280px** (desktop baseline).

| Skeleton | 375px expectation | 1280px expectation |
|----------|-------------------|---------------------|
| MetricCard | Single column; each card full width; cascade stagger top-to-bottom | 3-column grid (per business-dashboard.md responsive table); 6 cards in 2 rows; cascade left-to-right |
| Table row | **Not used** — falls back to InvoiceCard skeleton | Full-width table; 8 skeleton rows; cascade top-to-bottom |
| InvoiceCard | Single column; 5 cards stacked; cascade top-to-bottom | 2- or 3-column grid depending on container; 5 cards visible above the fold; cascade left-to-right then top-to-bottom |
| Detail page | Single column; all regions stacked vertically; KV grid collapses to single column (label above value) | 2-column KV grid; timeline alongside KV on wide pages; action footer right-aligned |

### Checks at each breakpoint

- [ ] Skeleton bounding boxes match live component bounding boxes within ±2px (zero layout shift on swap)
- [ ] Cascade stagger visible but not seasick — pause between adjacent cards is 80 ms (MetricCard, InvoiceCard) or 60 ms (table row)
- [ ] Shimmer sweep crosses the full skeleton width within 1.4 s
- [ ] No skeleton renders narrower than 24px (would read as a glitch)
- [ ] Container `aria-busy="true"` while skeleton mounted; removed on swap
- [ ] `prefers-reduced-motion: reduce` results in zero animation; skeleton is fully static
- [ ] Cross-fade out (120 ms) is smooth — no flash, no bounce

---

## Future Implementation Notes

This spec is UX-only. The future engineering work lives at `quicklendx-frontend/app/components/Skeleton.tsx` (file does not yet exist).

Implementation outline (reference only — not part of this issue):

```
quicklendx-frontend/app/components/
├── ClientOnly.tsx
├── ErrorBoundary.tsx
├── ErrorToast.tsx
└── Skeleton.tsx           ← new
    ├─ <SkeletonBar />     primitive
    ├─ <SkeletonBlock />   primitive
    ├─ <SkeletonCircle />  primitive
    ├─ <MetricCardSkeleton />
    ├─ <TableRowSkeleton rows={n} />
    ├─ <InvoiceCardSkeleton />
    └─ <DetailPageSkeleton regions={['kv','timeline','actions']} />
```

Implementation must:
- Read `skeleton-base` / `skeleton-highlight` from CSS variables defined alongside the existing design tokens — do **not** hardcode hex values.
- Honor `prefers-reduced-motion: reduce` via the media query in [Reduced-Motion Fallback](#reduced-motion-fallback).
- Apply `role="status"`, `aria-busy="true"`, `aria-live="polite"`, and a descriptive `aria-label` on the container — and `aria-hidden="true"` on the primitives.
- Default to the 100 ms bail-out — accept a `delayMs` prop, default 100.
- Default to the 120 ms cross-fade — accept a `fadeOutMs` prop, default 120.
- Not introduce a third-party skeleton library — primitives are small enough that a runtime dependency is not justified.

### Build & lint verification

`npm run build` and `npm run lint` are required by the issue. They are gates for the *implementation* PR (`Skeleton.tsx`), not for this docs-only spec — running them against an unchanged frontend tree adds no signal. The implementation PR must pass both.

---

## Acceptance Checklist

For a future implementation to be considered conformant with this spec:

- [ ] All four skeleton families (MetricCard, Table row, InvoiceCard, Detail page) implemented
- [ ] Three primitives (`SkeletonBar`, `SkeletonBlock`, `SkeletonCircle`) composable for ad-hoc skeletons
- [ ] Bounding boxes verified to match live components at 375px and 1280px (zero layout shift)
- [ ] Shimmer: 1.4 s linear infinite, gradient `base → highlight → base`, 200% background-size
- [ ] Cascade stagger: 80 ms for cards, 60 ms for table rows
- [ ] Mount delay: 100 ms bail-out — skeleton skipped if data arrives sooner
- [ ] Cross-fade out: 120 ms ease-out
- [ ] `prefers-reduced-motion: reduce` removes animation entirely (static placeholder)
- [ ] `role="status"`, `aria-busy="true"`, `aria-live="polite"`, descriptive `aria-label` on container
- [ ] Primitives marked `aria-hidden="true"`
- [ ] No brand or risk colors used; only `skeleton-base` and `skeleton-highlight`
- [ ] No skeletons on refresh of already-rendered data (existing data stays in place)
- [ ] No skeleton inside an empty-state or error-state region
- [ ] Build passes (`npm run build`)
- [ ] Lint passes (`npm run lint`)

---

**Document Version**: 1.0
**Last Updated**: 2026-06-01
**For questions**: product-team@quicklendx.io
