# Invoice Categories and Tags

## Overview

The QuickLendX protocol supports invoice categorization and tagging to improve discoverability, filtering, and organization of invoices. This feature enables businesses to classify their invoices and investors to efficiently search for investment opportunities that match their preferences.

## Features

### Invoice Categories

Invoices can be assigned to one of the following predefined categories:

- **Services**: Professional services
- **Products**: Physical products
- **Consulting**: Consulting services
- **Manufacturing**: Manufacturing services
- **Technology**: Technology services/products
- **Healthcare**: Healthcare services
- **Other**: Other categories

Each invoice must have exactly one category, which can be updated after creation.

### Invoice Tags

Tags provide flexible, user-defined labels for invoices:

- **Maximum Tags**: Up to 10 tags per invoice
- **Tag Length**: 1-50 characters per tag
- **Dynamic Management**: Tags can be added or removed after invoice creation
- **Multi-tag Queries**: Support for querying invoices with multiple tags (AND logic)

## Validation Rules

### Category Validation

All enum-defined categories are valid. The validation function ensures type safety through Rust's enum system.

### Tag Validation

Tags must meet the following criteria:

1. **Count Limit**: Maximum 10 tags per invoice
2. **Length Limit**: Each tag must be 1-50 characters
3. **Non-empty**: Tags cannot be empty strings

Violations result in the following errors:
- `TagLimitExceeded` (1036): More than 10 tags
- `InvalidTag` (1035): Tag length outside 1-50 character range

## Storage and Indexing

### Category Index

Categories are indexed using the storage key pattern:
```
("cat_idx", InvoiceCategory) -> Vec<BytesN<32>>
```

This enables efficient retrieval of all invoices in a specific category.

### Tag Index

Tags are indexed using the storage key pattern:
```
("tag_idx", String) -> Vec<BytesN<32>>
```

Each tag maintains a list of invoice IDs that have been tagged with it.

### Index Maintenance

Indexes are automatically maintained during:
- Invoice creation
- Category updates
- Tag additions
- Tag removals

## API Functions

### Query Functions

#### `get_invoices_by_category(category: InvoiceCategory) -> Vec<BytesN<32>>`

Returns all invoice IDs in the specified category.

**Example:**
```rust
let services_invoices = client.get_invoices_by_category(&InvoiceCategory::Services);
```

#### `get_invoices_by_tag(tag: String) -> Vec<BytesN<32>>`

Returns all invoice IDs with the specified tag.

**Example:**
```rust
let urgent_invoices = client.get_invoices_by_tag(&String::from_str(&env, "urgent"));
```

#### `get_invoices_by_tags(tags: Vec<String>) -> Vec<BytesN<32>>`

Returns invoice IDs that have ALL specified tags (AND logic).

**Example:**
```rust
let mut tags = Vec::new(&env);
tags.push_back(String::from_str(&env, "urgent"));
tags.push_back(String::from_str(&env, "tech"));
let results = client.get_invoices_by_tags(&tags);
```

#### `get_invoices_by_cat_status(category: InvoiceCategory, status: InvoiceStatus) -> Vec<BytesN<32>>`

Returns invoices filtered by both category and status.

**Example:**
```rust
let verified_services = client.get_invoices_by_cat_status(
    &InvoiceCategory::Services,
    &InvoiceStatus::Verified
);
```

#### `get_invoice_count_by_category(category: InvoiceCategory) -> u32`

Returns the count of invoices in a category.

#### `get_invoice_count_by_tag(tag: String) -> u32`

Returns the count of invoices with a specific tag.

#### `get_all_categories() -> Vec<InvoiceCategory>`

Returns all available invoice categories.

#### `invoice_has_tag(invoice_id: BytesN<32>, tag: String) -> bool`

Checks if an invoice has a specific tag.

#### `get_invoice_tags(invoice_id: BytesN<32>) -> Vec<String>`

Returns all tags for an invoice.

### Mutation Functions

#### `update_invoice_category(invoice_id: BytesN<32>, new_category: InvoiceCategory) -> Result<(), QuickLendXError>`

Updates an invoice's category. Automatically maintains category indexes.

**Security**: Requires invoice owner authorization.

**Example:**
```rust
client.update_invoice_category(&invoice_id, &InvoiceCategory::Technology)?;
```

#### `add_invoice_tag(invoice_id: BytesN<32>, tag: String) -> Result<(), QuickLendXError>`

Adds a tag to an invoice.

**Validation**:
- Tag must be 1-50 characters
- Total tags must not exceed 10
- Duplicate tags are prevented

**Security**: Requires invoice owner authorization.

**Example:**
```rust
client.add_invoice_tag(&invoice_id, &String::from_str(&env, "urgent"))?;
```

#### `remove_invoice_tag(invoice_id: BytesN<32>, tag: String) -> Result<(), QuickLendXError>`

Removes a tag from an invoice.

**Security**: Requires invoice owner authorization.

**Example:**
```rust
client.remove_invoice_tag(&invoice_id, &String::from_str(&env, "urgent"))?;
```

## Events

The following events are emitted for category and tag operations:

### `InvoiceCategoryUpdated`

Emitted when an invoice category is changed.

**Fields**:
- `invoice_id`: BytesN<32>
- `old_category`: InvoiceCategory
- `new_category`: InvoiceCategory
- `updated_by`: Address

### `InvoiceTagAdded`

Emitted when a tag is added to an invoice.

**Fields**:
- `invoice_id`: BytesN<32>
- `tag`: String
- `added_by`: Address

### `InvoiceTagRemoved`

Emitted when a tag is removed from an invoice.

**Fields**:
- `invoice_id`: BytesN<32>
- `tag`: String
- `removed_by`: Address

## Use Cases

### For Businesses

1. **Organize Invoices**: Categorize invoices by business type
2. **Highlight Urgency**: Tag time-sensitive invoices as "urgent"
3. **Industry Tagging**: Use tags like "tech", "healthcare", "manufacturing"
4. **Project Tracking**: Tag invoices by project name or client

### For Investors

1. **Filter by Industry**: Find invoices in preferred categories
2. **Search by Criteria**: Use tags to find specific investment types
3. **Risk Assessment**: Filter by tags indicating risk levels
4. **Portfolio Diversification**: Invest across different categories

## Security Considerations

1. **Authorization**: All mutation operations require proper authentication
2. **Validation**: Strict validation prevents malformed data
3. **Index Integrity**: Automatic index maintenance ensures consistency
4. **Audit Trail**: All operations are logged via events

## Performance Considerations

1. **Index Efficiency**: O(1) lookup for category and tag queries
2. **Storage Optimization**: Indexes stored separately from invoice data
3. **Batch Operations**: Multi-tag queries optimized with early filtering
4. **Duplicate Prevention**: Built-in checks prevent duplicate entries

## Testing

The implementation includes comprehensive tests covering:

- Basic category and tag operations
- Validation edge cases (limits, lengths)
- Index integrity
- Category updates
- Tag additions and removals
- Multi-tag queries
- Count functions
- Error conditions

Test coverage exceeds 95% for all category and tag functionality.

## Error Handling

| Error Code | Error Name | Description |
|------------|------------|-------------|
| 1035 | InvalidTag | Tag length outside 1-50 character range |
| 1036 | TagLimitExceeded | More than 10 tags per invoice |
| 1001 | InvoiceNotFound | Invoice ID does not exist |
| 1002 | Unauthorized | Caller not authorized for operation |

## Best Practices

1. **Consistent Naming**: Use lowercase tags for consistency
2. **Meaningful Categories**: Choose the most specific category
3. **Tag Sparingly**: Use 3-5 tags per invoice for optimal discoverability
4. **Avoid Redundancy**: Don't duplicate category information in tags
5. **Update Promptly**: Keep categories and tags current as invoice status changes

## Future Enhancements

Potential future improvements:

- Custom category definitions
- Tag hierarchies and relationships
- Tag popularity metrics
- Auto-tagging based on invoice content
- Tag-based analytics and reporting
