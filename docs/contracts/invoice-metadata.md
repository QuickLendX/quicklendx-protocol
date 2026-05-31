# Invoice Metadata and Categorization

This document details the implementation of invoice metadata, categorization, and tagging within the QuickLendX Protocol.

## Overview

Invoices now support extended metadata, categorization, and tagging to facilitate better discovery and management.

### Features

- **Metadata**: Structured optional data including Customer Name, Tax ID, Address, Line Items, and Notes.
- **Categorization**: Enum-based categorization (e.g., Services, Products, Technology).
- **Tagging**: Flexible string-based tags (up to 10 per invoice).
- **Indexing**: Efficient on-chain indexing allowing queries by category, tag, customer name, and tax ID.

## Data Structures

### InvoiceCategory

Enum representing the industry or type of invoice:
- `Services`
- `Products`
- `Consulting`
- `Manufacturing`
- `Technology`
- `Healthcare`
- `Other`

### InvoiceMetadata

Optional struct attached to invoices:
```rust
struct InvoiceMetadata {
    customer_name: String,
    customer_address: String,
    tax_id: String,
    line_items: Vec<LineItemRecord>,
    notes: String,
}
```

### Tagging

- Tags are strings (max 50 chars).
- Max 10 tags per invoice.
- Stored as `Vec<String>`.

## Contract Entrypoints

### `update_invoice_metadata`

Updates the optional metadata for an invoice.

```rust
fn update_invoice_metadata(env: Env, invoice_id: BytesN<32>, metadata: InvoiceMetadata)
```

### `add_invoice_tag`

Adds a tag to an existing invoice.

```rust
fn add_invoice_tag(env: Env, invoice_id: BytesN<32>, tag: String)
```

### `remove_invoice_tag`

Removes a tag from an invoice.

```rust
fn remove_invoice_tag(env: Env, invoice_id: BytesN<32>, tag: String)
```

### Queries

- `get_invoices_by_category(category)`: Returns invoices in a specific category.
- `get_invoices_by_tag(tag)`: Returns invoices with a specific tag.
- `get_invoices_by_customer(name)`: Returns invoices matching customer name.
- `get_invoices_by_tax_id(tax_id)`: Returns invoices matching tax ID.

## Access Control

All metadata write operations enforce **strict owner-only authorization**:

### Authorization pattern

```rust
// 1. Check caller matches invoice business owner
if self.business != *business {
    return Err(QuickLendXError::Unauthorized);  // code 1100
}
// 2. Require cryptographic proof of ownership
business.require_auth();
```

Both `update_metadata` and `clear_metadata` perform this check **before** any state mutation, ensuring:

- **No partial writes**: If authorization fails, zero fields are modified.
- **No information leakage**: Validation errors (invalid field lengths, etc.) are only reachable by the owner.
- **Index consistency**: Derived indexes (customer name, tax ID) only change when the owner successfully updates or clears metadata.

### Security invariants

| Invariant | Enforced by |
|-----------|-------------|
| Only business owner can update metadata | Address comparison + `require_auth()` |
| Only business owner can clear metadata | Address comparison + `require_auth()` |
| No partial state on auth failure | Auth check precedes all mutations |
| Indexes reflect owner-authorized state only | `InvoiceStorage::update()` reads from the Invoice struct, which is unchanged on auth failure |
| Validation runs after auth | Auth check is first in both methods |

### Test coverage

The `test_invoice_metadata.rs` module provides 13 tests covering:

- Owner update/clear success
- Non-owner update/clear rejection (`Unauthorized`)
- No partial writes on auth failure (both update and clear)
- Index creation, swap, and removal
- Validation failure atomicity
- Multiple attacker rejection
- Recovery after failed attack

## Issue: input-validation & metadata test headings

### Description

- `store_invoice`, `upload_invoice`, and `update_invoice_metadata` accept String fields (customer name, tax id, metadata) that feed search indexes (`get_invoices_by_customer`, `get_invoices_by_tax_id`, `search_invoices`).
- Add tests enforcing maximum string lengths and rejecting empty/oversized inputs so a single oversize field cannot bloat storage or break pagination.

### Requirements and context

- Reference contract entrypoints: `store_invoice`, `update_invoice_metadata`, `clear_invoice_metadata` and search helpers (`get_invoices_by_customer`, `invoice_search::search_invoices`).
- Security: enforce bounded string lengths; reject empty required fields.
- Tests: at-limit, over-limit, empty, unicode-boundary inputs.
- Document these limits in this file and add `///` comments in the contract source where limits are enforced.

### Suggested execution

- Create a branch: `git checkout -b feature/metadata-input-validation`.
- Extend tests: `src/test_string_limits.rs`, `src/test_invoice_metadata.rs`, and `src/test_input_matrix.rs` to assert length bounds on every string field and metadata path.
- Update this document with the field length limits and add inline comments in the source where limits are applied.
- Validate that search indexes still resolve after clearing metadata.

### Test and commit

- Run: `cargo test test_string_limits test_invoice_metadata test_input_matrix` and ensure all new cases pass.
- Cover edge cases: max-length, over-length, empty, multibyte characters.
- Include test output and a storage-bloat note in the PR description.

### Example commit message

`test(invoice): add string and metadata input-validation bounds coverage`

### Guidelines

- Minimum 95% test coverage for the invoice/metadata module.
- Clear documentation of field length limits in this file and inline source comments.
- Timeframe: 96 hours for completion from branch creation.

### Field length limits (enforced)

The contract enforces the following maximum lengths (defined in `protocol_limits.rs`):

- Customer name: 150 characters
- Customer address: 300 characters
- Tax ID: 50 characters
- Notes: 2000 characters
- Line item description: 1024 characters
- Tag: 50 characters (max 10 tags per invoice)

These limits are enforced by `validate_invoice_metadata` and related verification helpers. See `quicklendx-contracts/src/protocol_limits.rs` for the canonical values.

### Storage-bloat mitigation

- Metadata vectors (line items, tags, ratings) are bounded by explicit caps (e.g., `MAX_METADATA_LINE_ITEMS`, `MAX_INVOICE_TAGS`) to prevent unbounded storage growth.
- Validation rejects empty or oversized fields to avoid storing trivially large payloads.

### Test run summary

- Targeted tests executed: `test_string_limits`, `test_invoice_metadata`, `test_input_matrix` — all passed locally (unit tests compiled and ran; no failures).
- Test run produced compiler warnings only; no test failures.

Include this summary in the PR description along with test output and a storage-bloat note.

## Storage and Indexing

Invoices are indexed using `(Symbol, Key)` tuples in the contract storage:
- **Category Index**: `("cat_idx", category) -> Vec<InvoiceId>`
- **Tag Index**: `("tag_idx", tag) -> Vec<InvoiceId>`
- **Customer Index**: `("meta_c", customer_name) -> Vec<InvoiceId>`
- **Tax ID Index**: `("meta_t", tax_id) -> Vec<InvoiceId>`

This ensures O(1) complexity for retrieving collections, avoiding expensive scans.
