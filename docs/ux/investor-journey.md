# Investor Journey Map

**Version**: 1.0
**Status**: Active
**Last Updated**: 2026-05-31
**Document Owner**: Product & Design Team
**Scope**: UX specification only (no frontend implementation)
**Related**: [content-style-guide.md](content-style-guide.md), [risk-copy-guidelines.md](../%20ux/risk-copy-guidelines.md), [business-dashboard.md](business-dashboard.md), [tooltips.md](tooltips.md), [modals.md](modals.md)

---

## Table of Contents

1. [Overview](#overview)
2. [Personas](#personas)
3. [End-to-End Journey Map](#end-to-end-journey-map)
4. [Stage 1 — KYC & Onboarding](#stage-1--kyc--onboarding)
5. [Stage 2 — Marketplace Browse](#stage-2--marketplace-browse)
6. [Stage 3 — Place Bid](#stage-3--place-bid)
7. [Stage 4 — Portfolio](#stage-4--portfolio)
8. [Stage 5 — Returns (Expected vs Realized)](#stage-5--returns-expected-vs-realized)
9. [Stage 6 — Disputes & Defaults](#stage-6--disputes--defaults)
10. [Trust Messaging Framework](#trust-messaging-framework)
11. [Bid Expiry & Ranking Communication](#bid-expiry--ranking-communication)
12. [Security & Privacy](#security--privacy)
13. [Open Questions](#open-questions)

---

## Overview

### Purpose

This document maps the end-to-end experience of an **investor** on QuickLendX, from first-touch KYC through realized returns and adverse events (disputes, defaults). It is paired with [business-dashboard.md](business-dashboard.md), which covers the business-side journey.

The investor journey has three load-bearing UX challenges that this map addresses explicitly:

1. **Trust calibration**: investors must understand *expected* returns are estimates, not guarantees, and realized returns may differ.
2. **Bid mechanics transparency**: bid expiry, ranking, and acceptance rules must be visible *before* a bid is placed — not discovered after.
3. **Adverse-event honesty**: disputes and defaults must be surfaced with neutral, factual language (no blame, no false reassurance) per [content-style-guide.md](content-style-guide.md).

### Out of Scope

- Frontend component implementation (separate engineering issue)
- Visual design tokens (see [design-tokens.md](design-tokens.md))
- Business-side dashboard (see [business-dashboard.md](business-dashboard.md))
- KYC vendor selection or compliance/legal review (separate workstream)

### Key Principles

1. **No misleading guarantees.** All return figures labeled as estimates with the basis disclosed.
2. **Decision-time disclosure.** Risks, fees, and bid rules visible before commitment, not after.
3. **Symmetric truth.** Realized returns shown with the same prominence as expected returns.
4. **Neutral language for losses.** Defaults and disputes described as system states, not user failures.
5. **Aggregate, anonymized counterparty data.** Investors never see business/debtor PII.

---

## Personas

### Primary: Priya (Yield-Seeking Retail Investor)

- **Background**: Tech professional, $5K–$50K invested across DeFi and TradFi yield products
- **Behavior**: Logs in 2–3x/week, scans yields, places small bids across many invoices
- **Pain points**:
  - Has been burned by "guaranteed APY" marketing elsewhere
  - Distrusts dashboards that show only winners
  - Wants to know *why* a bid was rejected or expired
- **Goals**: Predictable risk-adjusted yield; clear default/recovery exposure

### Secondary: Marcus (Institutional / Fund Manager)

- **Background**: Allocator at a credit fund, deploys $100K+ per position
- **Behavior**: Bulk bidding, exports portfolio data weekly, needs audit-grade records
- **Pain points**:
  - Needs realized-vs-expected reconciliation for LP reporting
  - Requires evidence of dispute outcomes
- **Goals**: Yield benchmarking, default-rate visibility, exportable receipts

### Edge case: Lena (First-Time Investor)

- **Background**: New to invoice financing; came from a referral
- **Behavior**: Cautious; reads tooltips; bounces if onboarding feels opaque
- **Pain points**:
  - Doesn't know what "grace period" or "bid ranking" mean
  - Worried about losing principal
- **Goals**: Understand the product before risking funds; small first bid

---

## End-to-End Journey Map

```
┌────────────┐   ┌────────────┐   ┌────────────┐   ┌────────────┐   ┌────────────┐   ┌────────────┐
│ 1. KYC &   │ → │ 2. Browse  │ → │ 3. Place   │ → │ 4. Portfolio│ → │ 5. Returns │ → │ 6. Disputes│
│ Onboarding │   │ Marketplace│   │   Bid      │   │  Tracking   │   │  Realized  │   │ & Defaults │
└────────────┘   └────────────┘   └────────────┘   └────────────┘   └────────────┘   └────────────┘
     │                │                │                │                  │                │
  Trust:           Trust:           Trust:           Trust:             Trust:           Trust:
  "We collect      "Estimates,      "Bid may not     "Status is        "Realized        "Neutral
   what we need,    not promises"    win or may       point-in-time"    differs from     framing,
   and why"                          expire"                            estimated"       no blame"
```

Each stage below documents: entry criteria → user goal → screens/states → microcopy → risk disclosures → exit criteria.

---

## Stage 1 — KYC & Onboarding

### Entry Criteria
- User has created an account (email + password) but is not yet verified to bid

### User Goal
"Get verified quickly without giving up more than necessary, and understand what I'm signing up for."

### Sub-Stages

| Step | Screen | What the investor sees | What the investor does |
|------|--------|------------------------|------------------------|
| 1.1 | Welcome | Plain-language summary of QuickLendX; "How it works" 3-step explainer | Clicks "Start verification" |
| 1.2 | Identity | Government ID upload, selfie liveness check | Uploads ID |
| 1.3 | Address & tax | Country, address, tax ID (where required) | Fills form |
| 1.4 | Source of funds | Disclosure on funding source per AML rules | Selects + acknowledges |
| 1.5 | Risk acknowledgment | **Required**: explicit risk disclosures (see below) | Checks all boxes |
| 1.6 | Review status | Verification timeline; what happens next | Waits / lands on marketplace in read-only mode |

### Microcopy — Risk Acknowledgment (Stage 1.5)

The investor must check each item individually. Do not bundle into a single "I agree" checkbox.

```
Before you can bid, please acknowledge:

☐ Invoice financing involves risk. You could lose part or all of the
  amount you invest in any bid.

☐ Estimated yields are based on bid terms and historical performance.
  Actual returns may be lower, may be delayed, or may be zero.

☐ Invoices can become overdue, enter a grace period, or be marked
  defaulted. Recovery options may be available but are not guaranteed.

☐ Smart contracts are audited but not immune to bugs. Past performance
  does not guarantee future results.

☐ Funds held on QuickLendX are not a bank deposit and are not FDIC
  or SIPC insured.

[Continue]
```

**Rules**:
- Each checkbox is required — `[Continue]` disabled until all five checked.
- Language must match [risk-copy-guidelines.md](../%20ux/risk-copy-guidelines.md) verbatim where overlapping; do not soften.
- Acknowledgment timestamps + version of disclosure text logged for audit trail.

### Microcopy — Verification Status

| State | Copy |
|-------|------|
| Pending | "Verification in review. Typical decisions: within 24 hours." |
| Approved | "Verification complete. You can now place bids." |
| Additional info needed | "We need one more document to complete verification. [View details]" |
| Rejected | "We're unable to verify your account at this time. [Contact support]" |

**Do not** show internal reasons codes (e.g., "AML_HIGH_RISK_JURISDICTION") to the user. Map them to safe, action-oriented copy per [content-style-guide.md](content-style-guide.md).

### Exit Criteria
- KYC approved → bidding enabled
- KYC pending → marketplace viewable in read-only mode (browse but cannot bid)
- KYC rejected → account in suspended state with support contact

### Security & Privacy Notes
- KYC documents are stored encrypted at rest; never returned via API after upload
- Do not display partial KYC data (e.g., last 4 of SSN) anywhere on the dashboard
- Logging of KYC status changes goes to an audit channel, not application logs

---

## Stage 2 — Marketplace Browse

### Entry Criteria
- User is KYC-approved (or KYC-pending in read-only mode)

### User Goal
"Find invoices that match my risk/yield preferences, and understand each listing before I bid."

### Screen Layout

```
┌──────────────────────────────────────────────────────────────────────┐
│  Marketplace                            [Filters ▾]  [Sort ▾]        │
├──────────────────────────────────────────────────────────────────────┤
│  Filters: Yield ▾ | Term ▾ | Industry ▾ | Risk band ▾ | Currency ▾ │
│  Data last refreshed: May 31, 2026, 14:22 UTC                       │
├──────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌─ INV-8201 ─────────────────────────────────────────────────────┐  │
│  │ Amount: $5,000 · Term: 30 days · Currency: USDC               │  │
│  │ Estimated yield (est.): 4.2% APY  ⓘ                            │  │
│  │ Risk band: B  ⓘ      Funding progress: 60% ████████░░         │  │
│  │ Bid window closes: Jun 2, 2026, 18:00 UTC (in 2d 4h)         │  │
│  │ Active bids: 7                                                 │  │
│  │ [View details]   [Place bid]                                   │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                                                                       │
│  ┌─ INV-8200 ─────────────────────────────────────────────────────┐  │
│  │ Amount: $3,500 · Term: 45 days · Currency: USDC               │  │
│  │ Estimated yield (est.): 5.1% APY  ⓘ                            │  │
│  │ Risk band: C  ⓘ      Funding progress: 20% ███░░░░░░░         │  │
│  │ Bid window closes: Jun 1, 2026, 12:00 UTC (in 21h)           │  │
│  │ Active bids: 2                                                 │  │
│  │ [View details]   [Place bid]                                   │  │
│  └────────────────────────────────────────────────────────────────┘  │
│                                                                       │
│  [Load more]                                                          │
└──────────────────────────────────────────────────────────────────────┘
```

### Listing Fields

| Field | Source | Visibility | Notes |
|-------|--------|------------|-------|
| Invoice ID | `invoice.id` | Public | Pseudonymous; not the debtor identifier |
| Amount | `invoice.amount` | Public | Target funding amount |
| Term | `invoice.due_date - now` | Public | Display in days |
| Currency | `invoice.currency` | Public | Whitelisted set only |
| Estimated yield | derived from bid terms | Public | Always suffixed `(est.)` |
| Risk band | `invoice.risk_band` | Public | A/B/C/D bands; methodology linked from `ⓘ` |
| Funding progress | `funded_amount / amount` | Public | Aggregated, never per-investor |
| Bid window close | `bid_window_end` | Public | Absolute UTC + relative supplement |
| Active bids | count of open bids | Public | Aggregated count only |
| Debtor name | `invoice.debtor` | **Hidden** | Investor never sees debtor PII |
| Business name | `invoice.business_id` | **Hidden** | Investor sees only pseudonymous ID |

### Tooltip Copy (`ⓘ` indicators)

**Estimated yield (est.)**:
> "Estimated APY based on the bid terms and the remaining term of the invoice. This is not a guarantee. Actual returns may be lower, may be delayed, or may be zero if the invoice is not repaid."

**Risk band**:
> "Risk bands (A–D) reflect a model-based assessment of repayment likelihood at listing time. Risk bands are estimates and may not reflect current conditions. Bands do not guarantee outcomes."

**Funding progress**:
> "Share of the target amount currently committed across all accepted bids. Does not reflect individual investor positions."

### Filter Behavior

- Filters apply server-side; cached queries refresh on every filter change.
- Empty filter result: "No invoices match these filters. Try widening your range." with a single-tap "Clear all filters" action.
- Do **not** auto-suggest higher-risk invoices to compensate for an empty result — let the user adjust deliberately.

### Data Freshness

- Show `Data last refreshed: <UTC timestamp>` at the top of the list.
- If the cache is older than 5 minutes, show a soft refresh prompt: "Listings may be out of date. [Refresh]".
- Per [content-style-guide.md](content-style-guide.md): always show last sync time for any data displayed in tables/lists.

### Exit Criteria
- Investor clicks `[View details]` → Stage 3 entry (invoice detail)
- Investor clicks `[Place bid]` → Stage 3 (skip detail page, but bid sheet shows full disclosures)

---

## Stage 3 — Place Bid

### Entry Criteria
- KYC approved
- Selected an invoice in `Pending` status with an open bid window

### User Goal
"Place a bid I understand: my offered terms, my realistic chance of acceptance, what happens if I win or expire."

### Invoice Detail Screen

```
┌──────────────────────────────────────────────────────────────────────┐
│  INV-8201                                    Status: Accepting bids   │
├──────────────────────────────────────────────────────────────────────┤
│  Amount: $5,000        Term: 30 days        Risk band: B  ⓘ          │
│  Bid window closes: Jun 2, 2026, 18:00 UTC (in 2d 4h)               │
│  Funding progress: 60% (aggregate of accepted bids)                   │
│                                                                       │
│  ─ Recent bid activity ──────────────────────────────────────────    │
│  • 7 active bids                                                      │
│  • Bid amount range: $200 – $1,500                                    │
│  • Implied yield range: 3.8% – 5.6% APY (est.)                       │
│  (Individual investor identities are not shown.)                      │
│                                                                       │
│  ─ How bids are ranked ──────────────────────────────────────────    │
│  Higher offered yields to the business may be ranked lower; lower    │
│  fees and earlier submission may be ranked higher. See [Bid ranking  │
│  & expiry](#bid-expiry--ranking-communication).                       │
│                                                                       │
│  [Place bid]   [Save to watchlist]                                    │
└──────────────────────────────────────────────────────────────────────┘
```

### Bid Sheet (Modal)

```
┌─ Place bid: INV-8201 ──────────────────────────────────[×]─┐
│                                                              │
│  Your bid amount (USDC)                                      │
│  ┌────────────────────────┐                                  │
│  │ $ 1,000                │  Max available: $2,000           │
│  └────────────────────────┘                                  │
│                                                              │
│  Bid term: 30 days (matches invoice)                         │
│  Estimated yield (est.): 4.2% APY                            │
│                                                              │
│  ─ Fees ──────────────────────────────────────────────────  │
│  • Platform fee: 0.5% on realized returns                    │
│  • Network gas fee: paid to Stellar (varies)                 │
│                                                              │
│  ─ Bid lifecycle ─────────────────────────────────────────  │
│  • Submitted: now                                            │
│  • Expires if not accepted: Jun 2, 2026, 18:00 UTC          │
│  • If accepted: funds move to escrow until settlement        │
│  • If not accepted by expiry: funds remain in your balance   │
│                                                              │
│  ─ Risk reminder ─────────────────────────────────────────  │
│  ⚠️ Estimated yields are not guarantees. The invoice may    │
│     be paid late, paid partially, or marked defaulted.      │
│     You could lose part or all of this bid amount.          │
│                                                              │
│  ☐ I have read and accept the bid terms                     │
│                                                              │
│  [Cancel]                              [Submit bid]          │
└─────────────────────────────────────────────────────────────┘
```

**Critical rules**:
- `[Submit bid]` disabled until acknowledgment checkbox is ticked.
- Fees shown as **both** percentage and dollar example, per [risk-copy-guidelines.md](../%20ux/risk-copy-guidelines.md) Section 2.
- "Estimated yield" must always carry the `(est.)` suffix per [content-style-guide.md](content-style-guide.md).
- Bid expiry shown as absolute UTC; relative supplement is optional but never the sole format.
- Do **not** show realtime competing bid values during the bid-entry flow (would enable gaming).

### Bid Lifecycle States (Investor View)

| State | Display | Investor sees |
|-------|---------|---------------|
| Submitted | 🟡 Pending | Bid is live; expiry countdown |
| Accepted | 💰 Funded | Bid won; funds moved to escrow |
| Outranked | ⚪ Not accepted | "Your bid was not selected. Funds returned to your balance." |
| Expired | ⏱️ Expired | "Bid window closed before acceptance. Funds returned to your balance." |
| Withdrawn | ❌ Withdrawn | User-initiated; only allowed before any acceptance |
| Cancelled (invoice withdrawn) | ⚪ Cancelled | "The business withdrew this invoice. Funds returned." |

**Microcopy rules**:
- "Outranked" is shown to the investor as `Not accepted` — neutral, no implication of fault.
- Never display "You lost" or "Your bid was beaten" — uses competitive framing that misrepresents the matching process.
- Do not reveal which investor or what terms won.

### Exit Criteria
- Bid accepted → invoice appears in Portfolio (Stage 4) under `Active`
- Bid expired/outranked → funds returned; bid moves to Portfolio under `History`

---

## Stage 4 — Portfolio

### Entry Criteria
- Investor has at least one submitted bid (active, historical, or both)

### User Goal
"See where my money is, what it's doing, and what's next — without needing to think hard."

### Portfolio Overview

```
┌──────────────────────────────────────────────────────────────────────┐
│  Portfolio                              Data refreshed: 14:22 UTC    │
├──────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌─ Position summary ──────────────────────────────────────────────┐│
│  │  Available balance:        $12,300.00                            ││
│  │  Active bids (pending):    $2,400.00  across 4 bids              ││
│  │  In escrow (funded):       $8,750.00  across 7 invoices          ││
│  │  Estimated returns (est.): $312.45    over remaining term        ││
│  │  Realized returns (YTD):   $1,847.20  across 38 settled invoices ││
│  │                                                                  ││
│  │  Estimated vs realized comparison → [View details]               ││
│  └──────────────────────────────────────────────────────────────────┘│
│                                                                       │
│  ┌─ Active positions (7) ──────────────────────────────────────────┐│
│  │  Invoice    │ Bid amt   │ Status    │ Est. payout │ Due date    ││
│  │  INV-8201   │ $1,000    │ 💰 Funded │ $1,011.50   │ Jun 28      ││
│  │  INV-8198   │ $1,500    │ 💰 Funded │ $1,517.30   │ Jun 25      ││
│  │  INV-8150   │ $2,000    │ ⚠️ Disputed│ Held         │ —           ││
│  │  INV-8144   │ $750      │ ⚠️ Overdue │ Held         │ Past due    ││
│  │  ...                                                              ││
│  └──────────────────────────────────────────────────────────────────┘│
│                                                                       │
│  ┌─ Pending bids (4) ──────────────────────────────────────────────┐│
│  │  Invoice    │ Bid amt   │ Expires            │ Active bids       ││
│  │  INV-8210   │ $500      │ Jun 1, 12:00 UTC   │ 3                 ││
│  │  ...                                                              ││
│  └──────────────────────────────────────────────────────────────────┘│
│                                                                       │
│  [History]   [Export CSV]   [Download statements]                     │
└──────────────────────────────────────────────────────────────────────┘
```

### Position Summary Fields

| Field | Definition | Notes |
|-------|------------|-------|
| **Available balance** | Funds not committed to any bid; withdrawable | Real-time |
| **Active bids (pending)** | Sum of submitted bids awaiting acceptance/expiry | Real-time |
| **In escrow (funded)** | Sum of accepted bids on funded invoices, pre-settlement | Real-time |
| **Estimated returns (est.)** | Sum of expected returns on active positions, per bid terms | Always `(est.)` |
| **Realized returns (YTD)** | Sum of returns actually received on settled invoices | Year-to-date |

**Critical UX rule**: Estimated and Realized are shown together. Realized is not buried below the fold or in a separate section. This is the #1 trust signal in the portfolio view.

### Status Indicators (Investor View)

| Status | Icon | Display | Investor action |
|--------|------|---------|-----------------|
| Pending | 🟡 | Bid submitted, not yet accepted | Optional: withdraw before any acceptance |
| Funded | 💰 | Bid accepted; in escrow | Monitor; no action needed |
| Settled | ✓ | Invoice paid; returns received | Review receipt |
| Overdue | ⚠️ | Invoice past due date; in grace period | Monitor; see [Stage 6](#stage-6--disputes--defaults) |
| Disputed | ⚠️ | Dispute filed; payment held | Respond / await mediator |
| Defaulted | 🔴 | Invoice marked defaulted | Review recovery options (if any) |
| Expired | ⏱️ | Bid window closed without acceptance | Funds returned |
| Outranked | ⚪ | Bid not selected | Funds returned |
| Withdrawn | ❌ | Investor withdrew bid | Funds returned |

### Tooltip — Overdue & Grace Period

Per [content-style-guide.md](content-style-guide.md) approved terminology:

> "This invoice has passed its due date and is currently in its grace period. The grace period is a defined risk window, not a guarantee of repayment. If the grace period ends without payment, the invoice becomes eligible for default review."

**Never use**: "You have X free days", "No consequences during grace", "This will resolve automatically".

### Exit Criteria
- Investor drills into a position → settled receipt or dispute detail (Stage 5 or 6)
- Investor exports CSV → audit-grade record (Marcus persona)

---

## Stage 5 — Returns (Expected vs Realized)

### Entry Criteria
- At least one settled invoice in the investor's history

### User Goal
"Did my estimated yield match what I actually got? If not, why?"

This is the most reputation-critical surface in the product. It is also the easiest place to mislead with selective metrics. The rules below are non-negotiable.

### Returns Comparison View

```
┌──────────────────────────────────────────────────────────────────────┐
│  Returns — Expected vs Realized                                       │
├──────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  Period: Year to date (Jan 1 – May 31, 2026)         [Change period] │
│                                                                       │
│  ┌─ Aggregate ────────────────────────────────────────────────────┐ │
│  │  Settled positions:        38                                    │ │
│  │  Estimated returns (est.): $2,012.40   (weighted-avg APY: 4.4%)  │ │
│  │  Realized returns:         $1,847.20   (weighted-avg APY: 4.0%) │ │
│  │  Difference:               -$165.20   (-8.2%)                    │ │
│  │                                                                  │ │
│  │  Why the difference?                                             │ │
│  │  • 3 invoices repaid late (no penalty applied to investor)      │ │
│  │  • 1 invoice partially repaid after default → recovered 60%    │ │
│  │  • 2 invoices currently in dispute (excluded from realized)    │ │
│  │  [See per-invoice breakdown]                                    │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                       │
│  ┌─ Per-invoice reconciliation ───────────────────────────────────┐ │
│  │  Invoice  │ Est. APY │ Realized │ Outcome     │ Settlement      │ │
│  │  INV-8101 │ 4.5%     │ 4.5%     │ Paid in full│ Apr 28, 2026   │ │
│  │  INV-8088 │ 5.0%     │ 0%       │ Defaulted   │ Recovery: 60%  │ │
│  │  INV-8077 │ 4.2%     │ 3.8%     │ Paid late   │ +9 days        │ │
│  │  ...                                                             │ │
│  │  [Export CSV]                                                    │ │
│  └────────────────────────────────────────────────────────────────┘ │
│                                                                       │
│  Past performance does not guarantee future results.                  │
└──────────────────────────────────────────────────────────────────────┘
```

### Non-Negotiable Rules

1. **Realized is shown with equal prominence to Estimated.** Same font weight, same size, immediately adjacent. No collapsed sections.
2. **Difference is signed and labeled.** A negative delta is shown as `-$X (-Y%)` in the standard text color (not red panic) — neutral framing per [content-style-guide.md](content-style-guide.md).
3. **Reasons are enumerated.** When realized < expected, the system explains the contributing categories (late, partial, default, dispute).
4. **Disputed positions are excluded from realized.** Show them in a separate "Pending resolution" line so totals reconcile.
5. **Recovered amounts are labeled.** A defaulted invoice with 60% recovery shows `Realized: 60% of bid, Loss: 40% of bid`. Never roll the recovery silently into realized totals without breakdown.
6. **No cherry-picked time windows.** Default period is "All time" or "Year to date" — not "last 30 days" (which can be tuned to look favorable).
7. **Required disclosure footer.** Every page that shows return numbers ends with: *"Past performance does not guarantee future results."*

### Tooltip Copy

**Estimated APY (weighted-avg)**:
> "The principal-weighted average of estimated APYs across all settled positions in the selected period. Estimates reflect bid terms at acceptance. Not a guarantee."

**Realized APY (weighted-avg)**:
> "The principal-weighted average of actual returns received across all settled positions in the selected period. Includes partial recoveries from defaulted invoices. Excludes positions still in dispute."

**Difference**:
> "Realized minus estimated. A negative value means realized returns were below estimates. Common causes include late payment, partial recovery on default, or unfavorable currency movements."

### Settlement Receipt (Per Invoice)

```
┌─ Settlement receipt: INV-8101 ─────────────────────────────[×]─┐
│                                                                  │
│  Settled: April 28, 2026, 15:45 UTC                              │
│  Receipt ID: REC-20260428-8101-INV                               │
│                                                                  │
│  ─ Your position ──────────────────────────────────────────────  │
│  Bid amount:                 $1,000.00                           │
│  Term:                       30 days                             │
│  Estimated yield (est.):     4.5% APY  →  ~$3.70 expected        │
│                                                                  │
│  ─ Outcome ────────────────────────────────────────────────────  │
│  Invoice repaid:             In full, on time                    │
│  Gross return:               $3.70                               │
│  Platform fee (0.5%):        -$0.02                              │
│  Realized return (net):      $3.68                               │
│  Total returned to balance:  $1,003.68                           │
│                                                                  │
│  [Download PDF]   [Download JSON]                                │
└─────────────────────────────────────────────────────────────────┘
```

### Edge Case — Settlement Receipt for Defaulted/Recovered Position

```
┌─ Settlement receipt: INV-8088 ─────────────────────────────[×]─┐
│                                                                  │
│  Resolved: May 12, 2026, 10:00 UTC                               │
│  Receipt ID: REC-20260512-8088-INV                               │
│                                                                  │
│  ─ Your position ──────────────────────────────────────────────  │
│  Bid amount:                 $1,500.00                           │
│  Estimated yield (est.):     5.0% APY                            │
│                                                                  │
│  ─ Outcome ────────────────────────────────────────────────────  │
│  Status:                     Marked defaulted                    │
│  Recovery amount returned:   $900.00 (60% of bid)                │
│  Realized return:            -$600.00 (40% loss of principal)    │
│                                                                  │
│  Recovery options may continue to be available. We will notify   │
│  you of any further recoveries.                                  │
│                                                                  │
│  [Download PDF]   [Download JSON]                                │
└─────────────────────────────────────────────────────────────────┘
```

**Microcopy rules**:
- Use `Realized return: -$X` (signed) — do not hide the loss in fee math or net wording.
- Do **not** write "your funds are recovered" or "loss resolved" per [content-style-guide.md](content-style-guide.md) prohibited terms.
- Future recovery is described as *possible*, not promised: "Recovery options may continue to be available."

### Exit Criteria
- Investor downloads receipt for accounting
- Investor opens dispute or default detail (Stage 6)

---

## Stage 6 — Disputes & Defaults

### Entry Criteria
- One or more positions in `Disputed`, `Overdue`, or `Defaulted` state

### User Goal
"Tell me clearly what happened, what's being done, what I can do, and what to realistically expect."

### Disputes Overview (Investor Side)

```
┌──────────────────────────────────────────────────────────────────────┐
│  Disputes & defaults                                                  │
├──────────────────────────────────────────────────────────────────────┤
│                                                                       │
│  ┌─ Active dispute — INV-8150 ──────────────────────────────────┐   │
│  │  Status: Under review        Filed: Apr 26, 2026, 14:30 UTC   │   │
│  │  Your bid in this invoice: $2,000                              │   │
│  │  Position: held in escrow pending resolution                   │   │
│  │                                                                │   │
│  │  Why this is happening:                                        │   │
│  │  The business has been notified of a dispute against this      │   │
│  │  invoice. A neutral mediator is reviewing both sides.          │   │
│  │                                                                │   │
│  │  Expected timeline: 5–30 days from filing.                     │   │
│  │  Funds remain in escrow until the mediator decides.            │   │
│  │                                                                │   │
│  │  [View dispute] [Add evidence] [Contact support]               │   │
│  └────────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌─ Overdue — INV-8144 ─────────────────────────────────────────┐   │
│  │  Status: Overdue, in grace period                              │   │
│  │  Due date: May 24, 2026  · Grace ends: Jun 7, 2026, 09:00 UTC │   │
│  │  Your bid: $750                                                │   │
│  │                                                                │   │
│  │  After the grace period ends, the invoice becomes eligible    │   │
│  │  for default review. Recovery options may be available but    │   │
│  │  are not guaranteed.                                           │   │
│  │                                                                │   │
│  │  [View invoice] [Learn about grace period]                     │   │
│  └────────────────────────────────────────────────────────────────┘   │
│                                                                       │
│  ┌─ Defaulted — INV-8088 ───────────────────────────────────────┐   │
│  │  Status: Marked defaulted                                      │   │
│  │  Marked on: May 12, 2026, 10:00 UTC                            │   │
│  │  Your bid: $1,500 · Recovery to date: $900 (60%)              │   │
│  │                                                                │   │
│  │  Recovery options may continue to be available. We will       │   │
│  │  notify you of any further recoveries.                         │   │
│  │                                                                │   │
│  │  [View resolution timeline] [Download receipt]                 │   │
│  └────────────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────────┘
```

### Microcopy — State Definitions (Per [content-style-guide.md](content-style-guide.md))

| State | User-facing copy |
|-------|------------------|
| Overdue | "Payment is overdue. The invoice is currently in its grace period." |
| Grace ending | "Grace period ends on `<date UTC>`. After this, the invoice becomes eligible for default review." |
| Eligible for default review | "Grace period has ended. The invoice is eligible for default review and may be marked defaulted." |
| Marked defaulted | "This invoice has been marked defaulted. Recovery options may be available." |
| Dispute filed | "A dispute has been filed on this invoice. Funds are held in escrow pending mediator review." |
| Dispute resolved (in investor's favor) | "Dispute resolved. Funds released from escrow." |
| Dispute resolved (against investor) | "Dispute resolved. The mediator ruled in favor of the business. See receipt for details." |

### Required Prohibitions (Per Content Style Guide)

The following terms must **not** appear anywhere on dispute/default surfaces:
- "guaranteed", "guaranteed return", "risk-free", "safe", "funds are safe"
- "automatic recovery", "this will default automatically"
- "insurance will cover this", "claim approved"
- "default is cancelled", "default will not happen"
- "funds are recovered", "loss resolved"
- "you have X free days", "no consequences during grace"

### Investor Actions Available

| Action | Available when | Behavior |
|--------|----------------|----------|
| View dispute | Any disputed position | Read-only timeline + evidence (own + counterparty's permitted docs) |
| Add evidence | Investor opened the dispute, or mediator requested | File upload + text; all submissions logged |
| Contact support | Always | Routes to support inbox; SLA per support docs |
| Download receipt | After resolution or recovery event | PDF + JSON |
| Appeal | Mediator decision rendered; within appeal window | Triggers appeal flow (out of scope here) |

### Notifications

- Status changes (filed, under review, resolved, defaulted) → in-app alert + email
- Email subject lines use neutral framing: "Status update for invoice INV-8150" — **not** "Bad news about your bid".
- Emails do not contain dispute details (privacy); they link back to the in-app detail page.

### Exit Criteria
- Dispute resolves → settlement receipt (Stage 5) or recovery receipt
- Default marked → recovery flow begins; partial recoveries appended over time

---

## Trust Messaging Framework

This is the single most important section for product quality. Every screen in the investor journey must conform.

### The Four Anchors

| Anchor | Rule | Where it shows |
|--------|------|----------------|
| 1. **Label estimates as estimates** | Every forward-looking number carries `(est.)` and a tooltip explaining the basis | Marketplace, bid sheet, portfolio summary, per-position rows |
| 2. **Show realized next to expected** | Realized returns sit beside expected returns with equal visual weight | Portfolio summary, returns view, settlement receipts |
| 3. **Use neutral language for losses** | Late payment, default, dispute → factual system states, not user-blame language | Disputes & defaults stage, alerts, emails |
| 4. **Disclose at decision time** | Risks, fees, bid lifecycle visible *before* the user commits — not after | Bid sheet acknowledgment, KYC step 1.5 |

### Required Phrases

These phrases appear verbatim in the locations listed:

- **All return pages**: "Past performance does not guarantee future results."
- **All bid flows**: "You could lose part or all of this bid amount."
- **Risk acknowledgment**: "Estimated yields are based on bid terms and historical performance. Actual returns may be lower, may be delayed, or may be zero."
- **Footer / disclosures**: "Not a bank deposit — not FDIC or SIPC insured."

### Visual Treatment

- Estimated returns: standard text color (not green/optimistic).
- Realized returns: standard text color, equal weight to estimated.
- Negative deltas: standard text color with `-` sign — do **not** color-code in alarmist red. Red is reserved for explicit errors and active disputes (see [design-tokens.md](design-tokens.md)).
- Risk disclosures: high contrast, body-text size (not fine print).

### Anti-Patterns to Reject in Review

| Anti-pattern | Why it's rejected |
|--------------|-------------------|
| Realized returns collapsed under "Advanced" toggle | Buries the truth |
| "Expected APY: 4.2%" with no `(est.)` suffix | Implies certainty |
| Lifetime APY shown but loss-rate hidden | Selective metric |
| Default state shown as "Resolution in progress" | False reassurance |
| "Don't worry, recovery is likely" copy | Speculative; violates style guide |
| Time-window selector defaulting to "Best 30 days" | Cherry-picked |
| Green checkmark on a defaulted position with partial recovery | Misrepresents outcome |

---

## Bid Expiry & Ranking Communication

Bid mechanics are the area most likely to generate support tickets and trust loss if poorly explained. This section is the spec for how they are surfaced.

### Bid Window

Every invoice has a `bid_window_end` — the absolute UTC time after which no new bids are accepted. This is communicated in three places:

1. **Marketplace listing**: `Bid window closes: Jun 2, 2026, 18:00 UTC (in 2d 4h)`
2. **Invoice detail page**: same format, prominent header position
3. **Bid sheet (modal)**: full timeline including the expiry timestamp

Per [content-style-guide.md](content-style-guide.md): absolute UTC always; relative supplement optional.

### Bid Lifecycle (Per Bid)

A submitted bid has its own lifecycle independent of the invoice's bid window:

```
  Submitted ──► Accepted ──► (invoice settles via Stage 5)
      │
      ├──► Outranked     (another bid was selected; funds returned)
      ├──► Expired       (bid window closed without acceptance)
      ├──► Withdrawn     (investor cancelled before acceptance)
      └──► Cancelled     (invoice withdrawn by business)
```

### Ranking Communication

The platform's bid-ranking algorithm is **not** disclosed in full (preventing gaming), but the user-facing principles must be visible on the invoice detail page **before** a bid is placed:

```
─ How bids are ranked ──────────────────────────────────────
Bids are ranked using a combination of:
  • Offered yield to the business (lower yields rank higher
    for invoices seeking the lowest cost of funding)
  • Bid size relative to remaining funding need
  • Submission time (earlier submissions break ties)

Ranking does not depend on investor identity, history, or
account size. The full algorithm is not published in order
to prevent gaming. Individual investor bids are not visible
to other investors.

A higher rank improves the chance of acceptance but is not
a guarantee. Acceptance also depends on the business
accepting the bid before the bid window closes.
```

### Communicating Outcomes

| Outcome | In-app status | Copy shown to investor |
|---------|---------------|------------------------|
| Accepted | 💰 Funded | "Your bid was accepted. Funds are in escrow." |
| Outranked | ⚪ Not accepted | "Your bid was not selected. Funds returned to your balance." |
| Expired | ⏱️ Expired | "The bid window closed before your bid was accepted. Funds returned to your balance." |
| Withdrawn | ❌ Withdrawn | "You withdrew this bid. Funds returned to your balance." |
| Invoice cancelled | ⚪ Cancelled | "The business withdrew this invoice. Funds returned to your balance." |

**Prohibited copy for these outcomes**:
- "You lost", "Your bid was beaten", "Better luck next time"
- "Outbid", "Your offer was too high" (reveals competitor terms)
- Any phrasing that names or characterizes the winning bid

### Pre-Expiry Notifications

To respect investor attention without nagging:

- **24h before expiry**: in-app banner on portfolio page only ("4 bids expire within 24 hours")
- **No push/email** for pre-expiry — these are advisory states, not action-required
- **At expiry**: in-app status change; no notification (the lifecycle outcome is the notification)

### Tooltips

**On `Bid window closes` field**:
> "After this time, no new bids will be accepted on this invoice. Existing bids remain eligible for acceptance until the window closes or the invoice is fully funded."

**On `Active bids` count**:
> "The number of open bids currently on this invoice. Individual bid amounts and investor identities are not shown."

---

## Security & Privacy

### Data Exposure Matrix (Investor Surfaces)

| Data | Investor sees | Notes |
|------|---------------|-------|
| Invoice ID | ✓ | Pseudonymous; not the debtor identifier |
| Invoice amount, term, currency | ✓ | Required for bidding decision |
| Business identity | ✗ | Investor never sees business name |
| Debtor identity | ✗ | Never exposed to investors |
| Aggregate bid count | ✓ | Counts only |
| Bid amount range | ✓ | Range only (min/max), never individual values |
| Other investors' identities | ✗ | Always hidden |
| Other investors' bid amounts | ✗ | Always hidden (would enable gaming and information leakage) |
| Own bid history & receipts | ✓ | Scoped to authenticated account |
| Dispute counterparty identity | ✗ | Both parties anonymous to each other; visible to mediator only |
| Dispute evidence | ✓ (own + permitted) | Only docs each party uploads or mediator releases |

### Authentication & Access

- All investor endpoints require authenticated session (JWT) with KYC-verified flag
- Investor can only access their own positions, bids, and receipts (enforced server-side via subject claim)
- Read-only mode for KYC-pending users: marketplace browse only; bid endpoints return 403

### Logging

- Bid submissions, withdrawals, dispute submissions, and document uploads logged to an audit channel with timestamps and content hashes
- KYC and risk acknowledgments versioned and timestamped per acknowledgment event
- PII (KYC documents, tax IDs, addresses) must not appear in application logs

### Error Messages

Per [content-style-guide.md](content-style-guide.md), error messages must not expose system internals. Examples:

| Internal condition | User-facing message |
|--------------------|---------------------|
| `BID_WINDOW_CLOSED` | "Bid window has closed. Refresh to see current listings." |
| `KYC_NOT_VERIFIED` | "Your account is being verified. You can browse but not yet bid." |
| `INSUFFICIENT_BALANCE` | "Available balance is below the bid amount. Add funds to continue." |
| `INVOICE_WITHDRAWN` | "The business withdrew this invoice. Bid not accepted." |
| `RATE_LIMITED` | "Too many requests. Please wait a moment and try again." |

---

## Open Questions

These are intentionally deferred and require product/legal input before next iteration:

1. **Secondary market / bid transfer.** If an investor can sell a position before settlement, the realized-vs-expected math changes. Out of scope here.
2. **Tax document generation.** Year-end 1099-equivalent or jurisdiction-specific tax summaries. Touch points exist (settlement receipts, returns view) but full spec deferred.
3. **Push notifications.** Spec covers in-app and email; native push notifications need a separate spec covering opt-in defaults and quiet hours.
4. **Risk band methodology disclosure.** Tooltip references "model-based assessment" but the actual methodology page is not yet drafted.
5. **Recovery flow for defaulted invoices.** Stage 6 acknowledges partial recoveries but the recovery-attempt timeline and investor-visible steps are TBD.
6. **Insurance product surfacing.** If/when invoice insurance is available, surfacing it without violating prohibited terminology ("insurance will cover this") needs its own copy review.
7. **Multi-currency reporting.** Returns view assumes a single reporting currency; FX impact on realized vs estimated needs a column in the per-invoice reconciliation.

---

## Acceptance Checklist

For a future engineering implementation to be considered conformant with this spec:

- [ ] Every forward-looking yield/return number suffixed `(est.)` with tooltip
- [ ] Realized returns shown adjacent to estimated with equal visual weight on every relevant surface
- [ ] All five KYC risk acknowledgments individually checked; bundled checkbox not allowed
- [ ] Bid window close shown as absolute UTC + optional relative supplement
- [ ] Bid ranking principles visible on invoice detail page before bid placement
- [ ] No prohibited terminology present (automated lint where feasible against [content-style-guide.md](content-style-guide.md))
- [ ] No individual competing bid amounts or investor identities exposed on any investor surface
- [ ] Dispute and default surfaces use neutral, no-blame language per terminology glossary
- [ ] Required disclosure footer present on all return-display pages
- [ ] Data freshness timestamp visible on marketplace and portfolio
- [ ] Error messages mapped from internal codes to user-safe copy
- [ ] Settlement receipts available as PDF and JSON; defaulted/partial-recovery receipts show signed loss

---

**Document Version**: 1.0
**Last Updated**: 2026-05-31
**For questions**: product-team@quicklendx.io
