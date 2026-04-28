# Grace Period and Default Status UX Specification

## Purpose

This specification defines how QuickLendX should communicate overdue invoices,
grace periods, default risk, and recovery actions across product surfaces. It is
UX guidance only. It does not change contract behavior, settlement rules,
notification delivery, or frontend implementation.

The goal is to make default-related states clear, actionable, and accurate
without implying that QuickLendX can guarantee repayment, recovery, insurance
coverage, notification delivery, or a particular legal outcome.

## Source of Truth

Product copy and status timing must be based on contract state and indexed
events, not local UI assumptions.

Relevant protocol facts:

- An invoice becomes overdue after `now > due_date`.
- An invoice becomes defaultable after `now > due_date + grace_period`.
- The default grace period is 7 days unless protocol configuration or an
  explicit operator call uses a different value.
- The exact grace boundary is strict: an invoice is not defaultable at the
  deadline timestamp itself, only after it.
- Only funded invoices can be defaulted.
- Default marking is an admin/operator action, not an automatic user promise.
- A defaulted invoice is terminal for normal payment, refund, and default flows.
- Insurance handling, when available, is protocol-controlled and must not be
  described as guaranteed recovery.

## Audience Goals

### Business

Businesses need to know:

- whether an invoice is still on time, overdue, in grace, defaultable, or
  defaulted;
- the exact payment deadline and grace deadline shown in local time with timezone;
- what actions may reduce risk before default;
- that action availability depends on contract state and successful transaction
  processing.

### Investor

Investors need to know:

- whether a funded invoice is on schedule, overdue, in grace, defaultable, or
  defaulted;
- that a grace period is time for resolution, not a promise of repayment;
- what has happened on-chain and what is still pending;
- whether insurance or recovery processes are present without overstating
  certainty.

### Operator/Admin

Operators need to know:

- which invoices are defaultable now;
- which invoices are still in grace and cannot yet be marked defaulted;
- scan progress and whether a list is complete or only a bounded batch;
- the specific reason a default action is unavailable.

## Status Model

| UX Status | Contract basis | User meaning | Primary tone |
| --- | --- | --- | --- |
| On time | `Funded` and `now <= due_date` | Payment due date has not passed | Neutral |
| Overdue, in grace | `Funded`, `now > due_date`, and `now <= due_date + grace_period` | Payment is late, but default cannot yet be marked | Warning |
| Defaultable | `Funded` and `now > due_date + grace_period` | Grace period has expired; operator may mark default | Urgent |
| Default pending action | Off-chain/operator queue or scan found eligible invoice, if available | Default action may be processed after review/automation | Urgent, qualified |
| Defaulted | Invoice status `Defaulted` | Invoice reached terminal default state | Critical, factual |
| Not default eligible | Not funded, paid, cancelled, refunded, verified, pending, or invalid | Default action does not apply | Neutral or blocked |

Do not invent intermediate statuses that sound contractual unless they map to a
real data source. Labels such as "collection started", "guaranteed payout", or
"insurance approved" require separate verified state.

## Timeline Communication

Each funded invoice detail view should be able to present these timestamps:

- Due date: the original invoice due date.
- Grace period: duration currently applied to the invoice view.
- Grace deadline: `due_date + grace_period`.
- Current status timestamp: current indexed or ledger-derived reference time.
- Default marked date: event or status update time when the invoice is defaulted.

Recommended timeline labels:

| Timeline point | Label | Supporting copy |
| --- | --- | --- |
| Before due date | Payment due | "Payment is due by this date." |
| After due date, before grace deadline | Grace period active | "Payment is overdue. Default cannot be marked until the grace deadline passes." |
| After grace deadline, before default | Eligible for default review | "Grace period has ended. An authorized operator may mark this invoice as defaulted." |
| After default event | Defaulted | "This invoice has been marked defaulted on-chain." |

Use exact dates and times. Avoid relative-only copy like "soon" or "today" for
default-critical deadlines. Relative text may be used as a supplement, for
example: "Grace period ends May 5, 2026, 09:00 UTC (in 2 days)."

## Warning Levels

### On Time

Purpose: keep repayment expectations visible without creating alarm.

Recommended copy:

- "Payment due May 1, 2026, 09:00 UTC."
- "No default action is available before the due date and grace period pass."

Avoid:

- "Safe"
- "Guaranteed on time"
- "No risk"

### Overdue, In Grace

Purpose: communicate urgency while preserving the borrower's opportunity to
resolve the overdue payment.

Recommended copy:

- "Payment is overdue and the grace period is active."
- "Default cannot be marked until after May 8, 2026, 09:00 UTC."
- "Resolve payment before the grace deadline to reduce default risk."

Avoid:

- "You have 7 free days"
- "No consequences during grace"
- "Default will not happen"

### Defaultable

Purpose: state that default is permitted, not guaranteed or already complete.

Recommended copy:

- "Grace period has ended. This invoice is eligible for default review."
- "An authorized operator may mark this invoice as defaulted."
- "Payment or recovery options may be limited by current contract state."

Avoid:

- "This will default automatically"
- "Investor recovery is guaranteed"
- "Insurance will pay"

### Defaulted

Purpose: clearly state final state and direct users to available records or
support workflows.

Recommended copy:

- "This invoice has been marked defaulted."
- "Normal settlement and refund actions are no longer available for this invoice."
- "Review transaction history, insurance status, and support options for next steps."

Avoid:

- "Funds are recovered"
- "Claim approved"
- "Loss resolved"

## Recovery Actions

Recovery actions must be framed as possible next steps, not promises. Show only
actions supported by current state, role, and available integrations.

### Business Actions

Before default:

- Pay the outstanding amount through the supported settlement flow.
- Contact support if payment was sent but not reflected.
- Review invoice details, due date, and amount owed.

After default:

- Review default record and transaction history.
- Contact support or follow dispute/recovery workflows if available.
- Do not show normal payment CTAs unless the protocol has a verified post-default
  remediation path.

### Investor Actions

Before default:

- Monitor payment and grace timeline.
- Review risk disclosures and invoice history.
- Avoid copy that suggests the investor can force default directly unless an
  authorized operator flow exists.

After default:

- Review default event and investment status.
- Review insurance status if the investment has coverage.
- Contact support or follow documented recovery workflows.

### Operator Actions

Before defaultable:

- Show why default is blocked, such as "Grace period has not expired."
- Show the exact grace deadline.

Once defaultable:

- Allow default review only for authorized operators.
- Confirm invoice ID, business, investor, amount, due date, grace deadline, and
  current status before submission.
- Present finality warning before marking defaulted.

After default:

- Show the terminal status and event reference.
- Do not offer repeat default actions.

## Notification Guidance

Notifications should be short, factual, and action-oriented. They must not expose
unnecessary sensitive invoice detail in notification text or event payloads.

Recommended notification moments:

| Moment | Recipient | Priority | Message intent |
| --- | --- | --- | --- |
| Due date approaching | Business | Medium | Remind business of upcoming payment deadline |
| Due date passed | Business and investor | High | State overdue status and grace deadline |
| Grace midpoint | Business | High | Remind that default risk increases as deadline approaches |
| Grace deadline passed | Business, investor, operator | High | State default eligibility and next review/action |
| Default marked | Business and investor | High | State terminal default status and where to review records |

Required notification qualifiers:

- Delivery may be delayed, disabled by preferences, or fail.
- In-app status should remain the authoritative product surface.
- Time-sensitive copy should include exact timestamps.

Avoid sending repeated high-priority warnings without new information. Duplicate
warnings can create panic and reduce trust.

## Copy Rules

Use:

- "overdue"
- "grace period"
- "eligible for default review"
- "may be marked defaulted"
- "marked defaulted"
- "insurance status"
- "recovery options may be available"

Avoid:

- "guaranteed"
- "risk-free"
- "automatic recovery"
- "certain payout"
- "insurance will cover this"
- "default is cancelled"
- "funds are safe"

For dates, always include enough precision for the action:

- Good: "Grace period ends May 8, 2026, 09:00 UTC."
- Good: "Default may be marked after May 8, 2026, 09:00 UTC."
- Avoid: "Default tomorrow."

## Error and Empty States

Map protocol reasons to user-safe messages:

| Protocol condition | UX message |
| --- | --- |
| Invoice not found | "Invoice could not be found. Check the invoice ID or refresh." |
| Not funded | "Only funded invoices can enter the default process." |
| Already defaulted | "This invoice has already been marked defaulted." |
| Grace not expired | "Grace period is still active. Default cannot be marked yet." |
| Unauthorized | "You do not have permission to perform this action." |
| Scan batch incomplete | "This view reflects the current scan batch. More funded invoices may remain to be checked." |
| Data stale | "Status may be out of date. Refresh before taking action." |

Do not expose raw error names as the only explanation. Raw codes may be included
for support diagnostics after a plain-language message.

## Security and Trust Requirements

UX must preserve these security assumptions:

- Do not imply that warnings are delivered exactly once or exactly on time.
- Do not imply that an invoice defaults automatically at the grace deadline.
- Do not imply that an investor can directly mark a default unless the protocol
  exposes that permission.
- Do not imply that insurance coverage guarantees full or immediate repayment.
- Do not hide terminal-state finality after default.
- Do not show payment, refund, or settlement CTAs for defaulted invoices unless a
  separate verified recovery flow exists.
- Do not include sensitive invoice metadata in push/email/SMS previews.
- Do not rely on client clocks for authoritative eligibility decisions.

## Accessibility and Tone

- Use plain language before protocol terms.
- Pair color with text and icons; never rely on color alone.
- Use warning color for grace-period risk and critical color only for defaulted
  or defaultable states.
- Keep default messaging calm and specific. Avoid blame-oriented phrasing such as
  "You failed to pay."
- Confirm destructive or final operator actions with a review screen.

## Review Notes and Decisions

- Decision: Treat grace period as a risk window, not a relief guarantee.
- Decision: Use "eligible for default review" for post-grace, pre-default state
  to avoid implying automatic default.
- Decision: Keep "Defaulted" reserved for actual contract status or confirmed
  indexed event.
- Decision: Keep recovery copy conditional because insurance, support, and legal
  recovery outcomes are not guaranteed by the UX.
- Decision: Operator scan results must disclose bounded or stale data when the
  view may not cover all funded invoices.

## Validation

Docs-only validation performed:

- Cross-checked against `docs/contracts/defaults.md`.
- Cross-checked against `docs/contracts/default-handling.md`.
- Cross-checked against `docs/contracts/notifications.md`.
- Confirmed no frontend implementation task is included.
- Confirmed copy avoids misleading guarantees around repayment, insurance, and
  automatic default.

No automated tests are required for this UX deliverable.
