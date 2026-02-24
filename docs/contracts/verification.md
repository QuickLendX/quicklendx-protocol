# Verification

This document describes investor verification, risk scoring, tiering, and investment limit controls.

## Overview

Investor verification is implemented in:
- `quicklendx-contracts/src/verification.rs`
- `quicklendx-contracts/src/lib.rs`

Core behavior:
- Investors submit KYC and move through `Pending`, `Verified`, or `Rejected`.
- Verified investors are assigned:
  - `risk_score`
  - `risk_level` (`Low`, `Medium`, `High`, `VeryHigh`)
  - `tier` (`Basic`, `Silver`, `Gold`, `Platinum`, `VIP`)
  - `investment_limit`
- Bid placement enforces these limits and risk restrictions.

## Risk Score and Tier

Risk score is calculated by `calculate_investor_risk_score` using:
- KYC payload completeness proxy (`kyc_data.len()`)
- historical default rate
- historical invested volume adjustment

Tier is derived by `determine_investor_tier` from:
- `risk_score`
- `total_invested`
- `successful_investments`

Risk level is derived by `determine_risk_level` from score bands:
- `0..=25` => `Low`
- `26..=50` => `Medium`
- `51..=75` => `High`
- `76+` => `VeryHigh`

## Investment Limit Calculation

`calculate_investment_limit(tier, risk_level, base_limit)` applies:
- tier multiplier (`Basic=1`, `Silver=2`, `Gold=3`, `Platinum=5`, `VIP=10`)
- risk multiplier (`Low=100%`, `Medium=75%`, `High=50%`, `VeryHigh=25%`)

Final limit:
- `base_limit * tier_multiplier * risk_multiplier / 100`

Security guards:
- non-positive `base_limit` resolves to `0`
- saturation arithmetic is used to prevent overflow

## Bid Validation Integration

`place_bid` and `validate_bid` enforce investor controls:
- investor must be verified
- bid amount must be within computed `investment_limit`
- additional risk caps apply:
  - `VeryHigh`: max `10_000`
  - `High`: max `50_000`

## Analytics Updates on Outcomes

`update_investor_analytics` is called from settlement/default paths in `lib.rs`:
- `settle_invoice(...)` updates investor analytics on successful settlement
- `handle_default(...)` updates investor analytics on default
- `mark_invoice_defaulted(...)` updates investor analytics on default

Tracked fields include:
- `total_invested`
- `total_returns`
- `successful_investments`
- `defaulted_investments`
- `risk_score`, `risk_level`, `tier`, and recalculated `investment_limit`

The recalculation preserves admin intent by deriving a stable base limit from the prior tier/risk-adjusted limit before applying new tier/risk factors.

## Query Endpoints

Supported investor queries:
- `get_investor_analytics(investor)` -> `InvestorVerification`
- `get_investors_by_tier(tier)` -> `Vec<Address>`
- `get_investors_by_risk_level(risk_level)` -> `Vec<Address>`

Only verified investors are returned by tier/risk list filters.

## Security Notes

- Admin checks are enforced for verification/rejection/limit updates.
- Status lists are deduplicated and updated atomically during state transitions.
- Rejected -> resubmitted investors are removed from rejected lists and moved cleanly to pending.
- Limit and risk enforcement occurs before bid acceptance.
