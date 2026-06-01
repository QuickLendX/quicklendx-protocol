**Tooltip & Inline-Help Component Spec**

Purpose: define the behavior, content pattern, touch vs pointer interactions, and accessibility associations for tooltips and inline help used across the app and docs.

**Summary**
- **Open delay:** 300ms on pointer hover before showing tooltip.
- **Dismiss:** immediately on mouse-out or click outside. Pressing `Esc` closes tooltips.
- **Touch:** tap-to-open (no hover delay). Tap outside or a second tap closes it.
- **Keyboard:** tooltip must be revealed on keyboard focus; toggle with `Enter`/`Space` where appropriate.
- **ARIA:** associate trigger -> tooltip with `aria-describedby`; tooltip element uses `role="tooltip"` and an `id`.

**Content Pattern**
- **Definition:** short plain-language sentence (1–2 lines).
- **Formula:** inline math when needed; prefer short KaTeX math blocks: `$...$` or `$$...$$` for formula display.
- **Last-updated:** show a concise `Last updated: YYYY-MM-DD` or relative time (e.g., `updated 3 days ago`) where data is dynamic.
- **Docs link:** link to deeper documentation when helpful (`See details` → `docs/...`).

Template (recommended order):

Definition

Formula

Last-updated

Learn more: link

Notes:
- Keep tooltip copy concise — aim for 1–3 short paragraphs.
- Avoid interactive controls inside tooltips; if interactivity is required use a flyout/panel.

**Trigger affordance & placement**
- Triggers: info icon, inline info-token, or any element with explanatory need. Use a visible affordance (info circle) for non-obvious metrics.
- Placement: preferred above the trigger; fallback below when space is limited. Use collision detection to avoid clipping.
- Offset: 8px gap between trigger and tooltip.
- Max width: 320px on large screens; scale down on small screens (min comfortable width 220px).

Responsive checks (Visual QA):
- Mobile narrow: 375px viewport — tooltip should be readable, use full-width adapted layout if necessary.
- Desktop wide: 1280px viewport — placement and max-width should present as a small, unobtrusive panel.

**Pointer & touch behavior**
- Pointer (mouse): show after 300ms hover on trigger; cancel if cursor leaves before 300ms.
- Dismiss on mouse-out (immediate) or click outside the tooltip.
- Touch (mobile/tablet): tap-to-open on trigger; do not rely on hover. On open, keep tooltip on-screen until user taps outside or taps the trigger again.
- Touchs + keyboard: when a device reports both (touch + pointer), prefer pointer rules for pointer interactions and tap behavior for direct taps.

**Keyboard & screen-reader accessibility**
- Triggers must be keyboard-focusable (e.g., `button`, or `tabindex="0"`).
- On `focus` show tooltip (immediately, no 300ms delay) so keyboard users do not have to hover.
- The trigger element should contain `aria-describedby="<tooltip-id>"` referencing the tooltip node.
- Tooltip node: `<div id="<tooltip-id>" role="tooltip">...</div>`.
- When tooltip appears, assistive tech should be able to read concise content; keep the first sentence a short definition.
- Use `aria-hidden` toggling on the tooltip when hidden vs visible as appropriate for screen readers.
- Use `Esc` to close; if the tooltip steals focus for interactive contents, ensure focus returns to trigger on close.

Example (HTML):

<button id="metric-help-1" aria-describedby="metric-help-1-tip" class="info-token" aria-label="Metric definition">i</button>
<div id="metric-help-1-tip" role="tooltip" aria-hidden="true">
  <p><strong>Definition</strong> — Shows metric growth over 30 days.</p>
  <p><strong>Formula</strong> — $$\frac{current - previous}{previous} \times 100\%$$</p>
  <p class="muted">Last updated: 2026-05-28</p>
  <p><a href="/docs/ux/business-dashboard.md">See full explanation</a></p>
</div>

Example (React API suggestion):

// app/components/Tooltip.tsx (suggested API — implement later)
// <Tooltip id="help-1" triggerAs="button" content={...} delay={300} placement="top" />

Implementation notes for future `app/components/Tooltip.tsx`:
- Props: `id`, `content` (ReactNode), `trigger` (render prop), `delay` (ms, default 300), `placement`, `offset`, `maxWidth`.
- Behavior: pointer hover respects `delay`; focus opens immediately. Touch taps open instantly.
- Accessibility: automatically add `aria-describedby` to the trigger, and `role="tooltip"` to the content node.
- Positioning: use a lightweight positioning engine (like Floating UI or Popper) to handle collisions.

**Content authoring rules**
- Each tooltip for a metric should include the three-part template (definition, formula, last-updated).
- Use KaTeX for formulas where applicable and keep formulas short; link to docs for derivations.
- When content is auto-generated from data, ensure last-updated uses the most recent data ingestion timestamp.

**Testing & QA checklist**
- Visual checks at 375px and 1280px; ensure text wraps and placement is correct.
- Pointer: hover opens after 300ms; leaving hides immediately.
- Touch: tap opens; tap outside closes.
- Keyboard: Tab to trigger shows tooltip immediately; `Esc` closes and focus remains on trigger.
- Screen reader: `aria-describedby` announces tooltip content or makes content discoverable; first sentence should serve as short summary.
- Lint/build: adding docs should not affect build, but component implementation must pass `npm run build` and `npm run lint` once implemented.

**Design tokens / style guidance**
- Visual style: match existing `info-token` styling used across app (small circular icon with neutral background). Keep tooltip background and typography consistent with system tokens.
- Animation: fade/scale in over 120ms when opening; fade out over 80ms when closing.

**Commit message suggestion**
`docs(uiux): add tooltip and inline-help component spec`

**Next tasks (suggested)**
- Implement `app/components/Tooltip.tsx` using this spec.
- Add unit/visual tests for tooltip rendering and keyboard behavior.
