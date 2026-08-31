# QLX Onboarding Limits

Progressive limits applied to new businesses and investors while they build history on the QuickLendX protocol.

## Audience

This document is written for **operators** and **support** staff who need to explain or verify the progressive limits that apply to newly onboarded participants.

## Purpose

New businesses and investors start with conservative limits. These limits increase automatically (or via governance) as the participant builds a positive history of successful invoices, repayments, and dispute-free activity.

The goal is to:
- Reduce protocol risk from brand-new participants
- Allow legitimate users to grow their capacity over time
- Keep the rules transparent and auditable

## Limit Categories

| Category              | Description                                      | Initial Value (example) |
|-----------------------|--------------------------------------------------|-------------------------|
| Max Open Invoices     | Maximum number of concurrent open invoices       | 3                       |
| Max Invoice Amount    | Maximum size of a single invoice                 | 5,000 USDC              |
| Max Total Exposure    | Maximum total outstanding principal              | 15,000 USDC             |
| Max Concurrent Bids   | Maximum number of open bids an investor can place| 5                       |
| Max Position Size     | Maximum size of a single investment position     | 2,000 USDC              |

> Exact numeric values are stored in the contract’s configuration and may be updated via governance. Always check the on-chain parameters for the current numbers.

## Progression Rules

Limits increase based on the following signals (examples):

1. **Successful Invoice Completions**  
   After N invoices are fully repaid without dispute, the Max Invoice Amount and Max Total Exposure increase.

2. **Time on Platform**  
   After a minimum number of days with positive activity, concurrent invoice and bid limits are raised.

3. **Dispute History**  
   Any dispute that is resolved against the participant can freeze or reduce limits until a cool-down period ends.

4. **Governance Override**  
   Protocol administrators can manually raise or lower limits for a specific participant when justified.

## Worked Example

A new business is onboarded:

- Day 0: Max Open Invoices = 3, Max Invoice Amount = 5,000 USDC
- After 5 successful invoice cycles with no disputes:
  - Max Open Invoices → 5
  - Max Invoice Amount → 10,000 USDC
  - Max Total Exposure → 30,000 USDC

An investor starts with:

- Max Concurrent Bids = 5
- Max Position Size = 2,000 USDC

After consistent successful bids and repayments, both limits are raised by the progression rules.

## Related Documents

- `CAPS.md` – hard protocol-wide caps
- `INVESTOR_RISK_MODEL.md` – risk scoring that can influence limits
- `GOVERNANCE.md` – how parameters and individual overrides are changed

## Implementation Notes

The limits are enforced inside the Soroban contracts. Off-chain services should never assume a participant can exceed the on-chain limits.

When in doubt, query the contract for the current effective limits of a given business or investor address.
