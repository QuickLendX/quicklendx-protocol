# Invariant Self-Check Report Developer Guide

This document covers the admin-callable invariant self-check entrypoint (`invariant_self_check`) that provides a structured diagnostic report detailing cross-module invariant health.

## Overview

The `invariant_self_check` entrypoint aggregates cross-module and data-layer invariants into a single, read-only "heartbeat" list of `(check_name, passed, evidence)` rows. It is admin-gated and read-only, ensuring no state mutation can occur.

## Composed Invariant Checks

The diagnostic report executes the following 7 checks:

1. **`no_orphan_investments`**: Every entry in the active-investment index must carry `InvestmentStatus::Active`.
2. **`audit_chain_integrity`**: Every invoice's audit trail must hash-chain validate without missing or tampered entries.
3. **`solvency`**: Active principals must be positive, and funded amounts must not exceed face values.
4. **`storage_index_coherence`**: Every invoice must belong to exactly one status index corresponding to its actual record status.
5. **`sum_investments_le_sum_invoices`**: The sum of all active investments must be less than or equal to the sum of all invoice face values.
6. **`escrow_uniqueness`**: Every invoice ID has at most one associated escrow mapping, and any present escrow correctly references its invoice ID.
7. **`settlement_accounting_identity`**: For every settled (`Paid`) invoice, the recalculated platform fee and investor return must sum exactly to `total_paid` (`investor_return + platform_fee == total_paid`).

## Computational Complexity and Scan Cost

Because these invariants inspect global contract state, running them incurs a linear scan cost:

- **`no_orphan_investments`**: $O(N_{active})$ persistent storage reads.
- **`audit_chain_integrity`**: $O(N_{all} \times L_{audit})$ persistent storage reads, where $L_{audit}$ is the length of the audit chain per invoice.
- **`solvency`**: $O(N_{active} + N_{funded})$ persistent storage reads.
- **`storage_index_coherence`**: $O(N_{all})$ persistent storage reads.
- **`sum_investments_le_sum_invoices`**: $O(N_{active} + N_{all})$ persistent storage reads.
- **`escrow_uniqueness`**: $O(N_{all})$ persistent storage reads.
- **`settlement_accounting_identity`**: $O(N_{paid})$ persistent storage reads.

> [!WARNING]
> Due to the $O(N)$ scanning cost, this endpoint should only be called sparingly by admins or monitoring agents (e.g. during audits or incident triage) and not inside hot paths.
