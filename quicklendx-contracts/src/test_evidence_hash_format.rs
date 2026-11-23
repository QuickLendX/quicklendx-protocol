/// Evidence hash format tests (issue #1839).
///
/// # Coverage
///
/// Locks in that `validate_evidence_hash` requires exactly
/// [`EVIDENCE_HASH_LENGTH`] (32) bytes — the size of a `BytesN<32>` /
/// SHA-256 digest. Wrong lengths must fail with `InvalidDisputeEvidence`.
///
/// | Test name | Input length | Expected outcome |
/// |---|---|---|
/// | `evidence_hash_exactly_32_bytes_accepted` | 32 | Ok(()) |
/// | `evidence_hash_sha256_like_bytes_accepted` | 32 | Ok(()) |
/// | `evidence_hash_empty_rejected` | 0 | Err(InvalidDisputeEvidence) |
/// | `evidence_hash_one_under_32_rejected` | 31 | Err(InvalidDisputeEvidence) |
/// | `evidence_hash_one_over_32_rejected` | 33 | Err(InvalidDisputeEvidence) |
///
/// These tests are `#[cfg(test)]` (not `legacy-tests`) so they run on every
/// default CI `cargo test` matrix entry.
#[cfg(test)]
mod test_evidence_hash_format {
    use crate::errors::QuickLendXError;
    use crate::verification::{validate_evidence_hash, EVIDENCE_HASH_LENGTH};
    use soroban_sdk::{Bytes, Env};

    fn make_hash(env: &Env, len: u32) -> Bytes {
        let mut bytes = Bytes::new(env);
        for i in 0..len {
            bytes.push_back((i % 251) as u8);
        }
        bytes
    }

    /// Exactly 32 bytes must be accepted — the `BytesN<32>` boundary.
    #[test]
    fn evidence_hash_exactly_32_bytes_accepted() {
        let env = Env::default();
        let hash = make_hash(&env, EVIDENCE_HASH_LENGTH);
        assert!(
            validate_evidence_hash(&hash).is_ok(),
            "32-byte evidence hash must be accepted"
        );
    }

    /// A SHA-256-shaped 32-byte digest must be accepted.
    #[test]
    fn evidence_hash_sha256_like_bytes_accepted() {
        let env = Env::default();
        let digest = [
            0xa1, 0xb2, 0xc3, 0xd4, 0xe5, 0xf6, 0x01, 0x12, 0x23, 0x34, 0x45, 0x56, 0x67, 0x78,
            0x89, 0x9a, 0xab, 0xbc, 0xcd, 0xde, 0xef, 0xf0, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66,
            0x77, 0x88, 0x99, 0xaa,
        ];
        let hash = Bytes::from_slice(&env, &digest);
        assert_eq!(hash.len(), EVIDENCE_HASH_LENGTH);
        assert!(
            validate_evidence_hash(&hash).is_ok(),
            "SHA-256-length digest must be accepted"
        );
    }

    /// Empty hash bytes must be rejected.
    #[test]
    fn evidence_hash_empty_rejected() {
        let env = Env::default();
        let hash = make_hash(&env, 0);
        let result = validate_evidence_hash(&hash);
        assert!(result.is_err(), "empty evidence hash must be rejected");
        assert_eq!(result.unwrap_err(), QuickLendXError::InvalidDisputeEvidence);
    }

    /// 31 bytes (one under the `BytesN<32>` boundary) must be rejected.
    #[test]
    fn evidence_hash_one_under_32_rejected() {
        let env = Env::default();
        let hash = make_hash(&env, EVIDENCE_HASH_LENGTH - 1);
        let result = validate_evidence_hash(&hash);
        assert!(
            result.is_err(),
            "31-byte evidence hash must be rejected (one under 32)"
        );
        assert_eq!(result.unwrap_err(), QuickLendXError::InvalidDisputeEvidence);
    }

    /// 33 bytes (one over the `BytesN<32>` boundary) must be rejected.
    #[test]
    fn evidence_hash_one_over_32_rejected() {
        let env = Env::default();
        let hash = make_hash(&env, EVIDENCE_HASH_LENGTH + 1);
        let result = validate_evidence_hash(&hash);
        assert!(
            result.is_err(),
            "33-byte evidence hash must be rejected (one over 32)"
        );
        assert_eq!(result.unwrap_err(), QuickLendXError::InvalidDisputeEvidence);
    }

    /// `EVIDENCE_HASH_LENGTH` must stay locked at 32.
    #[test]
    fn evidence_hash_length_constant_is_32() {
        assert_eq!(
            EVIDENCE_HASH_LENGTH, 32,
            "EVIDENCE_HASH_LENGTH must be 32; update this test intentionally if changing the format"
        );
    }
}
