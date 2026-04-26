# Dispute Management

The QuickLendX protocol provides a robust dispute resolution mechanism for invoices. Disputes can be raised by either the business owner or the investor associated with an invoice.

## Lifecycle of a Dispute

1.  **Disputed**: A dispute is initiated by an authorized party (business or investor).
2.  **Under Review**: A platform administrator moves the dispute to this status to signal that investigation is in progress.
3.  **Resolved**: An administrator provides a resolution and closes the dispute.

## Core Entities

### Dispute Status

- `None`: No dispute exists (default).
- `Disputed`: Dispute has been raised.
- `UnderReview`: Dispute is being investigated by an administrator.
- `Resolved`: Dispute has been closed with a resolution.

### Dispute Data

The `Dispute` structure contains:
- `created_by`: Address of the initiator.
- `created_at`: Ledger timestamp of creation.
- `reason`: Explanation for the dispute.
- `evidence`: Supporting evidence/links.
- `resolution`: Final resolution text (once resolved).
- `resolved_by`: Address of the administrator who resolved it.
- `resolved_at`: Ledger timestamp of resolution.

## Discovery & Indexing

Disputes are indexed in a centralized, append-only discovery index within `InvoiceStorage`. This avoids technical debt associated with shadowed local counters and ensures all disputes are discoverable by platform interfaces.

### Query Endpoints

- `get_invoices_with_disputes`: Returns all unique invoice IDs that have ever had a dispute.
- `get_invoices_by_dispute_status(status)`: Filters the dispute index by current status.
- `get_dispute_details(invoice_id)`: Retrieves the full dispute record for a specific invoice.

## Security Controls

- **Authorization**: Only the invoice owner (business) or the current investor can raise a dispute.
- **Admin Control**: Only platform administrators can move disputes to `UnderReview` or `Resolved`.
- **Integrity**: Disputes cannot be deleted once raised, ensuring a complete audit trail.
- **Payload Validation**: Reason, evidence, and resolution texts are validated against protocol limits (e.g., length).
