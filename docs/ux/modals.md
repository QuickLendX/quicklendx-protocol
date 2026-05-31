# Modal & Dialog Component Specification

## Purpose

This specification defines the modal and dialog component system for QuickLendX.
It covers anatomy, sizes, focus management, confirmation dialogs, mobile
behavior, accessibility, and design token references. This is UX guidance only;
it does not prescribe implementation details for specific frameworks.

Modals carry high-value confirmations in QuickLendX (invoice withdrawal,
settlement actions, dispute resolution), so their accessibility and
safe-default behavior are critical for preventing user error.

## Source of Truth

Modal behavior must align with:

- `business-dashboard.md` - Confirmation modal template, focus management,
  touch optimization, keyboard navigation
- `design-tokens.md` - Elevation, color palette, spacing, typography,
  focus indicators
- `defaults-spec.md` - Grace period/default states for modal confirmation copy

---

## 1. Modal Anatomy

All modals follow a consistent structure:

```
┌──────────────────────────────────────────────┐
│ [Title]                                 [×]  │
├──────────────────────────────────────────────┤
│                                              │
│  [Content Area]                              │
│  - Scrolling enabled for long content        │
│  - Max height: 70vh (desktop)                │
│                                              │
├──────────────────────────────────────────────┤
│  [Secondary Action]  [Primary Action]        │
└──────────────────────────────────────────────┘
```

### 1.1 Backdrop Overlay

- Semi-transparent black overlay (`rgba(0, 0, 0, 0.5)`)
- Covers entire viewport
- Click-to-dismiss (when modal is dismissible)
- Prevents interaction with background content

### 1.2 Modal Container

- **Background**: `#FFFFFF` (white)
- **Border Radius**: `8px` (desktop), `0px` (mobile full-screen)
- **Shadow**: `elevation-high` (`0 10px 15px -3px rgba(0,0,0,0.1)`)
- **Padding**: `spacing-lg` (24px)
- **Max Width**: Determined by size variant (see Section 2)

### 1.3 Header

- **Title**: `text-h2` (24px, Semi-Bold), `color-neutral-900`
- **Close Button**: `×` icon, positioned top-right
- **Separator**: `1px solid color-neutral-200` below header

### 1.4 Body

- **Text**: `text-body` (16px, Regular), `color-neutral-900`
- **Line Height**: `1.5`
- **Max Height**: `70vh` on desktop; auto-expand on mobile
- **Overflow**: Scrollable when content exceeds max height

### 1.5 Footer

- **Alignment**: Right-aligned on desktop; full-width stacked on mobile
- **Button Order**: Secondary (left) → Primary (right)
- **Spacing**: `spacing-sm` (8px) between buttons
- **Separator**: `1px solid color-neutral-200` above footer

---

## 2. Modal Sizes

| Size | Width | Usage |
|:-----|:------|:------|
| `sm` | `400px` | Simple confirmations, alerts |
| `md` | `500px` | Standard forms, content display |
| `lg` | `640px` | Complex forms, multi-step flows |
| `fullscreen` | `100vw` | Mobile default (<768px) |

### 2.1 Responsive Behavior

- **Desktop (≥768px)**: Centered horizontally and vertically
- **Mobile (<768px)**: Full-screen with slide-up animation
- **Tablet (768px-1024px)**: Use `md` or `lg` size, centered

---

## 3. Focus Management

Focus management is critical for accessibility and preventing accidental
actions in high-value transactions.

### 3.1 Focus Trap

- **Tab/Shift+Tab**: Cycles through focusable elements within modal only
- **First Focusable Element**: Receives focus on modal open
- **Last Focusable Element**: Tab wraps to first focusable element
- **Shift+Tab**: Reverse cycle (last → first)

### 3.2 Initial Focus

**Standard Modals**: First focusable element (typically title or first input)

**Confirmation Dialogs (Destructive Actions)**:
- **Default Focus**: Cancel button (safe default)
- **Rationale**: Prevents accidental confirmation of destructive actions
- **Exception**: If Cancel is disabled, focus moves to next safe element

### 3.3 Focus Return

- **On Close**: Focus returns to element that triggered the modal
- **Trigger Identification**: Store reference to trigger element on open
- **Multiple Triggers**: Return to most recently focused trigger

### 3.4 Keyboard Navigation

| Key | Action |
|:----|:-------|
| `Tab` | Move focus to next focusable element |
| `Shift+Tab` | Move focus to previous focusable element |
| `Escape` | Close modal (when dismissible) |
| `Enter` | Activate focused button/link |
| `Space` | Activate focused button |

### 3.5 Focus Indicators

- **Style**: 3px solid `color-primary-600` (#2563EB)
- **Visibility**: Must be visible at all zoom levels (≥100%)
- **No Hidden Focus**: Never use `outline: none` without replacement

---

## 4. Confirmation Dialog Template

Confirmation dialogs are specialized modals for destructive or high-value
actions. They follow safe-default patterns to prevent user error.

### 4.1 Standard Confirmation

```
┌──────────────────────────────────────────────┐
│ Confirm Action                          [×]  │
├──────────────────────────────────────────────┤
│                                              │
│ Are you sure you want to proceed?            │
│                                              │
│ This action will:                            │
│ • [Consequence 1]                            │
│ • [Consequence 2]                            │
│ • Cannot be undone                           │
│                                              │
│ [Cancel]        [Confirm]                    │
│                                              │
└──────────────────────────────────────────────┘
```

**Behavior**:
- Default focus on Cancel button
- Confirm button uses primary styling (`color-primary-600`)
- Dismissible via `×` button or `Escape` key

### 4.2 Destructive Confirmation

```
┌──────────────────────────────────────────────┐
│ ⚠️ Withdraw Invoice                     [×]  │
├──────────────────────────────────────────────┤
│                                              │
│ You are withdrawing invoice INV-8200.        │
│                                              │
│ This action:                                 │
│ • Cancels all pending bids                   │
│ • Forfeits funding in progress (30%)         │
│ • Cannot be undone                           │
│                                              │
│ Reason for withdrawal (optional):            │
│ ┌────────────────────────────────────────┐   │
│ │ [Text input field]                     │   │
│ └────────────────────────────────────────┘   │
│                                              │
│ [Cancel]        [Withdraw Anyway]            │
│                                              │
└──────────────────────────────────────────────┘
```

**Behavior**:
- **Warning Icon**: ⚠️ displayed in header
- **Danger Button**: Red background (`color-danger-500` #EF4444) + warning icon
- **Default Focus**: Cancel button (safe default)
- **Optional Input**: Text field for user feedback/reason
- **Dismissible**: Via `×` button or `Escape` key

### 4.3 Button Styling

| Button Type | Background | Text | Usage |
|:------------|:-----------|:-----|:------|
| Primary | `color-primary-600` | White | Standard confirmations |
| Secondary | `color-neutral-200` | `color-neutral-900` | Cancel, dismiss |
| Danger | `color-danger-500` | White | Destructive actions |

### 4.4 Confirmation Copy Guidelines

Reference `defaults-spec.md` for grace period and default state messaging:

- **Be Specific**: Reference exact invoice numbers, amounts, deadlines
- **State Consequences**: List all consequences of the action
- **No Guarantees**: Avoid implying guaranteed outcomes
- **Time Sensitivity**: Include deadlines when relevant
- **Action Clarity**: Make button labels describe the action (not just "OK")

---

## 5. Mobile Behavior

Mobile modals prioritize touch usability and screen real estate.

### 5.1 Full-Screen Mode

- **Trigger**: Viewport width <768px
- **Dimensions**: 100vw × 100vh
- **Border Radius**: 0px (edge-to-edge)
- **Safe Area**: Respect `env(safe-area-inset-*)` for notched devices

### 5.2 Touch Optimization

- **Touch Targets**: Minimum 48px × 48px for all interactive elements
- **Spacing**: 16px between interactive elements (prevent misclicks)
- **Button Size**: Full-width buttons stacked vertically
- **Swipe to Dismiss**: Horizontal swipe gesture (optional, configurable)

### 5.3 Mobile Animation

- **Open**: Slide up from bottom (300ms ease-out)
- **Close**: Slide down to bottom (250ms ease-in)
- **Backdrop**: Fade in/out (200ms)

### 5.4 Mobile Layout

```
┌─────────────────────────────────────┐
│ [Title]                        [×]  │
├─────────────────────────────────────┤
│                                     │
│  [Content Area - Full Width]        │
│  [Scrollable if needed]             │
│                                     │
├─────────────────────────────────────┤
│  [Cancel - Full Width]              │
│  [Action - Full Width]              │
│                                     │
└─────────────────────────────────────┘
```

---

## 6. Accessibility

### 6.1 ARIA Attributes

| Attribute | Value | Element |
|:----------|:------|:--------|
| `role` | `"dialog"` | Modal container |
| `aria-modal` | `"true"` | Modal container |
| `aria-labelledby` | ID of title element | Modal container |
| `aria-describedby` | ID of description element | Modal container |
| `aria-label` | `"Close"` | Close button |

### 6.2 Screen Reader Behavior

- **On Open**: Focus moves to modal; screen reader announces title
- **During Interaction**: Standard focus navigation
- **On Close**: Focus returns to trigger; screen reader announces context

### 6.3 Keyboard Navigation

- **Tab Order**: Logical left-to-right, top-to-bottom within modal
- **Focus Trap**: Prevents focus from escaping modal
- **Escape Key**: Closes modal (unless explicitly disabled)
- **Enter/Space**: Activates focused buttons

### 6.4 Color Contrast

- **Text**: Minimum 4.5:1 contrast ratio (WCAG AA)
- **Buttons**: Minimum 4.5:1 contrast ratio
- **Focus Indicators**: Minimum 3:1 contrast ratio against background

---

## 7. Design Tokens Reference

### 7.1 Elevation

| Token | Value | Usage |
|:------|:------|:------|
| `elevation-high` | `0 10px 15px -3px rgba(0,0,0,0.1)` | Modal shadow |

### 7.2 Colors

| Token | Value | Usage |
|:------|:------|:------|
| `color-danger-500` | `#EF4444` | Destructive action buttons |
| `color-primary-600` | `#2563EB` | Primary action buttons |
| `color-neutral-200` | `#E2E8F0` | Borders, dividers |
| `color-neutral-50` | `#F8FAFC` | Modal background (optional) |
| `color-neutral-900` | `#0F172A` | Title text |

### 7.3 Spacing

| Token | Value | Usage |
|:------|:------|:------|
| `spacing-xs` | `4px` | Tight elements |
| `spacing-sm` | `8px` | Button gaps |
| `spacing-md` | `16px` | Content padding (mobile) |
| `spacing-lg` | `24px` | Modal padding (desktop) |

### 7.4 Typography

| Token | Size | Weight | Usage |
|:------|:-----|:-------|:------|
| `text-h2` | `24px` | 600 (Semi-Bold) | Modal title |
| `text-body` | `16px` | 400 (Regular) | Modal content |
| `text-caption` | `12px` | 500 (Medium) | Helper text |

### 7.5 Interaction States

| State | Visual Feedback |
|:------|:----------------|
| **Hover** | 10% brightness increase |
| **Active** | 10% scale reduction (98%) |
| **Disabled** | 40% Opacity + `not-allowed` cursor |
| **Focus** | 3px solid `color-primary-600` |

---

## 8. Animation & Transitions

### 8.1 Desktop Animation

- **Open**: Fade in + scale up (200ms ease-out)
- **Close**: Fade out (150ms ease-in)
- **Backdrop**: Fade in/out (200ms)
- **Scale**: Start at 0.95, end at 1.0

### 8.2 Mobile Animation

- **Open**: Slide up from bottom (300ms ease-out)
- **Close**: Slide down to bottom (250ms ease-in)
- **Backdrop**: Fade in/out (200ms)

### 8.3 Reduced Motion

- **Preference**: `prefers-reduced-motion: reduce`
- **Behavior**: Disable animations; use instant show/hide
- **Implementation**: Check media query and adjust timing

---

## 9. Future Implementation

### 9.1 React Component: `app/components/Modal.tsx`

**Props Interface** (preliminary):

```typescript
interface ModalProps {
  isOpen: boolean;
  onClose: () => void;
  title: string;
  size?: 'sm' | 'md' | 'lg' | 'fullscreen';
  children: React.ReactNode;
  dismissible?: boolean; // default: true
  closeOnEscape?: boolean; // default: true
  closeOnBackdrop?: boolean; // default: true
  initialFocus?: 'first' | 'cancel' | 'confirm'; // default: 'first'
  className?: string;
}
```

**Component Structure**:

```typescript
export const Modal: React.FC<ModalProps> = ({
  isOpen,
  onClose,
  title,
  size = 'md',
  children,
  dismissible = true,
  closeOnEscape = true,
  closeOnBackdrop = true,
  initialFocus = 'first',
  className,
}) => {
  // Focus trap logic
  // Keyboard event handling
  // Animation states
  // Backdrop click handling
  // Render modal structure
};
```

**Integration Notes**:
- Follow patterns from `ErrorBoundary.tsx` (client-side rendering)
- Use `"use client"` directive for Next.js compatibility
- Implement focus trap using refs and keyboard event listeners
- Support `prefers-reduced-motion` for accessibility

### 9.2 CSS Variables

Add to `globals.css`:

```css
:root {
  --modal-backdrop: rgba(0, 0, 0, 0.5);
  --modal-shadow: 0 10px 15px -3px rgba(0, 0, 0, 0.1);
  --modal-radius: 8px;
  --modal-padding: 24px;
  --modal-max-height: 70vh;
  --modal-mobile-padding: 16px;
}

@media (max-width: 767px) {
  :root {
    --modal-radius: 0px;
    --modal-padding: 16px;
    --modal-max-height: 100vh;
  }
}
```

---

## 10. Testing Checklist

### 10.1 Visual QA

- [ ] Desktop (1280px): Modal centered, focus trap works, keyboard navigation
- [ ] Mobile (375px): Full-screen, swipe-to-dismiss, touch targets 48px+
- [ ] All sizes (sm/md/lg/fullscreen) render correctly
- [ ] Focus indicators visible (3px blue outline)
- [ ] Animations smooth (200ms desktop, 300ms mobile)
- [ ] Backdrop overlay covers entire viewport

### 10.2 Accessibility

- [ ] `role="dialog"` and `aria-modal="true"` present
- [ ] Screen reader announces modal title on open
- [ ] Focus trapped within modal (Tab/Shift+Tab cycle)
- [ ] Escape key closes modal
- [ ] Focus returns to trigger on close
- [ ] Color contrast meets WCAG AA (4.5:1 minimum)

### 10.3 Functional

- [ ] Click backdrop to dismiss (when enabled)
- [ ] Close button (×) works
- [ ] Keyboard shortcuts work (Escape, Enter, Space)
- [ ] Confirmation dialogs default focus to Cancel
- [ ] Destructive actions use red styling + warning icon

---

## 11. Design Decisions

1. **Default Focus on Cancel**: For destructive confirmations, focus defaults
   to Cancel button to prevent accidental actions. This aligns with
   `business-dashboard.md` focus management rules.

2. **Danger Styling**: Red background (`color-danger-500`) + warning icon for
   destructive actions creates visual friction, reinforcing the action's
   severity.

3. **Mobile Full-Screen**: Prioritizes touch usability and screen real estate
   on small devices, per `business-dashboard.md` touch optimization guidelines.

4. **Focus Trap**: Essential for accessibility - prevents users from accidentally
   interacting with background content while modal is open.

5. **Animation Timing**: 200ms for desktop (snappy), 300ms for mobile (smooth
   slide-up) - balances responsiveness with visual feedback.

6. **Swipe to Dismiss**: Optional mobile gesture for power users; must be
   configurable to prevent accidental dismissal of critical confirmations.

---

## 12. References

- `business-dashboard.md` - Confirmation modal template (lines 685-715)
- `business-dashboard.md` - Focus management rules (lines 1139-1142)
- `business-dashboard.md` - Touch optimization (lines 1049-1055)
- `business-dashboard.md` - Keyboard navigation (lines 1076-1100)
- `design-tokens.md` - Elevation, colors, spacing, typography
- `defaults-spec.md` - Grace period/default state messaging
- `review-notes.md` - Design decision rationale
