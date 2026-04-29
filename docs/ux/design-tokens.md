# Design Tokens: QuickLendX Protocol

This document defines the core design tokens and visual semantics for the QuickLendX interface. These tokens ensure consistency across platforms and reinforce the protocol's focus on trust, security, and professional lending.

## 1. Color Palette

### 1.1 Brand Colors
| Token | Value (Hex) | Usage |
| :--- | :--- | :--- |
| `color-primary-900` | `#1E293B` | Deep Navy: Headers, text emphasis |
| `color-primary-600` | `#2563EB` | Trust Blue: Primary actions, brand elements |
| `color-primary-100` | `#DBEAFE` | Soft Blue: Background accents |
| `color-secondary-500` | `#0D9488` | Growth Teal: Success actions, investment highlights |

### 1.2 Semantic & Risk Colors
Semantic colors are tied to protocol states and risk levels.
| Token | Value (Hex) | Semantic Meaning |
| :--- | :--- | :--- |
| `color-success-500` | `#10B981` | Paid, Fully Collateralized, Verified |
| `color-warning-500` | `#F59E0B` | Upcoming Due Date, Low Collateral, Pending Action |
| `color-danger-500` | `#EF4444` | Overdue, Defaulted, High Risk, Unauthorized |
| `color-info-500` | `#3B82F6` | System Notifications, Metadata, Tooltips |

### 1.3 Neutral Scale
| Token | Value (Hex) | Usage |
| :--- | :--- | :--- |
| `color-neutral-900` | `#0F172A` | Primary Body Text |
| `color-neutral-500` | `#64748B` | Secondary Text, Icons |
| `color-neutral-200` | `#E2E8F0` | Borders, Dividers |
| `color-neutral-50` | `#F8FAFC` | App Backgrounds, Surface Layers |

---

## 2. Typography

### 2.1 Font Families
- **Primary Sans**: `Inter`, `-apple-system`, `BlinkMacSystemFont`, `sans-serif` (Readable, professional).
- **Data Mono**: `JetBrains Mono`, `monospace` (For Stellar addresses, Transaction Hashes, and precise amounts).

### 2.2 Type Scale
| Token | Size | Weight | Usage |
| :--- | :--- | :--- | :--- |
| `text-h1` | 32px | 700 (Bold) | Page Titles |
| `text-h2` | 24px | 600 (Semi-Bold) | Section Headers |
| `text-body` | 16px | 400 (Regular) | Default Body Text |
| `text-caption` | 12px | 500 (Medium) | Small metadata, helper text |

---

## 3. Spacing & Layout
QuickLendX uses a **Base-8 (8px)** spacing system.

| Token | Value | Usage |
| :--- | :--- | :--- |
| `spacing-xs` | 4px | Tight elements, small icons |
| `spacing-sm` | 8px | Inner component padding |
| `spacing-md` | 16px | Standard gap between components |
| `spacing-lg` | 24px | Container padding |
| `spacing-xl` | 48px | Section vertical spacing |

---

## 4. Elevation & Surfaces
Used to create hierarchy and focus in the dashboard.

| Token | Shadow Value | Usage |
| :--- | :--- | :--- |
| `elevation-flat` | `none` | Background surfaces |
| `elevation-low` | `0 1px 3px rgba(0,0,0,0.1)` | Cards, input fields |
| `elevation-high` | `0 10px 15px -3px rgba(0,0,0,0.1)` | Modals, dropdowns, risk alerts |

---

## 5. Interaction States
Consistency in feedback prevents user error in high-value transactions.

| State | Visual Feedback | Rationale |
| :--- | :--- | :--- |
| **Hover** | 10% brightness increase | Indicates interactability |
| **Active** | 10% scale reduction (98%) | Tactile confirmation of click |
| **Disabled** | 40% Opacity + `not-allowed` cursor | Prevents unauthorized/invalid actions |
| **Focus** | 2px solid `color-primary-600` | Accessibility and keyboard navigation |
