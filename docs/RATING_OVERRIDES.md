# Invoice Rating Overrides: Policy and Process

This document is written for the **Support Team and Operators**. It outlines the tribal knowledge regarding when and how invoice ratings can be overridden, who is authorized to do so, and the required review cadence.

Because QuickLendX smart contracts are designed to make on-chain `InvoiceRating` entries append-only (there is no `override_rating` or `remove_rating` function in the contract), all rating overrides are handled **off-chain** via the indexing database and frontend filtering layers.

## When to Use Rating Overrides

Ratings should only be overridden or hidden in the following exceptional circumstances:
- **Abusive or Harassing Language:** The feedback text contains hate speech, harassment, or doxxing.
- **Provable Spam / Bot Activity:** A coordinated attempt to manipulate a business's average rating using automated accounts.
- **Factually Incorrect Data:** The rating references the wrong business, the wrong invoice, or a dispute that was objectively ruled in favor of the business.

Overrides should **never** be used simply because a business is unhappy with a poor, but honest, review from an investor.

## Who Can Execute an Override

Only the **Trust & Safety Operations Team** has the authorization to flag an on-chain rating as "hidden" in the backend database. 

Engineers should not manually patch the database for rating disputes. Support staff must use the internal admin dashboard (under *Moderation -> Ratings*) to flag the specific rating ID for override. This ensures an audit trail is maintained.

## Review Cadence

To ensure overrides are not being abused and to identify recurring abusive investors:
- **Weekly Triage:** The Support Team reviews all pending rating dispute tickets from businesses.
- **Monthly Audit:** The Trust & Safety lead reviews a random sample of 10% of all overridden ratings from the previous month to ensure they strictly adhered to the "When to Use" criteria.

## Process for Support Agents

When a business opens a ticket regarding a rating:
1. Verify the invoice ID and the specific rating/feedback in question.
2. Determine if the feedback violates the terms of service (Abuse, Spam, Factually Incorrect).
3. If it violates the policy, locate the rating in the Admin Dashboard and toggle `Hide from UI`. 
4. Reply to the business confirming the action. (Do not page an engineer to alter the smart contract state; the on-chain data remains immutable for audit purposes, but the frontend will respect the backend's hidden flag).
