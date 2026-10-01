# Cross-invoice analytics and read patterns

Audience: contributors building dashboards, indexers, or other read-only integrations.

This note documents the contract entrypoints that read across more than one invoice or aggregate invoice state for analytics workflows. The goal is to make the supported read patterns and their bounds explicit so review comments and support questions can point to one source of truth.

## What counts as a cross-invoice read?

A cross-invoice read is any read-only entrypoint that either:

- returns multiple invoice IDs or records at once, or
- computes a metric from the current invoice set instead of a single invoice.

These entrypoints are read-only and do not mutate contract state.

## Supported reads and their bounds

| Entrypoint | What it returns | Bounds / behavior |
|---|---|---|
| `get_business_invoices_paged(env, business, status_filter, offset, limit)` | Invoice IDs for one business | `limit` is clamped to `MAX_QUERY_LIMIT` (50). `offset` is validated before paging; if it is beyond the available data, the result is empty. Results are sorted newest-first by `created_at`. |
| `get_available_invoices_paged(env, min_amount, max_amount, category_filter, offset, limit)` | Verified invoice IDs that match optional filters | Same paging bounds as above: `limit` is capped at 50 and `offset` beyond the available set returns an empty page. |
| `get_invoices_by_status(env, status)` | All invoice IDs in one status bucket | No pagination. This is convenient for batch processing, but it can be large. Prefer the paged variants for UI and indexer workloads. |
| `get_invoice_count_by_status(env, status)` and `get_total_invoice_count(env)` | Counters only | These do not return invoice IDs. They are the cheapest way to answer “how many invoices are there?” questions. |
| `get_category_breakdown(env)` | Category counts for dashboard charts | The output is bounded by the invoice-category enum and only includes categories that currently have at least one invoice. |
| `get_platform_metrics(env)` | Aggregate platform totals and rates | Computes the current platform-wide view from the live invoice state. No pagination is involved. |
| `get_performance_metrics(env)` | Aggregate performance and timing metrics | Derived from the same invoice state as the platform metrics. No pagination is involved. |
| `get_analytics_summary(env)` | A tuple of platform and performance metrics | Convenience wrapper for a single round-trip. It is equivalent to calling the two metric getters separately. |
| `export_analytics_snapshot(env)` | A versioned analytics snapshot | Intended for off-chain indexers and analytics pipelines. It is read-only and returns a stable payload shape for downstream consumers. |
| `get_invoices_with_disputes(env)` and `get_invoices_by_dispute_status(env, dispute_status)` | Multi-invoice dispute lists | Useful for dispute dashboards. They scan the invoice status buckets and return invoice IDs that match the requested dispute state. |

## Practical usage patterns

### 1. Fetch the first page of a business’s invoices

```rust
let ids = contract.get_business_invoices_paged(
    env.clone(),
    business.clone(),
    Some(InvoiceStatus::Verified),
    0,
    10,
);
```

This returns up to 10 invoice IDs for the business. If you need the next page, advance the offset to 10 and request again.

### 2. Fetch the next page of available invoices

```rust
let ids = contract.get_available_invoices_paged(
    env.clone(),
    Some(1_000_000i128),
    None,
    Some(InvoiceCategory::Services),
    10,
    10,
);
```

The same paging rules apply: the contract clamps the requested page size to 50 and returns an empty vector once the offset is past the available set.

### 3. Use aggregate analytics when you do not need the full list

```rust
let (platform, performance) = contract.get_analytics_summary(env.clone());
```

This is the best entrypoint for dashboards that need totals, rates, and health indicators without walking every invoice ID in the client.

## Bounds cheat sheet

- Maximum page size for paged invoice reads: 50 records.
- `limit = 0` returns an empty page.
- `offset` beyond the available data returns an empty page.
- `offset` values that would overflow the pagination math are rejected by the contract query guard.
- The business and available-invoice paged reads are sorted by recency for the caller’s convenience.

## Which entrypoint should I use?

- Use `get_business_invoices_paged` or `get_available_invoices_paged` when you need a list of invoice IDs to show in a UI.
- Use `get_invoices_by_status` only when you need the full unpaged set for a batch job.
- Use `get_platform_metrics`, `get_performance_metrics`, or `get_analytics_summary` when you need the aggregate picture rather than the raw invoice list.
