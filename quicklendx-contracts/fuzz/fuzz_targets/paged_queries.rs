//! Fuzz the overflow-safe pagination primitives used by query endpoints.
#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use quicklendx_contracts::pagination::{
    calculate_safe_bounds, cap_query_limit, paginate_slice, MAX_QUERY_LIMIT,
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
    }
});
