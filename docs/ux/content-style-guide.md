# Content Style Guide & Microcopy

## Overview

This guide defines the unified voice, terminology, and formatting conventions for QuickLendX. Our UI copy must be calm, precise, and devoid of misleading guarantees. This ensures that users—both businesses and investors—understand the financial risks, obligations, and system states accurately.

## Voice and Tone

**Calm and Objective**
Keep messaging specific and factual. Avoid panic-inducing or overly celebratory language. Financial tools should inspire confidence through precision, not hype.

**No Blame**
When issues occur (e.g., overdue payments), use neutral language describing the system state rather than blaming the user.

- **Use:** "Payment is overdue."
- **Avoid:** "You failed to pay."

**Plain Language First**
Use plain, accessible language before introducing protocol-specific terms. Keep sentences short and action-oriented.

## Terminology Glossary

### Approved Terms

- **"overdue"**: Use to describe an invoice that has passed its due date.
- **"grace period"**: The specific time window after a due date before an invoice becomes defaultable. Treat it as a risk window, not a relief guarantee.
- **"eligible for default review"**: Use for post-grace, pre-default state. Do not imply automatic default.
- **"may be marked defaulted"**: Indicates operator eligibility to default an invoice.
- **"marked defaulted"**: Terminal default state.
- **"insurance status"**: The current state of any attached insurance.
- **"recovery options may be available"**: A factual statement indicating potential, non-guaranteed next steps.
- **"Expected Yield"**: The anticipated return on an investment.

### Prohibited Terms

Do not use terms that imply absolute certainty or absence of risk.

- **"guaranteed"**, **"Guaranteed Return"**, **"Guaranteed on time"**
- **"risk-free"**, **"No risk"**, **"Safe"**, **"funds are safe"**
- **"automatic recovery"**, **"This will default automatically"**
- **"certain payout"**, **"guaranteed payout"**
- **"insurance will cover this"**, **"Insurance will pay"**, **"Claim approved"**
- **"default is cancelled"**, **"Default will not happen"**
- **"funds are recovered"**, **"Loss resolved"**
- **"You have X free days"**, **"No consequences during grace"**

## Formatting and Capitalization

**Sentence Case**
Use sentence case for all headers, buttons, and alerts (e.g., "Expected yield", "Eligible for default review", "Accept bid"). Capitalize only the first word and proper nouns.

**Data Freshness & Estimations**

- Always display the last sync time for invoice data.
- Explicitly label estimates with `(est.)` and provide a tooltip explaining the variables.

## Date and Time Phrasing

Use exact dates and times with timezones to prevent ambiguity around critical deadlines. Avoid relying solely on relative copy like "soon" or "today".

- **Format:** `Month Day, Year, HH:MM UTC`
- **Optional Relative Supplement:** You may append a relative time in parentheses.

**Examples:**

- **Use:** "Payment due May 1, 2026, 09:00 UTC."
- **Use:** "Grace period ends May 8, 2026, 09:00 UTC (in 2 days)."
- **Avoid:** "Default tomorrow."

## Error Handling and Empty States

Map protocol conditions to user-safe, action-oriented messages. Never expose raw error names or raw codes as the only explanation.

### Error Messages

Be specific, suggest recovery, and use an "Error:" prefix for accessibility.

- **Invoice Not Found:** "Invoice could not be found. Check the invoice ID or refresh."
- **Not Funded:** "Only funded invoices can enter the default process."
- **Already Defaulted:** "This invoice has already been marked defaulted."
- **Grace Not Expired:** "Grace period is still active. Default cannot be marked yet."
- **Unauthorized:** "You do not have permission to perform this action."
- **Scan Batch Incomplete:** "This view reflects the current scan batch. More funded invoices may remain to be checked."
- **Data Stale:** "Status may be out of date. Refresh before taking action."

### Empty States

Empty states should clearly state the situation and guide the user to the next logical action.

- **No Invoices:** "You haven't uploaded any invoices. Start by uploading your first invoice to begin fundraising."
- **No Active Disputes:** "You have no active disputes at this time."
