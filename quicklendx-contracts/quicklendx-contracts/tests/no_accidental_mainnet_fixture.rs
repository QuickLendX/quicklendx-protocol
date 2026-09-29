//! Regression tests for a shared test fixture that must never be used on mainnet.
//!
//! These tests keep the boundary explicit and deterministic so a future change
//! cannot silently reintroduce a dangerous default.

#[cfg(test)]
mod fixture {
    pub const NO_ACCIDENTAL_MAINNET: &str = "TEST_ONLY_FIXTURE: do not deploy this fixture to mainnet";

    pub fn assert_non_mainnet(network: &str) -> Result<(), &'static str> {
        if network.eq_ignore_ascii_case("mainnet") {
            Err(NO_ACCIDENTAL_MAINNET)
        } else {
            Ok(())
        }
    }
}

#[test]
fn accepts_testnet_and_local_networks() {
    use self::fixture::assert_non_mainnet;

    assert!(assert_non_mainnet("testnet").is_ok());
    assert!(assert_non_mainnet("standalone").is_ok());
    assert!(assert_non_mainnet("local").is_ok());
}

#[test]
fn rejects_mainnet_for_test_fixtures() {
    use self::fixture::{assert_non_mainnet, NO_ACCIDENTAL_MAINNET};

    let err = assert_non_mainnet("mainnet").unwrap_err();
    assert_eq!(err, NO_ACCIDENTAL_MAINNET);
}
