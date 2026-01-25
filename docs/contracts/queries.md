# Query Functions Documentation

## Overview

The QuickLendX contract provides comprehensive read-only query functions to support frontend dashboards and analytics. All query functions are gas-efficient and support pagination where needed.

## Query Functions

### Invoice Queries

#### get_invoices_by_business_paginated
Get invoices for a specific business with optional status filter and pagination.

**Parameters:**
- `business: Address` - Business address
- `status_filter: Option<InvoiceStatus>` - Optional status filter (Pending, Verified, Funded, Paid, Defaulted, Cancelled)
- `offset: u32` - Pagination offset (0-based)
- `limit: u32` - Maximum number of results to return

**Returns:** `Vec<BytesN<32>>` - List of invoice IDs

**Example:**
```rust
// Get first 10 verified invoices for a business
let invoices = client.get_invoices_by_business_paginated(
    &business,
    &Some(InvoiceStatus::Verified),
    &0,
    &10
);
```

#### get_available_invoices_paginated
Get available invoices (verified and not funded) with optional filters and pagination.

**Parameters:**
- `min_amount: Option<i128>` - Optional minimum amount filter
- `max_amount: Option<i128>` - Optional maximum amount filter
- `category_filter: Option<InvoiceCategory>` - Optional category filter
- `offset: u32` - Pagination offset
- `limit: u32` - Maximum number of results

**Returns:** `Vec<BytesN<32>>` - List of invoice IDs

**Example:**
```rust
// Get invoices between 1000 and 5000 in Services category
let invoices = client.get_available_invoices_paginated(
    &Some(1000i128),
    &Some(5000i128),
    &Some(InvoiceCategory::Services),
    &0,
    &20
);
```

#### get_available_invoices
Get all available invoices (verified and not funded) - simple version without pagination.

**Returns:** `Vec<BytesN<32>>` - List of invoice IDs

### Investment Queries

#### get_investments_by_investor_paginated
Get investments for a specific investor with optional status filter and pagination.

**Parameters:**
- `investor: Address` - Investor address
- `status_filter: Option<InvestmentStatus>` - Optional status filter (Active, Withdrawn, Completed, Defaulted)
- `offset: u32` - Pagination offset
- `limit: u32` - Maximum number of results

**Returns:** `Vec<BytesN<32>>` - List of investment IDs

**Example:**
```rust
// Get first 10 active investments for an investor
let investments = client.get_investments_by_investor_paginated(
    &investor,
    &Some(InvestmentStatus::Active),
    &0,
    &10
);
```

#### get_investments_by_investor
Get all investments for an investor - simple version without pagination.

**Parameters:**
- `investor: Address` - Investor address

**Returns:** `Vec<BytesN<32>>` - List of investment IDs

### Bid Queries

#### get_bid_history_paginated
Get bid history for an invoice with optional status filter and pagination.

**Parameters:**
- `invoice_id: BytesN<32>` - Invoice identifier
- `status_filter: Option<BidStatus>` - Optional status filter (Placed, Withdrawn, Accepted, Expired)
- `offset: u32` - Pagination offset
- `limit: u32` - Maximum number of results

**Returns:** `Vec<Bid>` - List of bid records

**Example:**
```rust
// Get first 5 placed bids for an invoice
let bids = client.get_bid_history_paginated(
    &invoice_id,
    &Some(BidStatus::Placed),
    &0,
    &5
);
```

#### get_investor_bid_history_paginated
Get bid history for an investor with optional status filter and pagination.

**Parameters:**
- `investor: Address` - Investor address
- `status_filter: Option<BidStatus>` - Optional status filter
- `offset: u32` - Pagination offset
- `limit: u32` - Maximum number of results

**Returns:** `Vec<Bid>` - List of bid records

**Example:**
```rust
// Get all accepted bids for an investor
let bids = client.get_investor_bid_history_paginated(
    &investor,
    &Some(BidStatus::Accepted),
    &0,
    &100
);
```

#### get_bid_history
Get all bid history for an invoice - simple version without pagination.

**Parameters:**
- `invoice_id: BytesN<32>` - Invoice identifier

**Returns:** `Vec<Bid>` - List of bid records

## Pagination Patterns

### Basic Pagination
```rust
// Page 1: First 10 results
let page1 = client.get_invoices_by_business_paginated(&business, &None, &0, &10);

// Page 2: Next 10 results
let page2 = client.get_invoices_by_business_paginated(&business, &None, &10, &10);

// Page 3: Next 10 results
let page3 = client.get_invoices_by_business_paginated(&business, &None, &20, &10);
```

### Filtered Pagination
```rust
// Get verified invoices only, paginated
let verified = client.get_invoices_by_business_paginated(
    &business,
    &Some(InvoiceStatus::Verified),
    &0,
    &10
);
```

### Amount Range Filtering
```rust
// Get invoices between 1000 and 10000
let filtered = client.get_available_invoices_paginated(
    &Some(1000i128),
    &Some(10000i128),
    &None,
    &0,
    &20
);
```

## Best Practices

### Pagination
- Use reasonable page sizes (10-50 items) to balance gas costs and user experience
- Always check if results are fewer than the limit to detect end of data
- Store offset values for efficient navigation

### Filtering
- Use status filters to reduce data transfer and processing
- Combine multiple filters for precise queries
- Cache frequently accessed filtered results

### Performance
- Query functions are read-only and gas-efficient
- Use pagination for large datasets
- Filter at the contract level to minimize data transfer

## Frontend Integration

### Dashboard Queries
```rust
// Business dashboard: Get recent invoices
let recent_invoices = client.get_invoices_by_business_paginated(
    &business,
    &None,
    &0,
    &10
);

// Investor dashboard: Get active investments
let active_investments = client.get_investments_by_investor_paginated(
    &investor,
    &Some(InvestmentStatus::Active),
    &0,
    &20
);
```

### Search and Filter
```rust
// Search available invoices by amount range
let search_results = client.get_available_invoices_paginated(
    &Some(min_amount),
    &Some(max_amount),
    &category,
    &0,
    &50
);
```

### Analytics
```rust
// Get all investments for analytics
let all_investments = client.get_investments_by_investor(&investor);

// Get bid history for analysis
let bid_history = client.get_bid_history(&invoice_id);
```

## Security Notes

- All query functions are read-only (no state changes)
- No authorization required for queries (public data)
- Pagination limits prevent excessive gas usage
- Filters are applied at contract level for efficiency

