# Investment Query Pagination Documentation

## Overview

This document describes the pagination system for investor investment queries in the QuickLendX protocol, with a focus on boundary handling, overflow-safe arithmetic, and security considerations.

## Core Functions

### `get_investor_investments_paged`

Retrieves paginated investments for a specific investor with comprehensive boundary validation and overflow protection.

#### Signature

```rust
pub fn get_investor_investments_paged(
    env: Env,
    investor: Address,
    status_filter: Option<InvestmentStatus>,
    offset: u32,
    limit: u32,
) -> Vec<BytesN<32>>
```

#### Parameters

- `env`: Soroban environment context
- `investor`: Address of the investor to query investments for
- `status_filter`: Optional filter by investment status (Active, Completed, Defaulted, Withdrawn)
- `offset`: Starting position in the result set (0-based indexing)
- `limit`: Maximum number of records to return (subject to `MAX_QUERY_LIMIT`)

#### Returns

Vector of investment IDs (`BytesN<32>`) matching the specified criteria.

#### Security Features

1. **Overflow-Safe Arithmetic**: All arithmetic operations use saturating variants to prevent integer overflow attacks
2. **Boundary Validation**: Offset and limit values are validated against collection bounds
3. **DoS Protection**: Query limit is capped to `MAX_QUERY_LIMIT` (100) to prevent resource exhaustion
4. **Graceful Edge Case Handling**: Handles empty collections, oversized offsets, and zero limits safely

## Pagination Boundary Handling

### Offset Boundaries

| Condition | Behavior | Example |
|-----------|----------|---------|
| `offset < total_count` | Normal pagination | `offset=5, total=10` → starts at index 5 |
| `offset == total_count` | Returns empty result | `offset=10, total=10` → empty vector |
| `offset > total_count` | Returns empty result | `offset=15, total=10` → empty vector |

### Limit Boundaries

| Condition | Behavior | Example |
|-----------|----------|---------|
| `limit == 0` | Returns empty result | Any offset with `limit=0` → empty vector |
| `limit <= MAX_QUERY_LIMIT` | Uses requested limit | `limit=50` → returns up to 50 items |
| `limit > MAX_QUERY_LIMIT` | Capped to MAX_QUERY_LIMIT | `limit=200` → capped to 100 items |

### Collection Size Boundaries

| Condition | Behavior | Example |
|-----------|----------|---------|
| Empty collection | Always returns empty | Any offset/limit on empty collection → empty vector |
| Single item | Proper pagination | `offset=0, limit=1` → 1 item; `offset=1, limit=1` → empty |
| Large collection | Efficient chunking | Handles collections larger than `MAX_QUERY_LIMIT` |

## Overflow-Safe Arithmetic

### Saturating Operations

All arithmetic operations use saturating variants to prevent overflow:

```rust
// Safe addition - will not overflow
let end = start.saturating_add(capped_limit).min(collection_size);

// Safe subtraction - will not underflow  
let remaining = total_count.saturating_sub(safe_offset);

// Safe increment in loops
idx = idx.saturating_add(1);
```

### Boundary Calculation

The `calculate_safe_bounds` function ensures all bounds are within valid ranges:

```rust
pub fn calculate_safe_bounds(offset: u32, limit: u32, collection_size: u32) -> (u32, u32) {
    let capped_limit = Self::cap_query_limit(limit);
    let start = offset.min(collection_size);
    let end = start.saturating_add(capped_limit).min(collection_size);
    (start, end)
}
```

## Status Filtering

Investment queries support optional status filtering:

- `None`: Returns all investments regardless of status
- `Some(InvestmentStatus::Active)`: Returns only active investments
- `Some(InvestmentStatus::Completed)`: Returns only completed investments
- `Some(InvestmentStatus::Defaulted)`: Returns only defaulted investments
- `Some(InvestmentStatus::Withdrawn)`: Returns only withdrawn investments

Filtering is applied before pagination, ensuring consistent page sizes across different status filters.

## Performance Considerations

### Query Limits

- **Maximum Query Limit**: 100 items per request (`MAX_QUERY_LIMIT`)
- **Rationale**: Prevents memory exhaustion and ensures consistent response times
- **Enforcement**: Automatically applied to all pagination requests

### Efficient Iteration

The pagination system uses efficient iteration patterns:

1. **Single Pass Filtering**: Status filtering and pagination are combined in a single iteration
2. **Early Termination**: Stops processing once the required page is collected
3. **Bounds Checking**: Validates array access before retrieval

## Error Handling

### Graceful Degradation

The system handles error conditions gracefully:

- **Storage Access Failures**: Missing investments are skipped without failing the entire query
- **Invalid Parameters**: Extreme values (e.g., `u32::MAX`) are handled safely
- **Empty Results**: Returns empty vectors rather than errors for boundary conditions

### No Panics Policy

All functions are designed to never panic, even with malicious inputs:

- Saturating arithmetic prevents overflow panics
- Bounds checking prevents array access panics
- Defensive programming handles all edge cases

## Usage Examples

### Basic Pagination

```rust
// Get first page of active investments
let page1 = contract.get_investor_investments_paged(
    env,
    investor_address,
    Some(InvestmentStatus::Active),
    0,    // offset
    10    // limit
);

// Get second page
let page2 = contract.get_investor_investments_paged(
    env,
    investor_address,
    Some(InvestmentStatus::Active),
    10,   // offset
    10    // limit
);
```

### Handling Large Datasets

```rust
let mut all_investments = Vec::new(&env);
let mut offset = 0u32;
let limit = 100u32; // Use maximum limit for efficiency

loop {
    let page = contract.get_investor_investments_paged(
        env,
        investor_address,
        None, // No status filter
        offset,
        limit
    );
    
    if page.is_empty() {
        break; // No more data
    }
    
    for investment_id in page.iter() {
        all_investments.push_back(investment_id);
    }
    
    offset = offset.saturating_add(limit);
}
```

### Status-Specific Queries

```rust
// Get all completed investments
let completed = contract.get_investor_investments_paged(
    env,
    investor_address,
    Some(InvestmentStatus::Completed),
    0,
    100
);

// Get all defaulted investments  
let defaulted = contract.get_investor_investments_paged(
    env,
    investor_address,
    Some(InvestmentStatus::Defaulted),
    0,
    100
);
```

## Testing Coverage

The pagination system includes comprehensive boundary tests:

### Boundary Conditions
- Offset equals total count
- Offset exceeds total count  
- Zero limit values
- Limit exceeds maximum allowed
- Empty collections
- Single-item collections

### Overflow Protection
- Maximum `u32` values for offset and limit
- Saturating arithmetic validation
- Large offset with small collections
- Arithmetic overflow scenarios

### Consistency Tests
- Multi-page query consistency
- No duplicate results across pages
- Proper page boundaries
- Status filtering with pagination

### Edge Cases
- Mixed investment statuses
- Large datasets (> `MAX_QUERY_LIMIT`)
- Concurrent modifications (if applicable)
- Storage access failures

## Security Considerations

### DoS Attack Prevention

1. **Query Limit Enforcement**: Prevents resource exhaustion via large queries
2. **Timeout Protection**: Bounded execution time regardless of collection size
3. **Memory Usage Control**: Limited result set size prevents memory exhaustion

### Integer Overflow Protection

1. **Saturating Arithmetic**: All operations use overflow-safe variants
2. **Bounds Validation**: Parameters are validated before use
3. **Safe Indexing**: Array access is bounds-checked

### Input Validation

1. **Parameter Sanitization**: All inputs are validated and sanitized
2. **Range Checking**: Values are checked against valid ranges
3. **Type Safety**: Leverages Rust's type system for additional safety

## Best Practices

### For Developers

1. **Always Use Pagination**: Don't query large datasets without pagination
2. **Handle Empty Results**: Check for empty results and handle gracefully
3. **Respect Limits**: Don't attempt to circumvent `MAX_QUERY_LIMIT`
4. **Status Filtering**: Use status filters to reduce data transfer

### For Integrators

1. **Implement Proper Pagination**: Use offset-based pagination correctly
2. **Handle Boundaries**: Test edge cases like empty collections
3. **Error Handling**: Implement proper error handling for all scenarios
4. **Performance Monitoring**: Monitor query performance and adjust page sizes

## Constants

```rust
/// Maximum number of records returned by paginated query endpoints
pub const MAX_QUERY_LIMIT: u32 = 100;
```

This limit ensures:
- Consistent performance across all queries
- Protection against DoS attacks
- Reasonable memory usage
- Predictable response times

## Future Enhancements

Potential improvements to the pagination system:

1. **Cursor-Based Pagination**: For better performance with large, frequently-changing datasets
2. **Sorting Options**: Allow sorting by different fields (amount, date, etc.)
3. **Advanced Filtering**: Support for date ranges, amount ranges, etc.
4. **Caching**: Cache frequently-accessed pages for better performance
5. **Streaming**: Support for streaming large result sets

## Conclusion

The investment query pagination system provides a robust, secure, and efficient way to handle large datasets while protecting against common attack vectors and edge cases. The comprehensive boundary testing ensures reliable behavior across all scenarios, making it suitable for production use in financial applications.