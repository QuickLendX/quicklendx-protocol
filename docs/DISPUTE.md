# Dispute Lifecycle

> Audience: **operators and contributors** who need to reason about
> invoice disputes without reading every smart-contract source file.
> For the complete entrypoint/error reference used by contract developers,
> see [quicklendx-contracts/docs/contracts/dispute.md](../quicklendx-contracts/docs/contracts/dispute.md).

## What this document covers

The QuickLendX protocol allows a dispute to be opened on any invoice by the
business owner or the funding investor. Once opened, a platform administrator
must review and formally resolve it before the invoice can return to normal
processing. This document explains:

- the permitted state transitions,
- who is authorised to drive each transition,
- the (lack of) on-chain timeouts, and
- how disputes affect funds and settlement.

## State machine

```
                  create_dispute(invoice_id, creator, reason, evidence)
     None ────────────────────────────────────────────────────────► Disputed
                                                                         │
                                                    update_dispute_evidence
                                                    (creator only, while Disputed)
                                                                         │
                                               put_dispute_under_review(admin)
                                             ◄──────────────────────────
                                                                         │
                                      resolve_dispute / resolve_dispute_structured
                                    ◄───────────────────────────────────────────
                                                                         │
                                                                      Resolved
                                                                     (terminal)
```

### Summary table

| Current status | Allowed action | Required role | Next status |
|----------------|---------------|---------------|-------------|
| `None` | `create_dispute` | Business owner or investor | `Disputed` |
| `Disputed` | `update_dispute_evidence` | Original creator only | `Disputed` (no change) |
| `Disputed` | `put_dispute_under_review` | Platform admin | `UnderReview` |
| `UnderReview` | `resolve_dispute` or `resolve_dispute_structured` | Platform admin | `Resolved` (terminal) |

`Resolved` is strictly terminal: any further mutation is rejected.

## Timers and deadlines

**There are no on-chain timeouts.** The dispute state machine is purely
action-driven — a dispute sits in `Disputed` or `UnderReview` indefinitely
until an authorised party transitions it. There is no ledger-time check in any
dispute entry point, and no automatic expiry.

> **Operational note** — Frontend tooling may display a color-coded deadline
> countdown to prompt admin review, but this is an off-chain UX cue only: it
> has no effect on the contract state.

## Who can open a dispute

A dispute may be opened by the **business owner** or the **funding investor**
of the invoice. The pre-conditions checked on-chain are:

1. **Pause gate** — `PauseControl::require_not_paused()` passes.
2. **Auth** — `creator.require_auth()` passes (Soroban on-chain signature).
3. **Eligibility** — the invoice status is one of `Pending`, `Verified`,
   `Funded`, or `Paid`. Disputes cannot be opened on `Defaulted` or
   `Cancelled` invoices.
4. **Authorization** — `creator` equals either `invoice.business` or
   `invoice.investor`.
5. **No existing dispute** — `invoice.dispute_status` must be `None`.
6. **Field lengths** — `reason` is 1–1 000 characters and `evidence` is
   1–2 000 characters (both non-empty and bounded).

```rust
client.create_dispute(
    &invoice_id,
    &creator_address,
    &String::from_str(env, "Payment not received after due date"),
    &String::from_str(env, "Transaction ID: ABC123 on 2025-01-15"),
);
```

If any pre-condition fails, the call returns an error and no state is
modified.

## Who can resolve a dispute

Resolution is **platform-admin only**. The admin must first move the dispute
to `UnderReview`, then resolve it:

```rust
// Step 1 — mark as under review (signals investigation is in progress)
client.put_dispute_under_review(&invoice_id, &admin_address);

// Step 2 — resolve with a structured outcome
client.resolve_dispute_structured(
    &invoice_id,
    &admin_address,
    &DisputeResolution::FavorBusiness,
    &String::from_str(env, "Payment confirmed late; business retains funds"),
);
```

Two resolution functions exist:

| Function | Use when |
|-----------|----------|
| `resolve_dispute` | You only need to record a free-text resolution. The structured outcome is set to `None`. |
| `resolve_dispute_structured` | You also want to record a machine-readable outcome (see below). |

### Structured outcomes

`resolve_dispute_structured` accepts a `DisputeResolution` enum:

- `FavorBusiness` (code `1`) — the business's position is upheld.
- `FavorInvestor` (code `2`) — the investor's position is upheld.
- `Split` (code `3`) — funds or obligations are split between parties.
- `Dismissed` (code `4`) — the dispute is closed without finding for either side.

The struct outcome is stored in `invoice.dispute.resolution_outcome` so that
off-chain systems can branch on it programmatically.

## Evidence update window

The original creator may call `update_dispute_evidence` **only** while the
dispute is in `Disputed` status — once `put_dispute_under_review` advances
the state, no further evidence changes are accepted. Evidence updates do not
emit an event and do not create a timeline entry; they simply replace the
stored string.

## What happens to funds

Opening a dispute **does not directly move funds or touch escrow**. However,
it blocks settlement:

- **Settlement finalization is rejected** while `dispute_status != None`.
  The invoice stays in its current status (typically `Funded`); the settlement
  module checks the dispute status explicitly before finalising.
- **Partial payments may still be recorded** during a dispute. This preserves
  payment history for the investigation, but does not advance settlement.
- **Escrow operations follow the invoice status**, which in turn is gated by
  the dispute outcome:
  - `FavorInvestor` → admin should transition invoice to `Cancelled` or
    `Refunded`, unlocking `refund_escrow`.
  - `FavorBusiness` → invoice remains `Funded`; settlement and
    `release_escrow` can proceed normally once the dispute is resolved.
  - `Split` or `Dismissed` → platform policy determines the next steps.

## Entrypoint reference (operator cheat-sheet)

| Call | Role | Purpose |
|------|------|---------|
| `create_dispute(invoice_id, creator, reason, evidence)` | Business or investor | Open dispute |
| `update_dispute_evidence(invoice_id, creator, evidence)` | Original creator | Supplement evidence (while `Disputed`) |
| `put_dispute_under_review(invoice_id, admin)` | Admin | Signal investigation start |
| `resolve_dispute(invoice_id, admin, resolution)` | Admin | Finalize with free-text note |
| `resolve_dispute_structured(invoice_id, admin, outcome, note)` | Admin | Finalize with structured outcome |
| `get_invoice_dispute_status(invoice_id)` | Anyone | Read current status |
| `get_dispute_details(invoice_id)` | Anyone | Read full dispute record |
| `get_invoices_with_disputes()` | Anyone | List every invoice that ever had a dispute |
| `get_invoices_by_dispute_status(status)` | Anyone | Filter dispute index by status |
| `get_dispute_timeline(invoice_id, offset, limit)` | Anyone | Paginated redacted timeline |

## Error reference

| Error | Code | Symbol | When it occurs |
|-------|------|--------|----------------|
| `DisputeNotFound` | `1900` | `DSP_NF` | No dispute exists (`status` is `None`) |
| `DisputeAlreadyExists` | `1901` | `DSP_EX` | Duplicate dispute creation |
| `DisputeNotAuthorized` | `1902` | `DSP_NA` | Caller is not business or investor |
| `DisputeAlreadyResolved` | `1903` | `DSP_RS` | Terminal state mutation attempted |
| `DisputeNotUnderReview` | `1904` | `DSP_UR` | Resolve called with status ≠ `UnderReview` |
| `InvalidDisputeReason` | `1905` | `DSP_RN` | Reason or resolution empty / oversized |
| `InvalidDisputeEvidence` | `1906` | `DSP_EV` | Evidence empty / oversized |

## Related documentation

- [ESCROW](ESCROW.md) — How funds are locked and released.
- [quicklendx-contracts/docs/contracts/dispute.md](../quicklendx-contracts/docs/contracts/dispute.md) — Full contract-level entrypoint and field-length reference.
- [docs/dispute-timeline-invariants.md](dispute-timeline-invariants.md) — Executable invariant reference for timeline property tests.
- [quicklendx-contracts/docs/settlement-dispute-interaction.md](../quicklendx-contracts/docs/settlement-dispute-interaction.md) — How settlement is blocked and resumed across dispute boundaries.
