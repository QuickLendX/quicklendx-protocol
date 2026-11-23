//! Fuzz the overflow-safe pagination primitives used by query endpoints.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use quicklendx_contracts::pagination::{
    calculate_safe_bounds, cap_query_limit, paginate_slice, pagination_metadata,
    validate_pagination_params, validate_query_params, PageCursor, MAX_QUERY_LIMIT,
};

#[derive(Arbitrary, Debug)]
struct Input {
    items: Vec<u32>,
    queries: Vec<Query>,
}

#[derive(Arbitrary, Debug)]
struct Query {
    offset: u32,
    limit: u32,
    generation: u64,
    current_generation: u64,
}

fuzz_target!(|input: Input| {
    // Bound input allocations and the number of queries per iteration.
    let items: Vec<u32> = input.items.into_iter().take(256).collect();

    for query in input.queries.into_iter().take(32) {
        let size = items.len() as u32;
        let (start, end) = calculate_safe_bounds(query.offset, query.limit, size);
        assert!(start <= end);
        assert!(end <= size);
        assert!(end - start <= MAX_QUERY_LIMIT);
        assert!(cap_query_limit(query.limit) <= MAX_QUERY_LIMIT);

        let page = paginate_slice(&items, query.offset, query.limit);
        assert!(page.len() <= MAX_QUERY_LIMIT as usize);
        assert_eq!(page.len(), (end - start) as usize);
        assert_eq!(page, items[start as usize..end as usize]);

        let (safe_offset, effective_limit, has_more) =
            validate_pagination_params(query.offset, query.limit, size);
        assert!(safe_offset <= size);
        assert!(effective_limit <= MAX_QUERY_LIMIT);
        assert!(effective_limit <= size.saturating_sub(safe_offset));
        assert_eq!(
            has_more,
            effective_limit > 0
                && safe_offset.saturating_add(effective_limit) < size
        );
        assert_eq!(
            pagination_metadata(query.offset, query.limit, size),
            (size, query.offset.saturating_add(cap_query_limit(query.limit)) < size)
        );
        let _ = validate_query_params(query.offset, query.limit);

        let cursor = PageCursor::new(query.offset, query.generation);
        let encoded = cursor.encode();
        assert_eq!(PageCursor::decode(&encoded), Ok(cursor));
        assert_eq!(
            cursor.require_stable(query.current_generation).is_ok(),
            query.generation == query.current_generation
        );
        if let Some(next) = cursor.next_cursor(query.limit, size) {
            assert!(next.offset > cursor.offset);
            assert!(next.offset < size);
            assert_eq!(next.generation, cursor.generation);
        }
    }
});
