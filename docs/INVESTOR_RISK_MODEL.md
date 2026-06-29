# Investor Risk Score and Tier Derivation Model

## Overview

QuickLendX assigns each investor a numeric risk score and a derived tier label.
The score gates access to lending opportunities: higher-risk investors see fewer
(or lower-value) available invoices.

## Score Components

| Component | Weight | Notes |
|---|---|---|
| KYC level | 30% | Full KYC = 0 penalty; partial = 15; none = 30 |
| Historical default rate | 25% | (defaults / total_funded) × 100 |
| Portfolio concentration | 20% | Herfindahl index across active industries |
| Account age (days) | 15% | Capped at 730 days (2 years) |
| Average repayment delay | 10% | Mean days overdue on past positions |

Raw score = Σ(component × weight), range 0–100 (0 = lowest risk).

## Tier Mapping

| Tier | Score Range | Description |
|---|---|---|
| TIER_1 | 0–20 | Premium investor; access to all invoice sizes |
| TIER_2 | 21–45 | Standard investor; access up to $500 k per invoice |
| TIER_3 | 46–70 | Elevated risk; access up to $100 k; enhanced monitoring |
| TIER_4 | 71–100 | High risk; read-only access pending review |

## Recalculation Trigger

Scores are recalculated:
- On every new bid placement
- On every repayment received
- On admin manual re-score via `POST /admin/investors/:id/rescore`

Results are cached for 24 hours in Redis. The cached value is served for all
non-critical read paths (dashboard, portfolio view). Live recalculation is used
for bid placement and escrow lock.

## Auto-Suspension

An investor is automatically suspended (tier moved to `SUSPENDED`) when:
- Score > 85 for 3 consecutive recalculation cycles, OR
- A fraudulent KYC flag is raised by the compliance service

Suspension requires manual lift by an admin.
