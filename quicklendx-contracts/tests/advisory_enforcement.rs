#![allow(clippy::disallowed_methods)]
//! # Advisory Enforcement – Negative-Test Regression Guard
//!
//! Validates that `deny.toml` is configured to **fail the build** for any
//! crate with an open security advisory (RustSec / CVE).
//!
//! ## Why this test exists (threat model)
//!
//! Without `vulnerability = "deny"` in `deny.toml`, a dependency with a
//! known CVE (e.g., memory corruption, SSRF, privilege escalation) compiles
//! silently. CI shows green, reviewers see no warning, and the vulnerable
//! code ships to production WASM. This test acts as a compile-time tripwire:
//! if someone weakens the advisory policy (sets "allow" or "warn"), this test
//! fails immediately and blocks the PR.
//!
//! ## Negative-test behaviour
//!
//! * **Before fix** (vulnerability = "allow" or missing): test FAILS.
//! * **After fix** (vulnerability = "deny"):              test PASSES.

use std::fs;
use std::path::Path;

/// Confirm that the workspace `deny.toml` enforces `vulnerability = "deny"`.
///
/// This is a *negative test*: it would have failed before the fix that set
/// `vulnerability = "deny"`.  Weakening the policy to `"warn"` or `"allow"`
/// causes this test to fail, blocking the regressive change in CI.
#[test]
fn deny_toml_blocks_vulnerable_crates() {
    // Path is relative to the Cargo.toml of this crate; deny.toml lives one
    // directory above at the workspace root.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let deny_path = Path::new(manifest_dir).join("../deny.toml");

    let contents = fs::read_to_string(&deny_path).unwrap_or_else(|e| {
        panic!(
            "deny.toml not found at {:?}: {e}\n\
             Fix: create deny.toml at the workspace root with `vulnerability = \"deny\"`.",
            deny_path
        )
    });

    let table: toml::Value = contents.parse().unwrap_or_else(|e| {
        panic!("deny.toml is not valid TOML: {e}");
    });

    let vulnerability_policy = table
        .get("advisories")
        .and_then(|a| a.get("vulnerability"))
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            panic!(
                "deny.toml is missing [advisories].vulnerability.\n\
                 Fix: add `vulnerability = \"deny\"` under the [advisories] section."
            )
        });

    assert_eq!(
        vulnerability_policy, "deny",
        "SECURITY REGRESSION: deny.toml [advisories].vulnerability must be \"deny\" to \
         auto-fail CI when a crate with an open advisory is added to the dependency tree. \
         Current value: {vulnerability_policy:?}. \
         Change it back to `vulnerability = \"deny\"` to restore the security baseline."
    );
}

/// Confirm that yanked crate versions are at minimum warned about.
///
/// Yanked crates are often yanked *because* of security issues discovered
/// after release.  Silently allowing them is a weaker but still risky gap.
#[test]
fn deny_toml_warns_on_yanked_crates() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let deny_path = Path::new(manifest_dir).join("../deny.toml");
    let contents = fs::read_to_string(&deny_path).expect("deny.toml not found");
    let table: toml::Value = contents.parse().expect("deny.toml is not valid TOML");

    let yanked_policy = table
        .get("advisories")
        .and_then(|a| a.get("yanked"))
        .and_then(|v| v.as_str())
        .unwrap_or("warn"); // default is warn if absent — acceptable

    assert!(
        matches!(yanked_policy, "warn" | "deny"),
        "deny.toml [advisories].yanked should be \"warn\" or \"deny\", got {yanked_policy:?}"
    );
}
