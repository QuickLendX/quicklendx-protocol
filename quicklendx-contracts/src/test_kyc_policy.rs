#![cfg(test)]
//! Direct tests for `kyc_policy`: cursor encoding/decoding and the paged KYC
//! list queries (`get_businesses_paged`, `get_investors_paged`).
//!
//! The queries are called directly inside the contract context (rather than
//! through the generated client) so these tests exercise the module itself.
//! Seeding goes through the real `verification` entry points, so the KYC
//! indexes are populated exactly as in production.
//!
//! Complements `test_pagination_cursors.rs`, which covers invoice/bid cursors.

use crate::admin::AdminStorage;
use crate::kyc_policy::{
    decode_cursor, encode_cursor, get_businesses_paged, get_investors_paged, KycPageResult,
};
use crate::pagination::MAX_QUERY_LIMIT;
use crate::verification::{
    reject_business, reject_investor, submit_investor_kyc, submit_kyc_application,
    verify_business, verify_investor, BusinessVerificationStatus, BusinessVerificationStorage,
    InvestorVerificationStorage,
};
use crate::{QuickLendXContract, QuickLendXError};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    Address, Env, String, Vec,
};

const SEEDED: u32 = 25;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

struct Ctx {
    env: Env,
    contract_id: Address,
    admin: Address,
}

fn setup() -> Ctx {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    let contract_id = env.register(QuickLendXContract, ());
    let admin = Address::generate(&env);
    env.as_contract(&contract_id, || AdminStorage::initialize(&env, &admin))
        .unwrap();
    Ctx {
        env,
        contract_id,
        admin,
    }
}

/// Status assigned to the `i`-th seeded participant: a deterministic mix of
/// 9 verified, 8 rejected and 8 pending out of 25.
fn status_for(i: u32) -> BusinessVerificationStatus {
    match i % 3 {
        0 => BusinessVerificationStatus::Verified,
        1 => BusinessVerificationStatus::Rejected,
        _ => BusinessVerificationStatus::Pending,
    }
}

fn expected_count(status: &BusinessVerificationStatus, n: u32) -> u32 {
    (0..n).filter(|i| status_for(*i) == *status).count() as u32
}

/// Submit `n` business KYC applications at increasing ledger timestamps and
/// move each one into the status given by [`status_for`].
fn seed_businesses(ctx: &Ctx, n: u32) -> Vec<Address> {
    let env = &ctx.env;
    let mut all = Vec::new(env);
    for i in 0..n {
        let business = Address::generate(env);
        env.ledger().set_timestamp(1_000 + u64::from(i));
        env.as_contract(&ctx.contract_id, || {
            submit_kyc_application(env, &business, String::from_str(env, "kyc-data")).unwrap();
            match status_for(i) {
                BusinessVerificationStatus::Verified => {
                    verify_business(env, &ctx.admin, &business).unwrap()
                }
                BusinessVerificationStatus::Rejected => reject_business(
                    env,
                    &ctx.admin,
                    &business,
                    String::from_str(env, "rejected"),
                )
                .unwrap(),
                BusinessVerificationStatus::Pending => {}
            }
        });
        all.push_back(business);
    }
    all
}

/// Investor counterpart of [`seed_businesses`].
fn seed_investors(ctx: &Ctx, n: u32, mixed: bool) -> Vec<Address> {
    let env = &ctx.env;
    let mut all = Vec::new(env);
    for i in 0..n {
        let investor = Address::generate(env);
        env.ledger().set_timestamp(1_000 + u64::from(i));
        let status = if mixed {
            status_for(i)
        } else {
            BusinessVerificationStatus::Pending
        };
        env.as_contract(&ctx.contract_id, || {
            submit_investor_kyc(env, &investor, String::from_str(env, "kyc-data")).unwrap();
            match status {
                BusinessVerificationStatus::Verified => {
                    verify_investor(env, &ctx.admin, &investor, 10_000).unwrap();
                }
                BusinessVerificationStatus::Rejected => reject_investor(
                    env,
                    &ctx.admin,
                    &investor,
                    String::from_str(env, "rejected"),
                )
                .unwrap(),
                BusinessVerificationStatus::Pending => {}
            }
        });
        all.push_back(investor);
    }
    all
}

fn businesses_page(
    ctx: &Ctx,
    status: BusinessVerificationStatus,
    cursor: Option<String>,
    limit: u32,
) -> Result<KycPageResult, QuickLendXError> {
    ctx.env.as_contract(&ctx.contract_id, || {
        get_businesses_paged(&ctx.env, status, cursor, limit)
    })
}

fn investors_page(
    ctx: &Ctx,
    status: BusinessVerificationStatus,
    cursor: Option<String>,
    limit: u32,
) -> Result<KycPageResult, QuickLendXError> {
    ctx.env.as_contract(&ctx.contract_id, || {
        get_investors_paged(&ctx.env, status, cursor, limit)
    })
}

/// Follow `next_cursor` from the first page until it is `None`, asserting the
/// page-level invariants on the way, and return every item in walk order.
fn walk<F>(ctx: &Ctx, limit: u32, mut fetch: F) -> Vec<Address>
where
    F: FnMut(Option<String>, u32) -> Result<KycPageResult, QuickLendXError>,
{
    let mut collected = Vec::new(&ctx.env);
    let mut cursor: Option<String> = None;
    let mut pages = 0u32;
    loop {
        let page = fetch(cursor.clone(), limit).unwrap();
        pages += 1;
        assert!(pages <= SEEDED + 1, "pagination did not terminate");
        assert!(page.items.len() <= limit.min(MAX_QUERY_LIMIT));
        for item in page.items.iter() {
            collected.push_back(item);
        }
        match page.next_cursor {
            Some(next) => {
                // A non-final page is full and the cursor points just past it.
                assert_eq!(page.items.len(), limit.min(MAX_QUERY_LIMIT));
                assert_eq!(decode_cursor(&next).unwrap(), collected.len());
                cursor = Some(next);
            }
            None => {
                // The last page ends exactly at the reported total.
                assert_eq!(collected.len(), page.total_count);
                break;
            }
        }
    }
    collected
}

fn occurrences(list: &Vec<Address>, needle: &Address) -> u32 {
    list.iter().filter(|a| a == needle).count() as u32
}

// ---------------------------------------------------------------------------
// Cursor encoding / decoding
// ---------------------------------------------------------------------------

#[test]
fn test_kyc_policy_cursor_round_trip() {
    let env = Env::default();
    let samples: [u32; 12] = [
        0,
        1,
        7,
        9,
        10,
        49,
        50,
        99,
        1_000,
        123_456,
        u32::MAX - 1,
        u32::MAX,
    ];
    for offset in samples {
        let cursor = encode_cursor(&env, offset);
        assert_eq!(decode_cursor(&cursor), Ok(offset));
    }
}

#[test]
fn test_kyc_policy_cursor_encoding_is_plain_decimal() {
    let env = Env::default();
    assert_eq!(encode_cursor(&env, 0), String::from_str(&env, "0"));
    assert_eq!(encode_cursor(&env, 42), String::from_str(&env, "42"));
    assert_eq!(
        encode_cursor(&env, u32::MAX),
        String::from_str(&env, "4294967295")
    );
}

#[test]
fn test_kyc_policy_cursor_accepts_leading_zeros_up_to_ten_chars() {
    let env = Env::default();
    assert_eq!(decode_cursor(&String::from_str(&env, "007")), Ok(7));
    assert_eq!(decode_cursor(&String::from_str(&env, "0000000000")), Ok(0));
}

#[test]
fn test_kyc_policy_malformed_cursors_return_typed_error() {
    let env = Env::default();
    let malformed = [
        "",
        " ",
        "-1",
        "+1",
        " 1",
        "1 ",
        "1.0",
        "1e3",
        "0x10",
        "abc",
        "12abc",
        "١٢",          // non-ASCII digits
        "4294967296",  // u32::MAX + 1 (fits in 10 chars, overflows u32)
        "9999999999",  // 10 chars, overflows u32
        "00000000001", // 11 chars: over the length cap even though the value is small
        "99999999999",
    ];
    for raw in malformed {
        assert_eq!(
            decode_cursor(&String::from_str(&env, raw)),
            Err(QuickLendXError::InvalidCursor),
            "cursor {:?} should be rejected",
            raw
        );
    }
}

/// Fuzz-style sweep: deterministic pseudo-random byte strings that always
/// contain at least one non-digit byte must be rejected, never panic.
#[test]
fn test_kyc_policy_fuzzed_cursors_with_non_digits_are_rejected() {
    let env = Env::default();
    let mut state: u32 = 0x9E37_79B9;
    let mut next = || {
        // xorshift32
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state
    };

    for _ in 0..500 {
        let len = (next() % 10 + 1) as usize;
        let mut buf = [0u8; 10];
        for b in buf.iter_mut().take(len) {
            *b = (next() & 0xFF) as u8;
        }
        // Force one byte outside b'0'..=b'9'.
        let pos = (next() as usize) % len;
        let mut bad = (next() & 0xFF) as u8;
        if bad.is_ascii_digit() {
            bad = b'/';
        }
        buf[pos] = bad;

        let cursor = String::from_bytes(&env, &buf[..len]);
        assert_eq!(decode_cursor(&cursor), Err(QuickLendXError::InvalidCursor));
    }
}

/// Every all-digit string that fits in a u32 decodes to its numeric value.
#[test]
fn test_kyc_policy_fuzzed_digit_cursors_decode_to_their_value() {
    let env = Env::default();
    let mut state: u32 = 0x1234_5678;
    for _ in 0..500 {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let len = (state % 9 + 1) as usize; // <= 9 digits always fits in u32
        let mut buf = [0u8; 10];
        let mut expected: u32 = 0;
        let mut s = state;
        for b in buf.iter_mut().take(len) {
            let digit = (s % 10) as u8;
            s = s / 10 + 7;
            *b = b'0' + digit;
            expected = expected * 10 + u32::from(digit);
        }
        let cursor = String::from_bytes(&env, &buf[..len]);
        assert_eq!(decode_cursor(&cursor), Ok(expected));
    }
}

#[test]
fn test_kyc_policy_paged_queries_reject_malformed_cursor_without_side_effects() {
    let ctx = setup();
    seed_businesses(&ctx, SEEDED);
    seed_investors(&ctx, SEEDED, true);

    let before_b = businesses_page(&ctx, BusinessVerificationStatus::Verified, None, 50).unwrap();
    let before_i = investors_page(&ctx, BusinessVerificationStatus::Pending, None, 50).unwrap();

    for raw in ["abc", "-1", "4294967296", ""] {
        let bad = Some(String::from_str(&ctx.env, raw));
        assert_eq!(
            businesses_page(&ctx, BusinessVerificationStatus::Verified, bad.clone(), 10),
            Err(QuickLendXError::InvalidCursor)
        );
        assert_eq!(
            investors_page(&ctx, BusinessVerificationStatus::Pending, bad, 10),
            Err(QuickLendXError::InvalidCursor)
        );
    }

    // Invalid cursors leave the indexes untouched.
    assert_eq!(
        businesses_page(&ctx, BusinessVerificationStatus::Verified, None, 50).unwrap(),
        before_b
    );
    assert_eq!(
        investors_page(&ctx, BusinessVerificationStatus::Pending, None, 50).unwrap(),
        before_i
    );
}

// ---------------------------------------------------------------------------
// Paged walks over 25 seeded participants with mixed statuses
// ---------------------------------------------------------------------------

fn assert_business_walk_covers_status(limit: u32) {
    let ctx = setup();
    let seeded = seed_businesses(&ctx, SEEDED);

    let statuses = [
        BusinessVerificationStatus::Verified,
        BusinessVerificationStatus::Rejected,
        BusinessVerificationStatus::Pending,
    ];
    let mut seen_total = 0u32;
    for status in statuses {
        let walked = walk(&ctx, limit, |cursor, l| {
            businesses_page(&ctx, status.clone(), cursor, l)
        });

        // Status filtering: exactly the seeded participants with this status.
        assert_eq!(walked.len(), expected_count(&status, SEEDED));
        for (i, business) in seeded.iter().enumerate() {
            let expected = if status_for(i as u32) == status { 1 } else { 0 };
            assert_eq!(occurrences(&walked, &business), expected);
        }
        ctx.env.as_contract(&ctx.contract_id, || {
            for business in walked.iter() {
                let record =
                    BusinessVerificationStorage::get_verification(&ctx.env, &business).unwrap();
                assert_eq!(record.status, status);
            }
        });

        // Walking with a small page size yields the same deterministic order
        // as fetching everything in one page.
        let single = businesses_page(&ctx, status.clone(), None, MAX_QUERY_LIMIT).unwrap();
        assert!(single.next_cursor.is_none());
        assert_eq!(single.items, walked);

        seen_total += walked.len();
    }
    // Every seeded record appears in exactly one status list.
    assert_eq!(seen_total, SEEDED);
}

#[test]
fn test_kyc_policy_business_walk_page_size_1() {
    assert_business_walk_covers_status(1);
}

#[test]
fn test_kyc_policy_business_walk_page_size_4() {
    assert_business_walk_covers_status(4);
}

#[test]
fn test_kyc_policy_business_walk_page_size_9_ends_exactly_on_page_boundary() {
    // 9 verified businesses: one full page, then next_cursor must be None.
    let ctx = setup();
    seed_businesses(&ctx, SEEDED);
    let page = businesses_page(&ctx, BusinessVerificationStatus::Verified, None, 9).unwrap();
    assert_eq!(page.items.len(), 9);
    assert_eq!(page.total_count, 9);
    assert!(page.next_cursor.is_none());
}

#[test]
fn test_kyc_policy_investor_walk_covers_every_record_exactly_once() {
    let ctx = setup();
    let seeded = seed_investors(&ctx, SEEDED, true);

    let statuses = [
        BusinessVerificationStatus::Verified,
        BusinessVerificationStatus::Rejected,
        BusinessVerificationStatus::Pending,
    ];
    let mut seen_total = 0u32;
    for status in statuses {
        let walked = walk(&ctx, 3, |cursor, l| {
            investors_page(&ctx, status.clone(), cursor, l)
        });

        assert_eq!(walked.len(), expected_count(&status, SEEDED));
        for (i, investor) in seeded.iter().enumerate() {
            let expected = if status_for(i as u32) == status { 1 } else { 0 };
            assert_eq!(occurrences(&walked, &investor), expected);
        }
        ctx.env.as_contract(&ctx.contract_id, || {
            for investor in walked.iter() {
                let record = InvestorVerificationStorage::get(&ctx.env, &investor).unwrap();
                assert!(record.status == status);
            }
        });

        seen_total += walked.len();
    }
    assert_eq!(seen_total, SEEDED);
}

#[test]
fn test_kyc_policy_last_page_has_no_next_cursor() {
    let ctx = setup();
    seed_businesses(&ctx, SEEDED);

    // 9 verified, page size 4 -> pages of 4, 4, 1.
    let first = businesses_page(&ctx, BusinessVerificationStatus::Verified, None, 4).unwrap();
    assert_eq!(first.items.len(), 4);
    assert_eq!(first.total_count, 9);
    assert_eq!(first.next_cursor, Some(String::from_str(&ctx.env, "4")));

    let second =
        businesses_page(&ctx, BusinessVerificationStatus::Verified, first.next_cursor, 4)
            .unwrap();
    assert_eq!(second.items.len(), 4);
    assert_eq!(second.next_cursor, Some(String::from_str(&ctx.env, "8")));

    let last =
        businesses_page(&ctx, BusinessVerificationStatus::Verified, second.next_cursor, 4)
            .unwrap();
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.total_count, 9);
    assert!(last.next_cursor.is_none());
}

#[test]
fn test_kyc_policy_cursor_zero_matches_no_cursor() {
    let ctx = setup();
    seed_businesses(&ctx, SEEDED);
    let none = businesses_page(&ctx, BusinessVerificationStatus::Pending, None, 5).unwrap();
    let zero = businesses_page(
        &ctx,
        BusinessVerificationStatus::Pending,
        Some(String::from_str(&ctx.env, "0")),
        5,
    )
    .unwrap();
    assert_eq!(none, zero);
}

#[test]
fn test_kyc_policy_cursor_past_end_returns_empty_page() {
    let ctx = setup();
    seed_businesses(&ctx, SEEDED);
    for raw in ["8", "9", "1000", "4294967295"] {
        let page = businesses_page(
            &ctx,
            BusinessVerificationStatus::Rejected,
            Some(String::from_str(&ctx.env, raw)),
            10,
        )
        .unwrap();
        assert!(page.items.is_empty(), "cursor {raw} should be past the end");
        assert!(page.next_cursor.is_none());
        assert_eq!(page.total_count, 8);
    }
}

#[test]
fn test_kyc_policy_empty_lists_return_empty_pages() {
    let ctx = setup();
    for status in [
        BusinessVerificationStatus::Verified,
        BusinessVerificationStatus::Rejected,
        BusinessVerificationStatus::Pending,
    ] {
        let b = businesses_page(&ctx, status.clone(), None, 10).unwrap();
        assert!(b.items.is_empty());
        assert!(b.next_cursor.is_none());
        assert_eq!(b.total_count, 0);

        let i = investors_page(&ctx, status, None, 10).unwrap();
        assert!(i.items.is_empty());
        assert!(i.next_cursor.is_none());
        assert_eq!(i.total_count, 0);
    }
}

// ---------------------------------------------------------------------------
// Page size clamping
// ---------------------------------------------------------------------------

#[test]
fn test_kyc_policy_page_size_is_clamped_to_max_query_limit() {
    let ctx = setup();
    let over = MAX_QUERY_LIMIT + 10;
    seed_investors(&ctx, over, false);

    for limit in [MAX_QUERY_LIMIT + 1, 1_000, u32::MAX] {
        let page = investors_page(&ctx, BusinessVerificationStatus::Pending, None, limit).unwrap();
        assert_eq!(page.items.len(), MAX_QUERY_LIMIT);
        assert_eq!(page.total_count, over);
        assert_eq!(
            page.next_cursor,
            Some(encode_cursor(&ctx.env, MAX_QUERY_LIMIT))
        );
    }

    let rest = investors_page(
        &ctx,
        BusinessVerificationStatus::Pending,
        Some(encode_cursor(&ctx.env, MAX_QUERY_LIMIT)),
        u32::MAX,
    )
    .unwrap();
    assert_eq!(rest.items.len(), 10);
    assert!(rest.next_cursor.is_none());
}

#[test]
fn test_kyc_policy_zero_limit_returns_empty_page_without_cursor() {
    let ctx = setup();
    seed_businesses(&ctx, SEEDED);
    let page = businesses_page(&ctx, BusinessVerificationStatus::Verified, None, 0).unwrap();
    assert!(page.items.is_empty());
    assert!(page.next_cursor.is_none());
    assert_eq!(page.total_count, 9);
}
