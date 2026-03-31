# Invoice Module

## Overview

The QuickLendX invoice module manages the full lifecycle of invoice-backed financing. It handles invoice creation, categorization, tagging, metadata, payment tracking, ratings, disputes, and all associated on-chain indexes.

---

## Data Structures

### `Invoice`

Core on-chain record for every invoice.

| Field | Type | Description |
| :--- | :--- | :--- |
| `id` | `BytesN<32>` | Deterministic unique identifier |
| `business` | `Address` | Business that uploaded the invoice |
| `amount` | `i128` | Total invoice face value |
| `currency` | `Address` | Payment token address |
| `due_date` | `u64` | Unix timestamp deadline |
| `status` | `InvoiceStatus` | Current lifecycle status |
| `created_at` | `u64` | Creation timestamp |
| `description` | `String` | Human-readable description |
| `metadata_*` | `Option<String>` | Optional structured metadata fields |
| `metadata_line_items` | `Vec<LineItemRecord>` | Compact line-item list |
| `category` | `InvoiceCategory` | One of 7 predefined categories |
| `tags` | `Vec<String>` | Normalized discoverability tags (max 10) |
| `funded_amount` | `i128` | Amount committed by investor |
| `funded_at` | `Option<u64>` | Funding timestamp |
| `investor` | `Option<Address>` | Address of the funding investor |
| `settled_at` | `Option<u64>` | Settlement timestamp |
| `average_rating` | `Option<u32>` | Computed average rating (1-5) |
| `total_ratings` | `u32` | Count of ratings received |
| `ratings` | `Vec<InvoiceRating>` | Full rating history |
| `dispute_status` | `DisputeStatus` | Current dispute state |
| `dispute` | `Dispute` | Dispute details |
| `total_paid` | `i128` | Aggregate amount received |
| `payment_history` | `Vec<PaymentRecord>` | Ordered partial payment records |

### `InvoiceStatus`

```
Pending → Verified → Funded → Paid
                    ↘ Defaulted
          ↘ Cancelled
                             ↘ Refunded
```

| Variant | Description |
| :--- | :--- |
| `Pending` | Uploaded, awaiting admin verification |
| `Verified` | Verified and open for investor bids |
| `Funded` | Investor has committed funds |
| `Paid` | Fully settled |
| `Defaulted` | Grace period elapsed without payment |
| `Cancelled` | Cancelled by the business owner |
| `Refunded` | Refund issued (terminal, prevents double-refund) |

### `InvoiceCategory`

`Services` | `Products` | `Consulting` | `Manufacturing` | `Technology` | `Healthcare` | `Other`

### `InvoiceMetadata`

Optional structured metadata attached to an invoice.

| Field | Max Length | Description |
| :--- | :--- | :--- |
| `customer_name` | `MAX_NAME_LENGTH` | Debtor name (required if metadata present) |
| `customer_address` | `MAX_ADDRESS_LENGTH` | Debtor address |
| `tax_id` | `MAX_TAX_ID_LENGTH` | Tax identification number |
| `line_items` | 50 items | `Vec<LineItemRecord>` (description, qty, unit price, total) |
| `notes` | `MAX_NOTES_LENGTH` | Free-form notes |

---

## Invoice ID Generation

Invoice IDs are derived deterministically to prevent collisions:

```
id[0..8]  = timestamp (big-endian u64)
id[8..12] = ledger sequence (big-endian u32)
id[12..16] = monotonic counter (big-endian u32)
id[16..32] = zero-padded
```

The counter is incremented atomically in instance storage. If a collision is detected, the counter is probed forward until a free slot is found. Counter overflow returns `StorageError`.

---

## Tag Normalization

Tags are always stored in canonical form. Normalization: strip leading/trailing ASCII spaces, then ASCII-lowercase all letters.

| Input | Stored Form |
| :--- | :--- |
| `"Technology"` | `"technology"` |
| `" tech "` | `"tech"` |
| `"URGENT"` | `"urgent"` |

- Tags must be 1–50 characters after normalization.
- Max 10 tags per invoice.
- Adding a tag that already exists (after normalization) is a no-op.
- Tags supplied at creation time are also normalized before storage.

---

## Due Date Validation

- `due_date` must be **strictly greater** than the current ledger timestamp.
- Maximum due date is bounded by the protocol limit (`max_due_date_days`, default 365 days).
- Validated during both `store_invoice` (via `lib.rs`) and `upload_invoice`.

---

## Security and Permissions Matrix

| Function / Operation | Required Signer | Guard |
| :--- | :--- | :--- |
| Create invoice | `business` | `business.require_auth()` |
| Upload invoice | `business` | `business.require_auth()` + KYC verification |
| `add_tag` | `invoice.business` | `self.business.require_auth()` |
| `remove_tag` | `invoice.business` | `self.business.require_auth()` |
| `update_metadata` | `invoice.business` | `business.require_auth()` + address match |
| `clear_metadata` | `invoice.business` | `business.require_auth()` + address match |
| `verify` (admin) | `admin` | `admin.require_auth()` in caller |
| `cancel` | `invoice.business` | address match enforced by caller |

---

## `Invoice` Methods

### Lifecycle Mutations

| Method | Description |
| :--- | :--- |
| `Invoice::new(...)` | Construct a new invoice, normalize tags, generate ID, emit audit log |
| `mark_as_funded(investor, amount, ts)` | Transition to `Funded`, emit audit log |
| `mark_as_paid(actor, ts)` | Transition to `Paid`, emit audit log |
| `mark_as_refunded(actor)` | Transition to `Refunded`, emit audit log |
| `mark_as_defaulted()` | Transition to `Defaulted` (no auth, called by expiry logic) |
| `verify(actor)` | Transition to `Verified`, emit audit log |
| `cancel(actor)` | Transition to `Cancelled`; only from `Pending` or `Verified` |

### Expiration

| Method | Description |
| :--- | :--- |
| `is_overdue(now)` | Returns `true` if `now > due_date` |
| `grace_deadline(grace_period)` | `due_date.saturating_add(grace_period)` |
| `check_and_handle_expiration(grace_period)` | If `Funded` and past grace deadline, calls `handle_default`; returns `true` if defaulted |

### Payment Tracking

| Method | Description |
| :--- | :--- |
| `record_payment(amount, tx_id)` | Appends `PaymentRecord`, updates `total_paid`, returns progress % |
| `payment_progress()` | `(total_paid / amount) * 100`, capped at 100 |
| `is_fully_paid()` | `total_paid >= amount` |

### Tag Operations

| Method | Description |
| :--- | :--- |
| `add_tag(env, tag)` | Auth-gated; normalizes, checks limits, deduplicates, updates tag index |
| `remove_tag(tag)` | Auth-gated; normalizes, removes from list and tag index |
| `has_tag(tag)` | Case-insensitive lookup via normalization; returns `bool` |
| `get_tags()` | Returns cloned tag vector |

### Category

| Method | Description |
| :--- | :--- |
| `update_category(category)` | Replaces category field (index update is caller's responsibility) |

### Metadata

| Method | Description |
| :--- | :--- |
| `update_metadata(business, metadata)` | Auth-gated; validates and stores structured metadata |
| `clear_metadata(business)` | Auth-gated; sets all metadata fields to `None` |
| `set_metadata(metadata: Option<...>)` | Unauthenticated internal setter with validation |
| `metadata()` | Returns `Option<InvoiceMetadata>` if all required fields present |

### Ratings

| Method | Description |
| :--- | :--- |
| `add_rating(rating, feedback, rater, ts)` | Investor-only; validates 1-5 range, prevents duplicate ratings, updates average |
| `get_ratings_above(threshold)` | Returns ratings ≥ threshold |
| `get_all_ratings()` | Returns reference to rating vector |
| `has_ratings()` | `total_ratings > 0` |
| `get_highest_rating()` | Max rating value, `None` if empty |
| `get_lowest_rating()` | Min rating value, `None` if empty |
| `get_invoice_rating_stats()` | Returns `InvoiceRatingStats` |

---

## `InvoiceStorage` — Index and Persistence

### Write Operations

| Method | Description |
| :--- | :--- |
| `store_invoice(invoice)` | Persist invoice; update total count, business index, status index, category index, tag indexes |
| `update_invoice(invoice)` | Overwrite invoice record (no index update — caller must maintain indexes) |
| `delete_invoice(invoice_id)` | Remove from all indexes (status, business, category, tags, metadata) and decrement count |
| `clear_all(env)` | Wipe all invoices and indexes (used by backup restore only) |
| `add_category_index(category, id)` | Deduplicated insert into category bucket |
| `remove_category_index(category, id)` | Rebuild bucket without the given ID |
| `add_tag_index(tag, id)` | Deduplicated insert into tag bucket |
| `remove_tag_index(tag, id)` | Rebuild bucket without the given ID |
| `add_to_status_invoices(status, id)` | Deduplicated insert into status bucket |
| `remove_from_status_invoices(status, id)` | Rebuild bucket without the given ID |
| `add_metadata_indexes(invoice)` | Index by `customer_name` and `tax_id` if present |
| `remove_metadata_indexes(metadata, id)` | Remove from `customer_name` and `tax_id` indexes |

### Query Operations

| Method | Returns | Description |
| :--- | :--- | :--- |
| `get_invoice(id)` | `Option<Invoice>` | Fetch by ID |
| `get_business_invoices(business)` | `Vec<BytesN<32>>` | All invoice IDs for a business |
| `count_active_business_invoices(business)` | `u32` | Count excluding `Cancelled` and `Paid` |
| `get_invoices_by_status(status)` | `Vec<BytesN<32>>` | All IDs in a status bucket |
| `get_invoices_by_category(category)` | `Vec<BytesN<32>>` | All IDs in a category bucket |
| `get_invoices_by_category_and_status(category, status)` | `Vec<BytesN<32>>` | Intersection of category and status buckets |
| `get_invoices_by_tag(tag)` | `Vec<BytesN<32>>` | All IDs with a given normalized tag |
| `get_invoices_by_tags(tags)` | `Vec<BytesN<32>>` | Intersection (AND) across all supplied tags |
| `get_invoice_count_by_category(category)` | `u32` | Count in a category bucket |
| `get_invoice_count_by_tag(tag)` | `u32` | Count in a tag bucket |
| `get_all_categories(env)` | `Vec<InvoiceCategory>` | Ordered list of all 7 categories |
| `get_invoices_with_rating_above(threshold)` | `Vec<BytesN<32>>` | `Funded`/`Paid` invoices with `average_rating >= threshold` |
| `get_invoices_by_customer(customer_name)` | `Vec<BytesN<32>>` | Lookup via metadata customer-name index |
| `get_invoices_by_tax_id(tax_id)` | `Vec<BytesN<32>>` | Lookup via metadata tax-ID index |
| `get_total_invoice_count(env)` | `u32` | Global active invoice count |

---

## Storage Keys

| Key | Type | Description |
| :--- | :--- | :--- |
| `invoice.id` (raw) | `Invoice` | Invoice record |
| `("cat_idx", category)` | `Vec<BytesN<32>>` | Category index bucket |
| `("tag_idx", tag)` | `Vec<BytesN<32>>` | Tag index bucket |
| `("business", address)` | `Vec<BytesN<32>>` | Per-business invoice list |
| `"pending"` / `"verified"` / … | `Vec<BytesN<32>>` | Per-status invoice list |
| `("icust", name)` | `Vec<BytesN<32>>` | Metadata customer-name index |
| `("itax", tax_id)` | `Vec<BytesN<32>>` | Metadata tax-ID index |
| `"total_iv"` | `u32` | Global invoice count |
| `"inv_cnt"` | `u32` | Monotonic ID counter |

---

## Error Reference

| Error | Trigger |
| :--- | :--- |
| `Unauthorized` | Caller address doesn't match stored `business` |
| `InvoiceDueDateInvalid` | `due_date` is in the past or exceeds `max_due_date_days` |
| `InvalidTag` | Tag is empty, too long (>50), or not found during removal |
| `TagLimitExceeded` | Invoice already has 10 tags; also used for >50 line items |
| `InvalidStatus` | Operation not valid for the current invoice status (e.g., cancel a `Funded` invoice) |
| `InvalidAmount` | Payment amount is zero or negative |
| `NotFunded` | Rating attempted on an invoice that is not `Funded` or `Paid` |
| `NotRater` | Rater is not the invoice's investor |
| `InvalidRating` | Rating value outside 1-5 |
| `AlreadyRated` | Investor has already rated this invoice |
| `StorageError` | Monotonic ID counter overflowed |
| `InvalidDescription` | Metadata string field length out of bounds |
