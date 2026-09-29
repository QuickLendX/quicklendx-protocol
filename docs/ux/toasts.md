# Toast Notifications: Visual & Accessibility Spec

Defines the visual system and screen-reader semantics for the toast component
(`quicklendx-frontend/app/components/ErrorToast.tsx`). This refines the concrete
component only — no error-handler logic changes. Severity/category come from
`quicklendx-frontend/app/lib/errors.ts`; colors come from
[design-tokens.md](./design-tokens.md).

## 1. Visual system

Every toast is a **light neutral surface** with a **severity-colored accent**:

- Surface: `#FFFFFF` background, `1px` `color-neutral-200` (`#E2E8F0`) border,
  `elevation-high` shadow (`0 10px 15px -3px rgba(0,0,0,0.1)`).
- Accent: `4px` left border + icon, colored by the severity's semantic token.
- Layout: severity icon · (category label + message + optional Retry) · Dismiss.
- Width: `min(92vw, 380px)` so it fits 375px mobile and reads well at 1280px.

**Color is never the only signal.** Each toast always carries a severity **icon**
*and* a text **category label** (e.g. "Network Error"), so meaning survives for
color-blind users and in monochrome.

## 2. Severity → token → visual mapping

| Severity   | Semantic token      | Accent (icon + border) | Icon            |
| :--------- | :------------------ | :--------------------- | :-------------- |
| `CRITICAL` | `color-danger-500`  | `#EF4444`              | Alert octagon   |
| `HIGH`     | `color-danger-500`  | `#EF4444`              | Alert triangle  |
| `MEDIUM`   | `color-warning-500` | `#F59E0B`              | Alert triangle  |
| `LOW`      | `color-info-500`    | `#3B82F6`              | Info circle     |

Text colors (constant across severities, on the white surface):

| Element        | Token                 | Value     |
| :------------- | :-------------------- | :-------- |
| Category label | `color-neutral-900`   | `#0F172A` |
| Message        | `color-neutral-700`   | `#334155` |
| Dismiss icon   | `color-neutral-500`   | `#64748B` |
| Retry text     | `color-primary-600`   | `#2563EB` |

## 3. Contrast verification (WCAG 2.1)

Verified against the white (`#FFFFFF`) surface. Text passes AA; the colored
accent is a supplementary cue paired with the text label, so it is not the sole
information carrier.

| Pair                                   | Ratio   | AA (4.5:1 text / 3:1 non-text) |
| :------------------------------------- | :------ | :------------------------------ |
| Label `#0F172A` on `#FFFFFF`           | ~17.9:1 | Pass                            |
| Message `#334155` on `#FFFFFF`         | ~10.4:1 | Pass                            |
| Dismiss `#64748B` on `#FFFFFF`         | ~4.8:1  | Pass                            |
| Retry `#2563EB` on `#FFFFFF`           | ~5.2:1  | Pass                            |
| Danger accent `#EF4444` on `#FFFFFF`   | ~3.8:1  | Pass (non-text)                 |
| Info accent `#3B82F6` on `#FFFFFF`     | ~3.7:1  | Pass (non-text)                 |
| Warning accent `#F59E0B` on `#FFFFFF`  | ~2.0:1  | Below 3:1 — supplementary only* |

\* This resolves the old "yellow on black" problem: the prior design used
`bg-yellow-500` with `text-black`, and `bg-red-600`/`bg-orange-500` solid fills
that left small text near/below AA. The new light-surface design keeps all
**text** comfortably above AA. The warning **accent** alone is below the 3:1
non-text threshold, which is acceptable because it never carries meaning on its
own — it is always paired with the alert-triangle icon and the "…Error" label.

Focus: interactive controls (Retry, Dismiss) use a `2px` `color-primary-600`
focus-visible ring, matching the design-token focus state.

## 4. Announcement semantics (screen readers)

The severity drives the live-region role so urgent errors interrupt and minor
ones queue politely:

| Severity         | `role`   | `aria-live`  | Behavior                          |
| :--------------- | :------- | :----------- | :-------------------------------- |
| `CRITICAL`/`HIGH`| `alert`  | `assertive`  | Interrupts and announces at once  |
| `MEDIUM`/`LOW`   | `status` | `polite`     | Announced after current speech    |

- `aria-atomic="true"` so the label and message are read as one unit.
- Severity icons are `aria-hidden` (decorative; the text label is authoritative).
- Dismiss button has `aria-label="Dismiss notification"`.

## 5. Auto-dismiss timing

Set per severity in `ErrorToastManager.showError` (overridable via
`options.duration` / `options.dismissible`):

| Severity   | Auto-dismiss            |
| :--------- | :---------------------- |
| `LOW`      | 4000 ms                 |
| `MEDIUM`   | 6000 ms                 |
| `HIGH`     | 8000 ms                 |
| `CRITICAL` | Persists (manual close) |

Critical toasts never auto-dismiss so a high-value-transaction failure cannot be
missed; the user must dismiss them explicitly.

## 6. QA checklist

- [ ] Render all four severities at 375px and 1280px.
- [ ] Confirm icon + label present for every severity (no color-only states).
- [ ] `npm run build` and `npm run lint` clean.
- [ ] Verify `alert`/`assertive` vs `status`/`polite` announcements with a
      screen reader.
- [ ] Keyboard: Retry and Dismiss reachable and show the focus ring.
