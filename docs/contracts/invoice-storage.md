# Invoice Storage and Indexing

## Overview

The Invoice Storage module provides comprehensive storage, indexing, and retrieval functionality for invoices in the QuickLendX protocol. It implements efficient data structures and indexes to support fast queries by business, status, category, tags, and metadata.

## Storage Architecture

### Primary Storage

Invoices are stored using a key-value pattern:

- **Key**: `(Symbol("invoice"), invoice_id: BytesN<32>)`
- **Value**: `Invoice` struct

### Indexes

Multiple indexes are maintained for efficient querying:

1. **Business Index**: Maps business address to their invoice IDs
   - Key: `(Symbol("bus_inv"), business: Address)`
   - Value: `Vec<BytesN<32>>`

2. **Status Index**: Maps invoice status to invoice IDs
   - Key: `(Symbol("stat_inv"), status: InvoiceStatus)`
   - Value: `Vec<BytesN<32>>`

3. **Category Index**: Maps category to invoice IDs
   - Key: `(Symbol("cat_inv"), category: InvoiceCategory)`
   - Value: `Vec<BytesN<32>>`

4. **Tag Index**: Maps tags to invoice IDs
   - Key: `(Symbol("tag_inv"), tag: String)`
   - Value: `Vec<BytesN<32>>`

5. **Rating Index**: Tracks invoices with ratings
   - Key: `Symbol("inv_ratings")`
   - Value: `Vec<BytesN<32>>`

6. **Metadata Indexes**:
   - Customer Index: `(Symbol("meta_cust"), customer_name: String)`
   - Tax ID Index: `(Symbol("meta_tax"), tax_id: String)`

## Data Structures

### Invoice

```rust
pub struct Invoice {
    pub id: BytesN<32>,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub due_date: u64,
    pub description: String,
    pub category: InvoiceCategory,
    pub tags: Vec<String>,
    pub status: InvoiceStatus,
    pub created_at: u64,
    pub updated_at: u64,
    pub metadata: Option<InvoiceMetadata>,
    pub ratings: Vec<InvoiceRating>,
    pub payment_records: Vec<PaymentRecord>,
    pub amount_paid: i128,
    pub grace_period: Option<u64>,
}
```

### InvoiceStatus

```rust
pub enum InvoiceStatus {
    Pending,    // Uploaded, awaiting verification
    Verified,   // Verified and available for bidding
    Funded,     // Funded by investor
    Paid,       // Fully paid and settled
    Defaulted,  // Payment overdue
    Cancelled,  // Cancelled by business
    Refunded,   // Refunded to investor
}
```

### InvoiceCategory

```rust
pub enum InvoiceCategory {
    Services,
    Products,
    Consulting,
    Manufacturing,
    Technology,
    Healthcare,
    Education,
    Other,
}
```

### InvoiceMetadata

```rust
pub struct InvoiceMetadata {
    pub customer_name: String,
    pub customer_address: Option<String>,
    pub tax_id: Option<String>,
    pub purchase_order: Option<String>,
    pub line_items: Vec<LineItemRecord>,
}
```

## Core Storage Functions

### store_invoice

Stores a new invoice and creates all necessary indexes.

```rust
pub fn store_invoice(env: &Env, invoice: &Invoice)
```

**Operations:**

1. Stores invoice in primary storage
2. Adds to business index
3. Adds to status index
4. Adds to category index
5. Adds to tag indexes (for each tag)
6. Adds to metadata indexes (if metadata exists)

**Example:**

```rust
let invoice = Invoice::new(
    &env,
    business_addr,
    amount,
    currency_addr,
    due_date,
    description,
    category,
    tags,
);

InvoiceStorage::store_invoice(&env, &invoice);
```

### get_invoice

Retrieves an invoice by ID.

```rust
pub fn get_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Invoice>
```

**Example:**

```rust
if let Some(invoice) = InvoiceStorage::get_invoice(&env, &invoice_id) {
    // Process invoice
}
```

### update_invoice

Updates an existing invoice and maintains index consistency.

```rust
pub fn update_invoice(env: &Env, invoice: &Invoice)
```

**Important:** When updating invoice status, category, or tags, indexes must be updated:

```rust
// Get old invoice
let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();

// Update status
let old_status = invoice.status.clone();
invoice.status = InvoiceStatus::Verified;

// Remove from old status index
InvoiceStorage::remove_from_status_invoices(&env, &old_status, &invoice_id);

// Add to new status index
InvoiceStorage::add_to_status_invoices(&env, &invoice.status, &invoice_id);

// Update storage
InvoiceStorage::update_invoice(&env, &invoice);
```

## Query Functions

### By Business

```rust
pub fn get_business_invoices(env: &Env, business: &Address) -> Vec<BytesN<32>>
```

Returns all invoice IDs for a specific business.

**Example:**

```rust
let invoice_ids = InvoiceStorage::get_business_invoices(&env, &business_addr);
for id in invoice_ids.iter() {
    if let Some(invoice) = InvoiceStorage::get_invoice(&env, &id) {
        // Process invoice
    }
}
```

### By Status

```rust
pub fn get_invoices_by_status(env: &Env, status: &InvoiceStatus) -> Vec<BytesN<32>>
```

Returns all invoice IDs with a specific status.

**Example:**

```rust
// Get all verified invoices
let verified_invoices = InvoiceStorage::get_invoices_by_status(
    &env,
    &InvoiceStatus::Verified
);
```

### By Category

```rust
pub fn get_invoices_by_category(env: &Env, category: &InvoiceCategory) -> Vec<BytesN<32>>
```

Returns all invoice IDs in a specific category.

**Example:**

```rust
let service_invoices = InvoiceStorage::get_invoices_by_category(
    &env,
    &InvoiceCategory::Services
);
```

### By Category and Status

```rust
pub fn get_invoices_by_category_and_status(
    env: &Env,
    category: &InvoiceCategory,
    status: &InvoiceStatus,
) -> Vec<BytesN<32>>
```

Returns invoice IDs matching both category and status.

**Example:**

```rust
// Get verified service invoices
let invoices = InvoiceStorage::get_invoices_by_category_and_status(
    &env,
    &InvoiceCategory::Services,
    &InvoiceStatus::Verified,
);
```

### By Tag

```rust
pub fn get_invoices_by_tag(env: &Env, tag: &String) -> Vec<BytesN<32>>
```

Returns all invoice IDs with a specific tag.

**Example:**

```rust
let tech_invoices = InvoiceStorage::get_invoices_by_tag(
    &env,
    &String::from_str(&env, "technology")
);
```

### By Multiple Tags

```rust
pub fn get_invoices_by_tags(env: &Env, tags: &Vec<String>) -> Vec<BytesN<32>>
```

Returns invoice IDs that have ALL specified tags (intersection).

**Example:**

```rust
let tags = vec![
    &env,
    String::from_str(&env, "urgent"),
    String::from_str(&env, "high-value")
];
let invoices = InvoiceStorage::get_invoices_by_tags(&env, &tags);
```

### By Rating

```rust
pub fn get_invoices_with_rating_above(env: &Env, threshold: u32) -> Vec<BytesN<32>>
```

Returns invoices with average rating above threshold.

**Example:**

```rust
// Get invoices with rating >= 4
let high_rated = InvoiceStorage::get_invoices_with_rating_above(&env, 4);
```

### By Business and Rating

```rust
pub fn get_business_invoices_with_rating_above(
    env: &Env,
    business: &Address,
    threshold: u32,
) -> Vec<BytesN<32>>
```

Returns invoices for a specific business with rating above threshold.

### By Metadata

```rust
pub fn get_invoices_by_customer(env: &Env, customer_name: &String) -> Vec<BytesN<32>>
pub fn get_invoices_by_tax_id(env: &Env, tax_id: &String) -> Vec<BytesN<32>>
```

Query invoices by metadata fields.

**Example:**

```rust
let customer_invoices = InvoiceStorage::get_invoices_by_customer(
    &env,
    &String::from_str(&env, "Acme Corp")
);
```

## Index Management

### Category Index

```rust
pub fn add_category_index(env: &Env, category: &InvoiceCategory, invoice_id: &BytesN<32>)
pub fn remove_category_index(env: &Env, category: &InvoiceCategory, invoice_id: &BytesN<32>)
```

**Usage:**

```rust
// When creating invoice
InvoiceStorage::add_category_index(&env, &invoice.category, &invoice.id);

// When updating category
InvoiceStorage::remove_category_index(&env, &old_category, &invoice.id);
InvoiceStorage::add_category_index(&env, &new_category, &invoice.id);
```

### Tag Index

```rust
pub fn add_tag_index(env: &Env, tag: &String, invoice_id: &BytesN<32>)
pub fn remove_tag_index(env: &Env, tag: &String, invoice_id: &BytesN<32>)
```

**Usage:**

```rust
// When adding tag
InvoiceStorage::add_tag_index(&env, &tag, &invoice.id);

// When removing tag
InvoiceStorage::remove_tag_index(&env, &tag, &invoice.id);
```

### Status Index

```rust
pub fn add_to_status_invoices(env: &Env, status: &InvoiceStatus, invoice_id: &BytesN<32>)
pub fn remove_from_status_invoices(env: &Env, status: &InvoiceStatus, invoice_id: &BytesN<32>)
```

**Usage:**

```rust
// When changing status
InvoiceStorage::remove_from_status_invoices(&env, &old_status, &invoice.id);
InvoiceStorage::add_to_status_invoices(&env, &new_status, &invoice.id);
```

### Metadata Index

```rust
pub fn add_metadata_indexes(env: &Env, invoice: &Invoice)
pub fn remove_metadata_indexes(env: &Env, metadata: &InvoiceMetadata, invoice_id: &BytesN<32>)
```

**Usage:**

```rust
// When adding metadata
InvoiceStorage::add_metadata_indexes(&env, &invoice);

// When removing metadata
if let Some(metadata) = &invoice.metadata {
    InvoiceStorage::remove_metadata_indexes(&env, metadata, &invoice.id);
}
```

## Statistics Functions

### Count by Category

```rust
pub fn get_invoice_count_by_category(env: &Env, category: &InvoiceCategory) -> u32
```

### Count by Tag

```rust
pub fn get_invoice_count_by_tag(env: &Env, tag: &String) -> u32
```

### Count with Ratings

```rust
pub fn get_invoices_with_ratings_count(env: &Env) -> u32
```

### Get All Categories

```rust
pub fn get_all_categories(env: &Env) -> Vec<InvoiceCategory>
```

## Complete Usage Example

### Creating and Storing an Invoice

```rust
use soroban_sdk::{Address, String, Vec, BytesN, Env};
use crate::invoice::{Invoice, InvoiceCategory, InvoiceStatus, InvoiceStorage};

// Create invoice
let tags = vec![
    &env,
    String::from_str(&env, "urgent"),
    String::from_str(&env, "services")
];

let invoice = Invoice::new(
    &env,
    business_addr,
    100000, // $1000.00
    usdc_addr,
    due_date,
    String::from_str(&env, "Consulting services"),
    InvoiceCategory::Consulting,
    tags,
);

// Store invoice (creates all indexes)
InvoiceStorage::store_invoice(&env, &invoice);

// Invoice is now queryable by:
// - Business address
// - Status (Pending)
// - Category (Consulting)
// - Tags ("urgent", "services")
```

### Updating Invoice Status

```rust
// Get invoice
let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();

// Update status with proper index management
let old_status = invoice.status.clone();
invoice.status = InvoiceStatus::Verified;

// Update indexes
InvoiceStorage::remove_from_status_invoices(&env, &old_status, &invoice.id);
InvoiceStorage::add_to_status_invoices(&env, &invoice.status, &invoice.id);

// Save updated invoice
InvoiceStorage::update_invoice(&env, &invoice);
```

### Updating Invoice Category

```rust
// Get invoice
let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();

// Update category with proper index management
let old_category = invoice.category.clone();
invoice.update_category(InvoiceCategory::Technology);

// Update indexes
InvoiceStorage::remove_category_index(&env, &old_category, &invoice.id);
InvoiceStorage::add_category_index(&env, &invoice.category, &invoice.id);

// Save updated invoice
InvoiceStorage::update_invoice(&env, &invoice);
```

### Adding and Removing Tags

```rust
// Get invoice
let mut invoice = InvoiceStorage::get_invoice(&env, &invoice_id).unwrap();

// Add tag
let new_tag = String::from_str(&env, "high-priority");
invoice.add_tag(&env, new_tag.clone())?;
InvoiceStorage::add_tag_index(&env, &new_tag, &invoice.id);

// Remove tag
let old_tag = String::from_str(&env, "urgent");
invoice.remove_tag(old_tag.clone())?;
InvoiceStorage::remove_tag_index(&env, &old_tag, &invoice.id);

// Save updated invoice
InvoiceStorage::update_invoice(&env, &invoice);
```

### Complex Queries

```rust
// Get all verified service invoices for a business
let business_invoices = InvoiceStorage::get_business_invoices(&env, &business_addr);
let mut verified_services = Vec::new(&env);

for id in business_invoices.iter() {
    if let Some(invoice) = InvoiceStorage::get_invoice(&env, &id) {
        if invoice.status == InvoiceStatus::Verified
            && invoice.category == InvoiceCategory::Services {
            verified_services.push_back(id);
        }
    }
}

// Get high-rated technology invoices
let tech_invoices = InvoiceStorage::get_invoices_by_category(
    &env,
    &InvoiceCategory::Technology
);
let mut high_rated_tech = Vec::new(&env);

for id in tech_invoices.iter() {
    if let Some(invoice) = InvoiceStorage::get_invoice(&env, &id) {
        if let Some(highest_rating) = invoice.get_highest_rating() {
            if highest_rating >= 4 {
                high_rated_tech.push_back(id);
            }
        }
    }
}
```

## Performance Considerations

### Index Maintenance

1. **Always update indexes when modifying indexed fields**:
   - Status changes require status index updates
   - Category changes require category index updates
   - Tag additions/removals require tag index updates
   - Metadata changes require metadata index updates

2. **Batch operations**:
   - When creating multiple invoices, batch storage operations
   - Consider transaction boundaries for consistency

3. **Query optimization**:
   - Use specific indexes when possible (category, status, tag)
   - Avoid full scans when indexes are available
   - Consider pagination for large result sets

### Storage Efficiency

1. **Minimize redundant data**:
   - Store invoice once in primary storage
   - Indexes only store invoice IDs, not full invoices

2. **Clean up on deletion**:
   - Remove from all indexes when deleting invoice
   - Clean up orphaned index entries

3. **Optimize metadata**:
   - Only index frequently queried metadata fields
   - Consider separate storage for large metadata

## Security Considerations

### Access Control

- Storage functions should only be called by authorized contract functions
- Direct storage access should be restricted
- Index updates must be atomic with invoice updates

### Data Integrity

- Always maintain index consistency
- Validate invoice data before storage
- Ensure status transitions are valid
- Prevent duplicate invoice IDs

### Input Validation

- Validate all invoice fields before storage
- Check amount is positive
- Verify due date is in future
- Validate description is not empty
- Check category is valid
- Validate tags (max 10, 1-50 chars each)

## Testing

The invoice storage module includes comprehensive tests covering:

- ✅ Invoice creation and storage
- ✅ Index creation and maintenance
- ✅ Query functions (by business, status, category, tag)
- ✅ Status transitions
- ✅ Category updates
- ✅ Tag management
- ✅ Metadata indexing
- ✅ Rating queries
- ✅ Edge cases and error handling

Run tests with:

```bash
cargo test invoice
```

## Best Practices

### Creating Invoices

1. **Validate before storage**:

   ```rust
   if amount <= 0 {
       return Err(QuickLendXError::InvalidAmount);
   }
   if due_date <= env.ledger().timestamp() {
       return Err(QuickLendXError::InvoiceDueDateInvalid);
   }
   ```

2. **Initialize all fields**:
   - Set created_at and updated_at timestamps
   - Initialize empty vectors for ratings and payment_records
   - Set initial status to Pending

3. **Create all indexes**:
   - Call `store_invoice` which handles all index creation
   - Don't manually create indexes unless necessary

### Updating Invoices

1. **Maintain index consistency**:

   ```rust
   // Always remove from old index before adding to new
   InvoiceStorage::remove_from_status_invoices(&env, &old_status, &id);
   InvoiceStorage::add_to_status_invoices(&env, &new_status, &id);
   ```

2. **Update timestamp**:

   ```rust
   invoice.updated_at = env.ledger().timestamp();
   ```

3. **Validate transitions**:
   - Check status transitions are valid
   - Verify caller has permission
   - Ensure business logic is satisfied

### Querying Invoices

1. **Use appropriate indexes**:
   - Query by status for status-specific lists
   - Query by category for category browsing
   - Query by tag for tag-based discovery

2. **Handle empty results**:

   ```rust
   let invoices = InvoiceStorage::get_business_invoices(&env, &business);
   if invoices.is_empty() {
       // Handle no invoices case
   }
   ```

3. **Paginate large results**:
   - Use paginated query functions when available
   - Implement client-side pagination for large datasets

## Integration

### With Invoice Lifecycle

```rust
// In invoice.rs
impl Invoice {
    pub fn verify(&mut self, env: &Env, actor: Address) {
        let old_status = self.status.clone();
        self.status = InvoiceStatus::Verified;
        self.updated_at = env.ledger().timestamp();

        // Update indexes
        InvoiceStorage::remove_from_status_invoices(env, &old_status, &self.id);
        InvoiceStorage::add_to_status_invoices(env, &self.status, &self.id);

        // Save
        InvoiceStorage::update_invoice(env, self);

        // Emit event
        events::emit_invoice_verified(env, &self.id, &self.business);
    }
}
```

### With Bidding System

```rust
// Query available invoices for bidding
let verified_invoices = InvoiceStorage::get_invoices_by_status(
    &env,
    &InvoiceStatus::Verified
);

// Filter by investor preferences
for id in verified_invoices.iter() {
    if let Some(invoice) = InvoiceStorage::get_invoice(&env, &id) {
        if invoice.amount >= min_amount && invoice.amount <= max_amount {
            // Show to investor
        }
    }
}
```

## Future Enhancements

- Composite indexes for common query patterns
- Full-text search on descriptions
- Date range queries
- Amount range indexes
- Geographic indexing (if location metadata added)
- Archive old invoices to reduce active storage
- Implement storage migration for schema updates

## References

- [Invoice Lifecycle](./invoice-lifecycle.md) - Invoice state management
- [Invoice Metadata](./invoice-metadata.md) - Metadata structure and usage
- [Bidding System](./bidding.md) - How invoices are bid on
- [Storage Schema](./storage-schema.md) - Overall storage architecture
