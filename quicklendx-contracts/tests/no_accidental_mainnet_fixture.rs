use quicklendx_contracts::test_utils::{assert_non_mainnet, NO_ACCIDENTAL_MAINNET};

#[test]
fn accepts_testnet_and_local_networks() {
    assert!(assert_non_mainnet("testnet").is_ok());
    assert!(assert_non_mainnet("standalone").is_ok());
    assert!(assert_non_mainnet("local").is_ok());
}

#[test]
fn rejects_mainnet_for_test_fixtures() {
    let err = assert_non_mainnet("mainnet").unwrap_err();
    assert_eq!(err, NO_ACCIDENTAL_MAINNET);
}
