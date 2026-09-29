# Buttons & Action Components: Canonical Spec

Defines the canonical button system for QuickLendX: variants, sizes, interaction
states, and loading behavior. This is a **UI/UX spec only** — it standardizes the
ad hoc Tailwind classes currently scattered across components and is the contract
for a future `quicklendx-frontend/app/components/Button.tsx`.

Sources of truth:
- Button types & interaction rules — [business-dashboard.md](./business-dashboard.md) §"Button Styles & Interactions".
- Color / spacing / interaction-state tokens — [design-tokens.md](./design-tokens.md).
- Destructive-vs-constructive friction — [review-notes.md](./review-notes.md) §2.

## 1. Variants

All variants map to semantic tokens — never raw Tailwind palette classes. Filled
text is `#FFFFFF`; outline/ghost text uses neutral/semantic foregrounds.

| Variant      | Use case                          | Surface                                   | Border                      | Text                  |
| :----------- | :-------------------------------- | :---------------------------------------- | :-------------------------- | :-------------------- |
| `primary`    | Most important action per section | `color-primary-600` `#2563EB` fill        | none                        | `#FFFFFF`             |
| `secondary`  | Supporting actions                | transparent                               | `1px color-neutral-200` `#E2E8F0` | `color-primary-900` `#1E293B` |
| `danger`     | Confirm a destructive action      | `color-danger-500` `#EF4444` fill         | none                        | `#FFFFFF`             |
| `danger-outline` | Inline destructive trigger    | transparent                               | `1px color-danger-500` `#EF4444` | `color-danger-500` `#EF4444` |
| `ghost`      | Low-emphasis / toolbar            | transparent                               | none                        | `color-neutral-500` `#64748B` |
| `icon`       | Compact, repeated (download/sort) | transparent (square)                      | none                        | `color-neutral-500` `#64748B` |

Notes:
- `secondary` is the **constructive** supporting action; do not use it for
  destructive intent.
- `primary` (Trust Blue) is reserved for the main constructive CTA per the
  "Strict Semantic Separation" decision — never use it to confirm a destructive
  action; use `danger` instead.

### 1.1 Constructive vs destructive (review-notes.md §2)

Destructive actions must carry visual friction and be unmistakably different from
constructive ones:

| Dialog role        | Variant         | Rationale                                  |
| :----------------- | :-------------- | :----------------------------------------- |
| Cancel (safe)      | `secondary`     | Flat outline, low emphasis, **default focus** |
| Confirm (build)    | `primary`       | Solid blue fill                            |
| Confirm (destroy)  | `danger`        | Solid red fill + warning icon              |

In a confirmation modal the **Cancel** button receives default keyboard focus
(safe default), and the destructive **Confirm** is solid `danger` with a leading
warning icon. The flat-outline Cancel vs solid Confirm contrast is mandatory.

## 2. Size scale

Base-8 spacing. Height is the visual box; on coarse (touch) pointers every button
is guaranteed a **≥ 48 × 48px hit area** (expand with padding or an invisible hit
layer) per the touch-target rule, even when the visual height is smaller.

| Size  | Height | Padding X | Font (token)        | Icon | Use case                         |
| :---- | :----- | :-------- | :------------------ | :--- | :------------------------------- |
| `sm`  | 32px   | 12px      | `text-caption` 12px | 16px | Dense tables, inline row actions |
| `md`  | 40px   | 16px      | 14px                | 18px | Default                          |
| `lg`  | 48px   | 24px      | `text-body` 16px    | 20px | Primary CTAs, mobile full-width  |

- `icon` buttons are square: `sm` 32×32, `md` 40×40, `lg` 48×48 (visual), always
  ≥ 48×48 touch hit area.
- Adjacent interactive elements keep **16px** spacing to prevent misclicks.
- Corner radius: `8px` (matches card/surface radius).

## 3. Interaction states

Applied uniformly per the [design-tokens.md](./design-tokens.md) Interaction
States table. States compose with every variant.

| State        | Visual feedback                                              | Notes                                  |
| :----------- | :---------------------------------------------------------- | :------------------------------------- |
| **Default**  | Variant base styling                                        | —                                      |
| **Hover**    | +10% brightness (`filter: brightness(1.1)`)                 | Suppressed when disabled or loading    |
| **Active**   | 98% scale (`transform: scale(0.98)`)                        | Tactile click confirmation             |
| **Focus**    | `2px solid color-primary-600` outline, `2px` offset         | `:focus-visible` only (keyboard)       |
| **Disabled** | 40% opacity, `cursor: not-allowed`, **no hover/active feedback** | `disabled` attr + `aria-disabled` |
| **Loading**  | Label replaced by spinner; non-interactive (see §4)         | `aria-busy="true"`                     |

- Transitions: `120ms ease-out` on brightness/transform; respect
  `prefers-reduced-motion` (drop the scale/spinner animation, keep state colors).
- Disabled buttons give **no** hover feedback (business-dashboard interaction rule).

## 4. Loading state

When an action is in progress the label is **replaced** by a spinner:

- Render a spinner in place of the text; **preserve the button's width** (reserve
  label space) so there is no layout shift.
- Set `aria-busy="true"` on the button for the duration.
- The button is non-interactive while loading: `disabled` + `pointer-events: none`,
  so clicks cannot fire twice.
- The spinner is decorative (`aria-hidden="true"`); keep the button's accessible
  name (e.g. `aria-label="Submitting…"` or a visually-hidden "Loading" via an
  `aria-live="polite"` region) so screen readers announce progress.
- On completion: success → success toast (with undo where applicable); failure →
  error toast (see [toasts.md](./toasts.md)) and the button returns to default.

Example sequence (from business-dashboard): user clicks **Accept Bid** → button
shows spinner, `aria-busy="true"` → API resolves → toast + button re-enabled.

## 5. Accessibility

- Use a real `<button>` element (never `<div onClick>`).
- **Focus visibility**: `2px` `color-primary-600` focus-visible outline on all
  variants, including `icon` and `ghost`.
- **Loading**: `aria-busy="true"`; never remove the accessible name.
- **Touch target**: ≥ 48 × 48px hit area on coarse pointers.
- **Not color alone**: pair color with icon + text — `danger`/`danger-outline`
  destructive confirms include a warning icon; `icon` buttons require an
  `aria-label`.
- **Contrast** (WCAG 2.1 AA): `#FFFFFF` on `primary-600` ≈ 4.6:1 (pass);
  `#FFFFFF` on `danger-500` ≈ 3.8:1 — acceptable for ≥ `lg`/bold button text
  (large-text 3:1) but pair with the icon + label; `secondary` text
  `primary-900` on white ≈ 16:1 (pass).

## 6. Future component API (`app/components/Button.tsx`)

Sketch for the planned implementation (not built by this spec):

```tsx
type ButtonVariant = "primary" | "secondary" | "danger" | "danger-outline" | "ghost" | "icon";
type ButtonSize = "sm" | "md" | "lg";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;   // default "primary"
  size?: ButtonSize;         // default "md"
  loading?: boolean;         // swaps label for spinner, sets aria-busy, disables
  leftIcon?: React.ReactNode;
  rightIcon?: React.ReactNode;
  fullWidth?: boolean;       // e.g. mobile CTAs
}
```

- `loading` and `disabled` both block interaction; `loading` additionally sets
  `aria-busy` and preserves width.
- `icon` variant requires an `aria-label` (enforce in dev via runtime warning).

## 7. QA checklist

- [ ] Render the full variant × size matrix at **375px** and **1280px**.
- [ ] Verify hover (+10% brightness), active (98% scale), disabled (40% opacity,
      no hover), and focus (`2px` primary outline) for every variant.
- [ ] Confirm `loading` swaps label → spinner with no width shift and sets
      `aria-busy`.
- [ ] Confirm destructive vs constructive distinction (flat outline Cancel vs
      solid Confirm) and Cancel default focus in dialogs.
- [ ] Keyboard: all buttons reachable, focus ring visible, Enter/Space activate.
- [ ] Touch targets ≥ 48 × 48px; 16px spacing between adjacent actions.
- [ ] `npm run build` and `npm run lint` clean (no code changed by this spec).
