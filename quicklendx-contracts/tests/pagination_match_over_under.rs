// Tests for pagination edge cases: match, over, under scenarios
#[cfg(test)]
mod pagination_match_over_under {
    use super::super::*; // import pagination module
    use quicklendx_contracts::errors::QuickLendXError;
    use quicklendx_contracts::pagination::{
        calculate_safe_bounds, cap_query_limit, paginate_slice, validate_pagination_params,
        validate_query_params, MAX_QUERY_LIMIT,
    };

    #[test]
    fn test_validate_query_params_overflow() {
        // offset near max, adding MAX_QUERY_LIMIT would overflow
        let offset = u32::MAX - 10; // MAX_QUERY_LIMIT is 50, so overflow potential
        let limit = 20;
        let res = validate_query_params(offset, limit);
        assert!(matches!(res, Err(QuickLendXError::InvalidAmount)));
    }

    #[test]
    fn test_validate_pagination_params_offset_over_total() {
        let offset = 100;
        let limit = 10;
        let total = 50; // offset > total
        let (safe_offset, effective_limit, has_more) =
            validate_pagination_params(offset, limit, total);
        assert_eq!(safe_offset, total);
        assert_eq!(effective_limit, 0);
        assert!(!has_more);
    }

    #[test]
    fn test_calculate_safe_bounds_offset_over_collection() {
        let offset = 200;
        let limit = 30;
        let collection_size = 150;
        let (start, end) = calculate_safe_bounds(offset, limit, collection_size);
        // offset > collection size => start == collection_size, end also collection_size
        assert_eq!(start, collection_size);
        assert_eq!(end, collection_size);
    }

    #[test]
    fn test_calculate_safe_bounds_limit_capped() {
        let offset = 0;
        let limit = 100; // exceeds MAX_QUERY_LIMIT (50)
        let collection_size = 60;
        let (start, end) = calculate_safe_bounds(offset, limit, collection_size);
        // limit should be capped to 50, end = min(50, collection_size) = 50
        assert_eq!(start, 0);
        assert_eq!(end, MAX_QUERY_LIMIT.min(collection_size));
    }

    #[test]
    fn test_paginate_slice_offset_beyond_len() {
        let items: Vec<u32> = (1..=10).collect();
        let result = paginate_slice(&items, 20, 5);
        assert!(result.is_empty());
    }
}
