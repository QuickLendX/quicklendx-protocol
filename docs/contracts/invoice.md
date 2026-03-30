# Invoice Categories, Tags, and Due Date Validation

## Overview

The QuickLendX protocol supports invoice categorization and tagging to improve discoverability, filtering, and organization of invoices. Additionally, the protocol enforces due date bounds and strict authorization checks to ensure system security and risk management.

## Features

### Invoice Categories
Invoices can be assigned to one of the following predefined categories:
- **Services**, **Products**, **Consulting**, **Manufacturing**, **Technology**, **Healthcare**, **Other**.

Each invoice must have exactly one category, which can be updated after creation by the authorized owner.

### Due Date Bounds Validation
Strict bounds are enforced to prevent excessive risk:
- **Maximum Due Date**: Configurable via protocol limits (default: 365 days).
- **Minimum Due Date**: Must be greater than the current ledger timestamp.
- **Validation**: Applied during both `store_invoice` and `upload_invoice`.

### Invoice Tags (Normalized)
- **Maximum Tags**: 10 per invoice.
- **Normalization**: All tags are trimmed of whitespace and converted to lowercase (ASCII) before storage or indexing.
- **Duplicate Prevention**: Detection occurs on the *normalized* form.

---

## Security and Permissions Matrix

The following table defines which identities are authorized to invoke specific mutation functions.

| Function | Required Signer | Enforcement Mechanism |
| :--- | :--- | :--- |
| `store_invoice` | `business` | `business.require_auth()` |
| `upload_invoice` | `business` | `business.require_auth()` + Verification |
| `update_invoice_category` | `invoice.business` | `self.business.require_auth()` |
| `add_invoice_tag` | `invoice.business` | `self.business.require_auth()` |
| `remove_invoice_tag` | `invoice.business` | `self.business.require_auth()` |
| `verify_invoice` | `admin` | `admin.require_auth()` |

> **Authorization Flow**: The contract retrieves the original `business` address stored in the `InvoiceData`. It calls Soroban's `require_auth()` on that specific address to ensure only the creator can modify the metadata.

---

## Tag Normalization Logic

Normalization is applied at creation, addition, and lookup to ensure index consistency.

| Input | Stored/Indexed Form |
| :--- | :--- |
| `"Technology"` | `"technology"` |
| `" tech "` | `"tech"` |
| `"URGENT"` | `"urgent"` |

### Duplicate Prevention Examples
- `["tech", "Tech"]` in one call $\rightarrow$ **Error**: `InvalidTag` (normalized duplicate).
- `add_invoice_tag("tech")` then `add_invoice_tag("TECH")` $\rightarrow$ **No-op** (idempotent).

---

## API Functions

### Query Functions
- `get_invoices_by_category(category: InvoiceCategory)`
- `get_invoices_by_tag(tag: String)`: Case-insensitive via normalization.
- `get_invoices_by_tags(tags: Vec<String>)`: Supports AND logic.
- `get_invoice_count_by_tag(tag: String)`

### Mutation Functions
- `update_invoice_category(invoice_id, new_category)`: O(1) index update.
- `add_invoice_tag(invoice_id, tag)`: Validates length (1-50) and count ($\le 10$).
- `remove_invoice_tag(invoice_id, tag)`

---

## Error Handling Reference

| Error Code | Error Name | Description |
| :--- | :--- | :--- |
| 1002 | Unauthorized | Caller does not match the stored business address |
| 1008 | InvoiceDueDateInvalid | Date is in the past or exceeds max bounds |
| 1035 | InvalidTag | Tag length outside 1-50 character range |
| 1036 | TagLimitExceeded | More than 10 tags per invoice |
| 1800 | InvalidTag (Normal) | Tag is empty or a duplicate after normalization |

---

## Testing and Performance
- **Index Efficiency**: O(1) lookup for categories and tags.
- **Coverage**: Implementation includes unit tests for auth bypass attempts, normalization edge cases, and index integrity during updates.