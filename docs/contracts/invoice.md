# Invoice Authentication Policy, Categories, Tags, and Due Date Validation

## Overview

The QuickLendX protocol supports invoice categorization and tagging to improve discoverability, filtering, and organization of invoices. Additionally, the protocol enforces due date bounds and strict authentication/KYC checks to ensure system security and prevent unauthorized invoice spam.

---

## `store_invoice` Authentication Policy (Issue #790)

`store_invoice` (and its alias `upload_invoice`) enforces a **two-layer authentication policy** to prevent unauthorized invoice creation and storage-based denial-of-service attacks.

### Policy Layers

| Layer | Requirement | Enforcement |
| :--- | :--- | :--- |
| 1 — Business Signature | The `business` address must sign the transaction | `business.require_auth()` |
| 2 — Verified KYC | The business must have a `Verified` KYC record | `require_business_not_pending()` |

### KYC State → Access Matrix

| KYC State | `store_invoice` Result | Error Code |
| :--- | :--- | :--- |
| No record | ❌ Blocked | `BusinessNotVerified` (1600) |
| `Pending` | ❌ Blocked | `KYCAlreadyPending` (1601) |
| `Rejected` | ❌ Blocked | `BusinessNotVerified` (1600) |
| `Verified` | ✅ Allowed | — |

### Security Invariants

- **Anti-spam / storage DoS**: Only KYC-gated addresses can write invoice data to on-chain persistent storage. An attacker cannot flood the contract with fake invoices without first passing admin-reviewed KYC.
- **Business-only signature**: The `business` address must sign. The admin cannot create invoices on behalf of a business. No third party can impersonate a business.
- **Distinct error codes**: `KYCAlreadyPending` vs `BusinessNotVerified` allows callers to distinguish "awaiting review" from "rejected/unknown" and provide appropriate UX feedback.
- **Verified is final**: Once verified, a business retains access unless the KYC lifecycle is explicitly reset (rejection → resubmission → re-verification).

### KYC Lifecycle and `store_invoice` Access

```
No KYC → submit_kyc_application() → Pending → verify_business() → Verified ✅
                                          ↓
                                   reject_business()
                                          ↓
                                       Rejected → submit_kyc_application() → Pending → ...
```

Access to `store_invoice` is only granted at the `Verified` leaf of this state machine.

---

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

| Function | Required Signer | KYC Required | Enforcement Mechanism |
| :--- | :--- | :--- | :--- |
| `store_invoice` | `business` | ✅ Verified | `business.require_auth()` + `require_business_not_pending()` |
| `upload_invoice` | `business` | ✅ Verified | `business.require_auth()` + `require_business_not_pending()` |
| `update_invoice_category` | `invoice.business` | — | `self.business.require_auth()` |
| `add_invoice_tag` | `invoice.business` | — | `self.business.require_auth()` |
| `remove_invoice_tag` | `invoice.business` | — | `self.business.require_auth()` |
| `verify_invoice` | `admin` | — | `admin.require_auth()` |

> **Authorization Flow**: The contract calls Soroban's `require_auth()` on the `business` address, then checks the KYC state via `BusinessVerificationStorage`. Both checks must pass for `store_invoice` to proceed.

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
- `store_invoice(business, amount, currency, due_date, description, category, tags)`: Requires business auth + Verified KYC.
- `upload_invoice(business, amount, currency, due_date, description, category, tags)`: Alias for `store_invoice`; same policy applies.
- `update_invoice_category(invoice_id, new_category)`: O(1) index update.
- `add_invoice_tag(invoice_id, tag)`: Validates length (1-50) and count ($\le 10$).
- `remove_invoice_tag(invoice_id, tag)`

---

## Error Handling Reference

| Error Code | Error Name | Description |
| :--- | :--- | :--- |
| 1100 | Unauthorized | Caller does not match the stored business address |
| 1004 | InvoiceDueDateInvalid | Date is in the past or exceeds max bounds |
| 1600 | BusinessNotVerified | Business has no KYC record or was rejected |
| 1601 | KYCAlreadyPending | Business KYC is awaiting admin review |
| 1800 | InvalidTag | Tag length outside 1-50 character range or normalized duplicate |
| 1801 | TagLimitExceeded | More than 10 tags per invoice |

---

## Testing and Performance
- **Index Efficiency**: O(1) lookup for categories and tags.
- **Auth Policy Coverage**: `src/test_store_invoice_auth.rs` contains dedicated regression tests for every KYC state, the business-signature requirement, admin bypass prevention, and anti-spam scenarios.
- **Coverage**: Implementation includes unit tests for auth bypass attempts, normalization edge cases, and index integrity during updates.