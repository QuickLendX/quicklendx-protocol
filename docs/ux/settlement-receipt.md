# Settlement Receipt Detail Layout & Download UX Spec

**Issue:** #1031  
**Scope:** UI/UX specification only — no API integration.  
**References:** `docs/ux/business-dashboard.md` §5, `docs/ux/design-tokens.md`, `docs/ux/visual-direction.md`, `docs/ux/explorer-link.md`

---

## 1. Page Layout

**Route:** `app/business/settlements/[id]/page.tsx`  
**Access:** Authenticated business users only. Scoped to their own settlements.

```
┌─ bg-gray-50 (color-neutral-50) ───────────────────────────────────────┐
│  py-12 px-4                                                           │
│                                                                       │
│  ┌─ max-w-3xl mx-auto ─────────────────────────────────────────────┐  │
│  │                                                                 │  │
│  │  ← Back to Settlements                                         │  │
│  │                                                                 │  │
│  │  Settlement Receipt                          [Status badge]    │  │
│  │  #REC-20260428-001                                             │  │
│  │  April 28, 2026 at 3:45 PM                                     │  │
│  │                                                                 │  │
│  │  ┌─ Invoice Details block ──────────────────────────────────┐  │  │
│  │  ├─ Funding Details block ──────────────────────────────────┤  │  │
│  │  └─ Settlement Calculation block ──────────────────────────┘  │  │
│  │                                                                 │  │
│  │  ┌─ Export Actions ────────────────────────────────────────┐  │  │
│  │  │  [↓ PDF]  [↓ JSON]  [⎙ Print]  [✉ Email]  [⧗ Timeline] │  │  │
│  │  └─────────────────────────────────────────────────────────┘  │  │
│  │                                                                 │  │
│  └─────────────────────────────────────────────────────────────────┘  │
└───────────────────────────────────────────────────────────────────────┘
```

---

## 2. Block Anatomy

### 2.1 Invoice Details Block

```
┌─ INVOICE DETAILS ──────────────────────────────────────────────────┐
│                                                                    │
│  Invoice ID        INV-8201                    [→ View Invoice]   │
│  Debtor            Acme Corp  ✓ Verified                          │
│  Invoice Amount    $5,000.00                                       │
│  Issue Date        April 15, 2026                                  │
│  Due Date          May 15, 2026                                    │
│  Description       Professional Services, April 2026              │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

- Block background: `#FFFFFF`, border: `1px solid #E2E8F0`, radius: `8px`, padding: `24px`
- Shadow: `elevation-low` (`0 1px 3px rgba(0,0,0,0.1)`)
- Label column: `text-caption` (12px/500), `color-neutral-500`
- Value column: `text-body` (16px/400), `color-neutral-900`
- "Verified" badge: `color-success-500` (`#10B981`) dot + text
- **Debtor name**: visible to business only — see §4 for privacy treatment
- "View Invoice" link: `color-primary-600`, opens invoice detail page

### 2.2 Funding Details Block

```
┌─ FUNDING DETAILS ──────────────────────────────────────────────────┐
│                                                                    │
│  Funded Date           April 23, 2026                             │
│  Full Funding Achieved  April 28, 2026 at 2:15 PM                 │
│  Funding Source        3 Investors                                │
│  Payment Status        Received from Debtor  ✓                    │
│  Escrow Release        Automatic, April 28, 2026                  │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

- Investor count shown as aggregate only — individual investor names never displayed
- "Received from Debtor ✓" uses `color-success-500` checkmark

### 2.3 Settlement Calculation Block (Fee Breakdown)

This is the receipt-style breakdown. It uses a semantic `<table>` for accessibility.

```
┌─ SETTLEMENT CALCULATION ───────────────────────────────────────────┐
│                                                                    │
│  Invoice Amount                              $5,000.00            │
│  ─────────────────────────────────────────────────────            │
│  Service Fee (3%)                             -$150.00            │
│  ─────────────────────────────────────────────────────            │
│  ══════════════════════════════════════════════════════           │
│  Net Payout                                  $4,850.00  ◄ bold   │
│  ══════════════════════════════════════════════════════           │
│                                                                    │
│  Transferred to account ending in ••••5678                        │
│  Transfer Time: 2–4 business hours                                │
│  Transferred At: April 28, 2026, 3:45 PM                          │
│                                                                    │
│  [↗ View on Stellar Expert]                                        │
│                                                                    │
└────────────────────────────────────────────────────────────────────┘
```

**Table token mapping:**

| Row | Typography | Color | Notes |
|---|---|---|---|
| Invoice Amount | `text-body` / 400 | `color-neutral-900` | Gross amount |
| Fee line items | `text-body` / 400 | `color-neutral-500` | Indented, negative values in `color-danger-500` |
| Divider rows | `1px solid color-neutral-200` | — | `<tr>` with border-top only |
| Net Payout | `text-body` / **700** | `color-neutral-900` | Double top border to signal total |
| Currency values | **Data Mono** (`JetBrains Mono`) | — | All monetary amounts use mono font |

**Fee line item expansion:** If multiple fee types exist (e.g., service fee + early-settlement fee), each appears as a separate row. A "Fee breakdown" disclosure toggle can collapse them to a single "Total Fees" row on mobile.

---

## 3. Field Visibility Table

| Field | Source | Business | Investor | Admin | Privacy Treatment |
|---|---|---|---|---|---|
| Transaction ID | `settlement.id` | ✓ | ✗ | ✓ | Public identifier |
| Settlement Date | `settlement.completed_at` | ✓ | ✗ | ✓ | — |
| Invoice ID | `invoice.id` | ✓ | ✗ | ✓ | Links to own invoice |
| Invoice Amount | `invoice.amount` | ✓ | ✗ | ✓ | Gross before fees |
| **Debtor Name** | `invoice.debtor` | ✓ | **✗** | ✓ | Business-only; never shown to investors |
| Fee Breakdown | Calculated | ✓ | ✗ | ✓ | Item-by-item |
| Net Payout | `amount - fees` | ✓ | ✗ | ✓ | Emphasized in UI |
| Investor Count | Count of investments | ✓ | ✗ | ✓ | Aggregate only |
| **Bank Account** | `settlement.transfer_method` | ✓ (masked) | ✗ | ✓ | **Last 4 digits only** — `••••5678` |
| Settlement Status | `settlement.status` | ✓ | ✗ | ✓ | — |
| Stellar Tx Hash | `settlement.tx_hash` | ✓ | ✗ | ✓ | Explorer link; Data Mono |

---

## 4. Privacy & Masking Rules

### 4.1 Debtor Name

- Shown **only** to the authenticated business that owns the invoice.
- Never rendered in any investor-facing view, shared PDF link, or public URL.
- In server-rendered HTML, the field must be gated by role check before rendering — not hidden via CSS.
- Rationale: business needs debtor name to reconcile with their own A/R records (see §1343 of `business-dashboard.md`).

### 4.2 Bank Account Number

- Display format: `••••` + last 4 digits (e.g., `••••5678`).
- Full account number must never appear in the UI, PDF, JSON export, or logs.
- Bullet character: `•` (U+2022), not `*`, for visual consistency.

### 4.3 PDF Downloads

- Must be generated server-side with an authenticated, short-lived token.
- URL must not be guessable (no sequential IDs in the download path).
- PDF must not embed full bank account number — apply the same `••••XXXX` masking.
- PDF must include a "Confidential — for [Business Name] only" footer.

---

## 5. Export Affordances

### 5.1 Button Set

```
[↓ Download PDF]  [↓ Download JSON]  [⎙ Print]  [✉ Email Receipt]  [⧗ View Timeline]
```

On mobile (< 768px): collapse to a single **"Export"** button that opens a bottom sheet with the same options listed vertically.

### 5.2 Button States

| Button | Default | Loading | Success | Error |
|---|---|---|---|---|
| Download PDF | Primary outline | Spinner + "Generating…" | "✓ Downloaded" (2s) | "Failed — Retry" |
| Download JSON | Secondary outline | Spinner + "Preparing…" | "✓ Downloaded" (2s) | "Failed — Retry" |
| Print | Secondary outline | — | Opens print dialog | — |
| Email Receipt | Secondary outline | Spinner + "Sending…" | "✓ Sent" (2s) | "Failed — Retry" |
| View Timeline | Ghost / link | — | Navigates to timeline | — |

**Loading state rules:**
- Disable the button during loading (`opacity-40`, `cursor-not-allowed`).
- Show an inline spinner (`role="status"`, `aria-label` matching the action).
- Do not disable other export buttons while one is loading.

### 5.3 Download Button Anatomy

```
┌──────────────────────────────┐
│  ↓  Download PDF Receipt     │  ← icon + label
└──────────────────────────────┘
     ↑ 16×16 SVG, aria-hidden
```

- `aria-label`: `"Download settlement receipt #REC-20260428-001 as PDF"`
- On success: swap icon to checkmark for 2 seconds, then revert.
- `aria-live="polite"` region announces completion to screen readers.

### 5.4 JSON Export Schema

The JSON download must include all fields the business is entitled to see (per §3), structured as:

```json
{
  "receipt": {
    "id": "REC-20260428-001",
    "settlement_date": "2026-04-28T15:45:00Z",
    "status": "SETTLED",
    "invoice": {
      "id": "INV-8201",
      "debtor": "Acme Corp",
      "amount": 5000.00,
      "issue_date": "2026-04-15",
      "due_date": "2026-05-15",
      "description": "Professional Services, April 2026"
    },
    "funding": {
      "funded_date": "2026-04-23",
      "full_funding_at": "2026-04-28T14:15:00Z",
      "investor_count": 3,
      "escrow_release": "automatic"
    },
    "calculation": {
      "gross_amount": 5000.00,
      "fees": [
        { "label": "Service Fee (3%)", "amount": -150.00 }
      ],
      "net_payout": 4850.00
    },
    "transfer": {
      "account_last4": "5678",
      "transferred_at": "2026-04-28T15:45:00Z",
      "stellar_tx_hash": "abc123…"
    }
  }
}
```

- `debtor` field included (business-scoped endpoint).
- Full bank account number **never** included — `account_last4` only.
- `stellar_tx_hash` included for Immutable Proof (see `visual-direction.md` §1.1).

---

## 6. Print Stylesheet Considerations

- Use `@media print` to hide: nav bar, export button row, back link, page chrome.
- Show: all three receipt blocks, receipt header, a "Printed on [date]" footer.
- Ensure `color-neutral-200` borders print (use `border-color` not `box-shadow`).
- Net Payout row: bold weight must survive print (avoid `font-weight` via utility classes that may be stripped).
- Page break: avoid breaking inside a block (`break-inside: avoid`).
- Font: fall back to system serif for print if JetBrains Mono is not embedded.

---

## 7. Accessibility Requirements

| Requirement | Implementation |
|---|---|
| Fee breakdown table | `<table>` with `<caption>`, `<th scope="row">` for labels, `<td>` for values |
| Net Payout emphasis | `<strong>` or `font-bold` + `aria-label="Net payout: $4,850.00"` on the cell |
| Download buttons | Descriptive `aria-label` including receipt ID and format |
| Loading spinners | `role="status"`, `aria-label` matching the action |
| Success feedback | `aria-live="polite"` region for "Downloaded" / "Sent" messages |
| Back link | Visible focus ring (`color-primary-600`), descriptive text |
| Status badge | Not color-only — include text label alongside color |
| Masked bank number | `aria-label="Account ending in 5678"` on the masked display |

---

## 8. Visual QA Checklist

- [ ] All three blocks render at **375px** (stacked, full-width)
- [ ] All three blocks render at **1280px** (max-w-3xl centered)
- [ ] Currency values use JetBrains Mono (Data Mono token)
- [ ] Net Payout row is visually distinct (bold, double border)
- [ ] Bank account shows `••••5678` — never full number
- [ ] Debtor name absent from any investor-role rendering
- [ ] Export buttons collapse to "Export" bottom sheet at 375px
- [ ] Download loading state disables only the clicked button
- [ ] Success feedback auto-resets after 2 seconds
- [ ] Print preview hides nav and export row; shows receipt blocks only
- [ ] All interactive elements have visible focus rings
- [ ] Fee breakdown table has `<caption>` and `<th scope="row">` headers

---

## 9. Future Component Placement

```
quicklendx-frontend/
  app/
    business/
      settlements/
        [id]/
          page.tsx          ← receipt detail page (placeholder exists)
```

The full implementation will require:
- Authenticated API call to `GET /api/settlements/:id` (business-scoped)
- Server-side PDF generation endpoint (authenticated token, not guessable URL)
- JSON download via `Blob` + `URL.createObjectURL`
- Print via `window.print()` with `@media print` stylesheet
