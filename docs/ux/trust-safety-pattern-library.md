# Trust & Safety Pattern Library

## Purpose

This library defines reusable UX patterns and copy rules for irreversible
on-chain actions, confirmations, risk disclosures, and error recovery in
QuickLendX. It is a UX deliverable only: it sets product behavior, copy, and
review criteria without assigning frontend implementation work.

Use this library with:

- [content-style-guide.md](./content-style-guide.md)
- [modals.md](./modals.md)
- [defaults-spec.md](./defaults-spec.md)
- [../data-freshness-semantics.md](../data-freshness-semantics.md)

## Principles

| Principle | Requirement |
| :--- | :--- |
| Informed consent | Users must see the material consequence, actor, asset, amount, destination, and timing before they sign. |
| No misleading guarantees | Copy must never imply funds, yield, payment, insurance, or recovery are guaranteed. |
| Safe default | The easiest path must be to pause, review, cancel, or retry safely. |
| Proportional friction | Add more review steps only when the action has higher financial, operational, or irreversible impact. |
| Accessibility first | Warnings must be conveyed by text and structure, not color alone. Keyboard and screen reader users must receive the same risk information. |
| Recovery clarity | Errors must explain what happened, what changed, and what the user can do next. |

## Risk Levels

Classify every transaction-triggering action before designing confirmation copy.

| Level | Examples | Pattern |
| :--- | :--- | :--- |
| Informational | Refresh data, copy invoice ID, view audit trail | No confirmation. Use status text when helpful. |
| Low risk | Save draft metadata, dismiss non-critical alert | Inline confirmation or toast. No modal unless context is lost. |
| Medium risk | Submit invoice, place bid, update profile data used for eligibility | Review panel with clear values and standard confirmation. |
| High risk | Accept bid and fund escrow, release escrow, refund escrow, settle invoice, mark defaulted, resolve dispute | Confirmation modal with consequence list, fee/risk disclosure, and safe default focus. |
| Critical | Execute emergency withdrawal, rotate treasury/admin recipient, irreversible terminal state change, action moving funds to a new address | Two-step confirmation with typed acknowledgement or equivalent deliberate review. Show auditability and cooling/timelock status when available. |

Do not downgrade an action because it is common. Frequency does not reduce risk.

## Pattern 1: Irreversible On-Chain Action Confirmation

Use for actions that mutate contract state, move funds, create terminal states,
or require wallet signing.

### Required content

Every irreversible-action confirmation must include:

- Action name in plain language.
- Actor taking the action.
- Affected invoice, bid, escrow, dispute, or recipient.
- Token, amount, fee, and destination when funds move.
- Current state and resulting state.
- Whether the action can be undone.
- Expected wallet/network step.
- Data freshness timestamp when the decision depends on indexed data.
- Safe exit action labeled "Cancel" or "Go back".

### Layout order

1. Plain-language title: "Confirm escrow release"
2. One-sentence summary: "This sends 10,000 USDC from escrow to the business."
3. Key facts table: invoice ID, amount, recipient, fee, current status.
4. Consequence list: concrete state changes and irreversible outcomes.
5. Risk banner when funds, terminal states, stale data, or admin authority are involved.
6. Actions: safe secondary action first, final action second.

### Copy template

Title:

> Confirm [action]

Summary:

> You are about to [plain-language action] for [object ID].

Consequences:

- [Amount] [asset] will move from [source] to [destination].
- [Object] will change from [current status] to [new status].
- This action cannot be undone after it is confirmed on-chain.

Wallet step:

> Your wallet will ask you to sign this transaction. Review the wallet details before signing.

Primary button:

> [Verb] [object]

Avoid:

- "Proceed"
- "OK"
- "Funds are safe"
- "Guaranteed"
- "No risk"

## Pattern 2: Critical Two-Step Confirmation

Use when a wrong action could permanently route funds to the wrong address,
transfer protocol-controlled funds, or close off normal recovery paths.

### Required triggers

Critical confirmation is required for:

- Emergency withdrawal execution.
- Treasury or fee-recipient rotation confirmation.
- Admin rotation confirmation when exposed in the UI.
- Manual default marking when the action creates or contributes to a terminal state.
- Dispute resolution that releases funds or finalizes a claim.
- Any action where the destination address was newly entered in the same flow.

### Required interaction

Use at least one deliberate friction mechanism:

- Typed acknowledgement using a short exact phrase, such as `CONFIRM`.
- Re-entering the last 6 characters of the destination address.
- Separate review and sign steps with no pre-selected final action.
- Mandatory review of a timelock, expiration, or cancellation window.

Do not use countdown pressure, pre-checked consent, misleading button emphasis,
or disabled cancel buttons while the transaction is still unsigned.

### Copy template

Title:

> Final review required

Risk statement:

> This action can permanently move funds or change protocol authority. Check every value before signing.

Acknowledgement:

> Type `CONFIRM` to continue.

Button:

> Sign transaction

## Pattern 3: Risk Banners

Risk banners communicate material risk before the user acts. They must be
concise, specific, and visible near the decision point.

### Severity model

| Severity | Use when | Required content |
| :--- | :--- | :--- |
| Notice | User should know context but no immediate risk exists | State what is happening and where to review details. |
| Caution | Decision may depend on estimates, fees, or stale data | Name the uncertainty and the safer next step. |
| Warning | Funds, eligibility, deadlines, or terminal states may be affected | State the consequence and whether the action can be undone. |
| Critical | Wrong input can cause permanent loss, misrouting, or authority change | State the worst credible consequence in plain language. |

### Copy rules

Use:

- "You could lose part or all of these funds."
- "This action cannot be undone after on-chain confirmation."
- "Data last synced May 31, 2026, 20:15 UTC. Refresh before signing if values look wrong."
- "Audits reduce smart contract risk but do not remove it."

Avoid:

- "Your funds are safe."
- "This is risk-free."
- "Instant recovery."
- "No downside."
- "Guaranteed payout."

### Accessibility requirements

- Include visible text labels such as "Warning:" or "Critical risk:".
- Do not rely on color or icon-only meaning.
- Keep the banner in the focus order when it contains actionable links.
- Associate banner text with the confirmation using an accessible description.
- Meet WCAG AA contrast for text and controls.

## Pattern 4: Wallet Signing Handoff

Wallets are separate signing surfaces. The app must prepare users without
claiming it can control wallet behavior.

### Required copy

Before opening the wallet:

> Your wallet will show the transaction request. Confirm only if the amount, token, and destination match this screen.

While waiting:

> Waiting for wallet signature. No on-chain changes have been made yet.

After user rejects:

> Transaction was not signed. No on-chain changes were made.

After submission:

> Transaction submitted. Confirmation may take time depending on network conditions.

After confirmation:

> Transaction confirmed on-chain.

Do not say "complete" until confirmation is observed from the chain or trusted
indexer state with freshness shown.

## Pattern 5: Error Recovery

Error messages must separate user action, wallet action, network submission,
and on-chain result. This prevents users from retrying dangerous transactions
without understanding what changed.

### Error states

| State | User-facing message | Recovery |
| :--- | :--- | :--- |
| Validation blocked | "This action cannot be submitted because [specific reason]." | Fix the input or return to the previous step. |
| Wallet rejected | "Transaction was not signed. No on-chain changes were made." | Offer "Review again" and "Cancel". |
| Submission failed | "The transaction was not submitted to the network." | Offer retry after checking wallet/network state. |
| Pending unknown | "Transaction status is unknown. Do not submit again until status is checked." | Offer status refresh and block duplicate final action where possible. |
| On-chain failed | "The transaction failed on-chain. No successful state change was confirmed." | Show sanitized reason and safe next step. |
| Confirmed | "Transaction confirmed on-chain." | Show final state, transaction ID, and audit trail link. |

### Duplicate-submit guard copy

Use when a retry could double-submit or confuse state:

> We are checking the latest on-chain status. Do not submit this action again until the check finishes.

## Pattern 6: Stale Data and Indexer Lag

Financial decisions must disclose data freshness. Follow
[data-freshness-semantics.md](../data-freshness-semantics.md) when indexed data
is delayed.

### Required behavior

- Show last sync time near action review.
- Use a caution banner when data may be stale.
- Block high-risk and critical actions when the current state cannot be verified.
- Never imply that an indexed view is the source of truth over confirmed
  on-chain state.

### Copy template

> Warning: Data may be out of date. Last synced [date/time]. Refresh before signing this transaction.

For blocked actions:

> This action is paused until the latest on-chain status is verified.

## Pattern 7: Risk Disclosure for Financial Outcomes

Use where users compare yields, fees, insurance, defaults, or recovery options.

### Required disclosures

- Expected returns are estimates, not promises.
- Fees and network costs can change before signing.
- Insurance or recovery options may be limited, delayed, or unavailable.
- Default, dispute, and settlement outcomes depend on contract state and
  authorized actions.
- Smart contracts may contain bugs even after review.

### Compact copy

> Risk: Returns are estimates. You could lose funds. Smart contract and liquidity risks apply.

### Expanded copy

> Expected returns are estimates based on available data. Actual returns may be lower. You could lose part or all of your funds due to default, market, liquidity, smart contract, or operational risk.

## Pattern 8: Anti-Dark-Pattern Rules

The following are prohibited in QuickLendX trust and safety flows:

- Pre-checked acknowledgement boxes.
- Hiding fees, gas, destination addresses, or terminal-state consequences below the action button.
- Styling "Cancel" as visually unavailable when cancel is allowed.
- Using shame, urgency, or loss-framed manipulation to push signing.
- Saying an action is reversible because support may be able to help.
- Presenting estimates as final values.
- Using vague final buttons such as "OK", "Continue", or "Proceed" for fund-moving actions.
- Showing success before wallet signature, network submission, and on-chain confirmation are distinct.
- Auto-opening wallet signing before the user sees the confirmation review.

## Pattern 9: Review Checklist

Use this checklist during design, product, and security review.

- [ ] The action risk level is documented.
- [ ] The copy names the actor, object, amount, asset, destination, and resulting state.
- [ ] The confirmation says whether the action can be undone.
- [ ] Risk copy avoids guarantees and prohibited terms from the content style guide.
- [ ] Fees, estimates, and network costs are labeled clearly.
- [ ] Data freshness is shown where indexed state affects the decision.
- [ ] Safe action is available and receives default focus for dangerous confirmations.
- [ ] Critical actions include deliberate acknowledgement.
- [ ] Wallet signing, submission, pending, failed, and confirmed states have distinct copy.
- [ ] Error recovery avoids duplicate-submit risk.
- [ ] Warning meaning is not conveyed by color alone.
- [ ] Screen reader users receive the same risk and consequence information.

## Security Assumptions

These UX patterns rely on the following validated assumptions from protocol docs:

- On-chain confirmation is the authoritative source for final state.
- Contract tests cover key guarded flows, but tests and audits do not eliminate
  smart contract risk.
- Terminal states and fund movements may not be reversible through the UI.
- Emergency recovery and recipient rotation are exceptional, high-authority
  actions that require stronger user review.
- Indexer or cache data can lag behind chain state and must be disclosed before
  high-risk decisions.

If any assumption changes, this library must be reviewed before related UI copy
ships.

## Review Notes and Decisions

| Decision | Rationale |
| :--- | :--- |
| Require risk classification before confirmation design | Keeps friction proportional and reviewable. |
| Require explicit consequence lists for irreversible actions | Users need to understand state and fund changes before signing. |
| Require two-step confirmation for critical authority/fund-routing actions | Wrong destinations or authority changes can be permanent. |
| Treat wallet rejection separately from failed on-chain execution | Prevents misleading recovery copy and unsafe retries. |
| Block high-risk actions when state freshness cannot be verified | Stale indexed data can lead users to sign against outdated assumptions. |
| Prohibit guarantee language | Aligns product copy with security reality and avoids dark patterns. |
