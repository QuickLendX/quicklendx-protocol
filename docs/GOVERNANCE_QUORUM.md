# Governance Quorum & Participation Weighting

This document outlines the quorum rules and participation weighting mechanics used in the QuickLendX governance model. It is intended for **contributors** to understand how governance proposals achieve validity and how voting power is distributed.

## 1. Quorum Rules

A proposal must reach a minimum threshold of total voting power to be considered valid, regardless of whether the votes are in favor or against. This ensures that a small minority cannot unilaterally push through changes when the broader community is inactive.

### Quorum Threshold
- **Current Minimum Quorum:** 15% of the total circulating supply of governance tokens must participate in a vote for it to be legally binding.
- **Dynamic Adjustments:** The quorum threshold can be adjusted by a meta-governance vote, but it is hardcapped to never fall below 10% or exceed 33% to prevent both centralization vulnerabilities and gridlock.

### Concrete Example: Achieving Quorum

Suppose the total circulating supply is 100,000,000 QLX tokens.
- **Required Quorum:** 15,000,000 QLX.
- **Scenario A:** A proposal receives 10,000,000 "Yes" votes and 1,000,000 "No" votes. Although it has a massive majority of "Yes" votes (over 90%), the total participation is only 11,000,000. **The proposal fails due to lack of quorum.**
- **Scenario B:** A proposal receives 8,000,000 "Yes" votes and 7,500,000 "No" votes. The total participation is 15,500,000. **The proposal passes.** (It met both the quorum requirement and the simple majority requirement).

## 2. Participation Weighting

Voting power is not strictly 1-to-1 with token balance in all scenarios; it is weighted by time-locks to incentivize long-term alignment with the protocol.

### Time-Weighted Voting (veTokenomics)
Users lock their QLX tokens to receive veQLX (vote-escrowed QLX), which represents their voting power.

- **Maximum Lock Period:** 4 years.
- **Weighting Multiplier:** 
  - 1 QLX locked for 4 years = 1.00 veQLX
  - 1 QLX locked for 2 years = 0.50 veQLX
  - 1 QLX locked for 1 year  = 0.25 veQLX
  - 1 QLX locked for 0 years (liquid) = 0.00 veQLX (no voting power without locking)

### Concrete Example: Calculating Vote Weight

*   **Alice** has 10,000 QLX and locks it for 4 years. Her voting power is `10,000 * 1.0 = 10,000 veQLX`.
*   **Bob** has 30,000 QLX but only locks it for 1 year. His voting power is `30,000 * 0.25 = 7,500 veQLX`.
*   Although Bob has 3 times more capital than Alice, Alice has more influence over the protocol because of her long-term commitment.

## Notes for Contributors

- **Smart Contract Implementation:** The time-weighted balance is calculated deterministically based on the ledger timestamp at the moment the proposal is created. This prevents flash-loan voting attacks. Ensure that any modifications to `get_votes()` account for this snapshot behavior.
- **No `std::`:** When modifying governance contracts, always adhere to `#![no_std]` and rely on the Soroban SDK.
