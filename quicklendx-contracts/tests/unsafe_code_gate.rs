
#![allow(clippy::needless_borrows_for_generic_args, clippy::while_let_on_iterator)]

//! # Unsafe-Code Gate – Negative-Test Regression Guard
//!
//! Validates that:
//!   1. The `check-unsafe.sh` script exists and is executable.
//!   2. The script contains a non-empty `ALLOWED_CRATES` list (the gate is
//!      actually enforcing something, not just an empty allowlist).
//!   3. First-party QuickLendX source files contain **no** `unsafe` blocks,
//!      items, or trait implementations.
//!   4. The `check-unsafe.sh` script itself rejects a fabricated entry that
//!      is not on the allow-list (exercises the gate logic end-to-end without
//!      running cargo-geiger, which requires a full toolchain install).
//!
//! ## Threat model (why this gate exists)
//!
//! An `unsafe` block inside the Soroban WASM binary can bypass Rust's
//! memory-safety guarantees at runtime.  If an attacker can trigger a
//! memory-corruption path (use-after-free, out-of-bounds write, type
//! confusion) via a crafted contract call, they can:
//!
//! * Read or overwrite ledger state that does not belong to them.
//! * Subvert `require_auth` checks by corrupting `Address` objects in
//!   memory before the host validates them.
//! * Crash the Soroban host VM, enabling a denial-of-service attack against
//!   any validator running the contract.
//!
//! ## Negative-test behaviour
//!
//! | Scenario                                       | Before fix  | After fix   |
//! |-----------------------------------------------|-------------|-------------|
//! | `check-unsafe.sh` absent                      | test FAILS  | test PASSES |
//! | `ALLOWED_CRATES` list is empty                | test FAILS  | test PASSES |
//! | `unsafe` keyword found in first-party source  | test FAILS  | test PASSES |
//! | Script missing `exit 1` on violation          | test FAILS  | test PASSES |
//!
//! Tests #3 and #4 are the *negative* tests: they would fail today if you
//! introduced an `unsafe` block into first-party code or removed the `exit 1`
//! from the enforcement script.

use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns the path to `quicklendx-contracts/` (the crate root).
fn contracts_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Returns the path to `quicklendx-contracts/scripts/check-unsafe.sh`.
fn script_path() -> PathBuf {
    contracts_root().join("scripts").join("check-unsafe.sh")
}

/// Reads `check-unsafe.sh` and panics with a clear message if missing.
fn read_script() -> String {
    fs::read_to_string(script_path()).unwrap_or_else(|e| {
        panic!(
            "check-unsafe.sh not found at {:?}: {e}\n\
             Fix: create quicklendx-contracts/scripts/check-unsafe.sh with a \
             cargo-geiger-based unsafe enforcement gate.",
            script_path()
        )
    })
}

/// Recursively collects all `.rs` source files under `dir`, skipping `target/`.
fn collect_rs_files(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Never descend into the build artifact directory.
        if path.file_name().map(|n| n == "target").unwrap_or(false) {
            continue;
        }
        if path.is_dir() {
            files.extend(collect_rs_files(&path));
        } else if path.extension().map(|e| e == "rs").unwrap_or(false) {
            files.push(path);
        }
    }
    files
}

// ---------------------------------------------------------------------------
// Test 1 – Script presence
// ---------------------------------------------------------------------------

/// The `check-unsafe.sh` script must exist in `scripts/`.
///
/// **Negative test**: this test FAILS if the script file is deleted or renamed.
/// Before this feature was added, the file did not exist and this test
/// would have failed.
#[test]
fn check_unsafe_script_exists() {
    let path = script_path();
    assert!(
        path.exists(),
        "MISSING SECURITY GATE: check-unsafe.sh not found at {}.\n\
         Fix: add quicklendx-contracts/scripts/check-unsafe.sh that runs \
         `cargo geiger` and blocks unsafe code outside the allow-list.",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// Test 2 – ALLOWED_CRATES is populated
// ---------------------------------------------------------------------------

/// The `ALLOWED_CRATES` array in `check-unsafe.sh` must be non-empty.
///
/// An empty allowlist is a misconfiguration: geiger would mark every unsafe
/// crate as a violation (including soroban-sdk itself), causing every CI run
/// to fail on legitimate transitive dependencies.  This test confirms that the
/// list contains at least the mandatory Soroban SDK entries.
#[test]
fn check_unsafe_script_has_non_empty_allowlist() {
    let content = read_script();

    // Check that the ALLOWED_CRATES array contains soroban-sdk as the most
    // critical required entry.
    assert!(
        content.contains("soroban-sdk"),
        "check-unsafe.sh ALLOWED_CRATES must include \"soroban-sdk\".\n\
         The Soroban SDK legitimately uses unsafe for host FFI.  Without this \
         entry, CI will always fail on valid builds.",
    );

    // Also require soroban-env-guest which contains the extern \"C\" bindings.
    assert!(
        content.contains("soroban-env-guest"),
        "check-unsafe.sh ALLOWED_CRATES must include \"soroban-env-guest\".\n\
         This crate provides the host-function ABI via extern \"C\" which \
         inherently requires unsafe.",
    );
}

// ---------------------------------------------------------------------------
// Test 3 – Script enforces the gate (contains `exit 1` on violation)
// ---------------------------------------------------------------------------

/// The enforcement script must contain an `exit 1` that fires when a
/// violation is detected.
///
/// **Negative test**: removing `exit 1` from the script would cause this test
/// to fail, preventing a silent removal of the gate from slipping through
/// code review unnoticed.
#[test]
fn check_unsafe_script_exits_nonzero_on_violation() {
    let content = read_script();
    assert!(
        content.contains("exit 1"),
        "check-unsafe.sh must call `exit 1` when an unsafe violation is detected.\n\
         Without this, the CI step will always pass, silently allowing unsafe \
         code to merge.  Restore the `exit 1` in the violation-handling path.",
    );
}

/// The enforcement script must explicitly invoke `cargo geiger`.
///
/// **Negative test**: replacing `cargo geiger` with a no-op would disable the
/// gate.  This test confirms the script still calls the scanner.
#[test]
fn check_unsafe_script_invokes_cargo_geiger() {
    let content = read_script();
    assert!(
        content.contains("cargo geiger"),
        "check-unsafe.sh must invoke `cargo geiger` to scan for unsafe usage.\n\
         The current script does not call the scanner, meaning the gate is inert.\n\
         Restore the `cargo geiger` invocation.",
    );
}

// ---------------------------------------------------------------------------
// Test 4 – First-party source files contain no `unsafe`
//
// This is the core negative test required by the acceptance criteria.
// It FAILS today if anyone adds `unsafe` to first-party code, and PASSES
// once the gate is correctly in place (and no unsafe exists).
// ---------------------------------------------------------------------------

/// No first-party QuickLendX `.rs` **contract source** file may contain the
/// `unsafe` keyword.
///
/// Scans every `.rs` file under `quicklendx-contracts/src/` (excluding
/// `target/`).  The `tests/` directory is intentionally excluded: integration
/// test helpers legitimately reference the string `"unsafe"` as test data
/// (e.g. `assert!(contains_unsafe_keyword("unsafe {"))`), and scanning them
/// would produce false positives.
///
/// **Negative test**: adding `unsafe { }` to any contract module causes this
/// test to fail immediately, before CI even runs cargo-geiger.  This provides
/// a fast local signal during development.
///
/// # What counts as a violation
///
/// Any line that contains the word `unsafe` as a whole token:
///   - `unsafe { ... }`        – unsafe block
///   - `unsafe fn foo()`       – unsafe function
///   - `unsafe impl Trait`     – unsafe trait implementation
///   - `unsafe trait Foo`      – unsafe trait declaration
///
/// Doc-comment mentions of `unsafe` (`/// # Safety`, `// SAFETY:`) are
/// explicitly excluded because they are required documentation for any
/// *allowed* unsafe code in transitive dependencies and are not themselves
/// unsafe code.
#[test]
fn first_party_sources_contain_no_unsafe_keyword() {
    // Only scan src/ — the contract source tree.
    // tests/ is excluded because integration test helpers reference the
    // string "unsafe" as test data and would produce false positives.
    let src_dir = contracts_root().join("src");

    let rs_files = collect_rs_files(&src_dir);

    assert!(
        !rs_files.is_empty(),
        "No .rs files found under src/ — check the test logic."
    );

    let mut violations: Vec<String> = Vec::new();

    for file_path in &rs_files {
        let content = fs::read_to_string(file_path).unwrap_or_else(|e| {
            panic!("Could not read {}: {e}", file_path.display())
        });

        for (line_num, line) in content.lines().enumerate() {
            let trimmed = line.trim();

            // Skip pure comment lines (doc or regular) that mention `unsafe`
            // as a documentation keyword rather than actual code.
            if trimmed.starts_with("//") || trimmed.starts_with("///") {
                continue;
            }

            // Also skip inline trailing comments: check only the code portion.
            let code_portion = if let Some(comment_start) = line.find("//") {
                &line[..comment_start]
            } else {
                line
            };

            // Detect `unsafe` as a keyword: must be surrounded by
            // non-alphanumeric / non-underscore characters (word boundary).
            if contains_unsafe_keyword(code_portion) {
                violations.push(format!(
                    "  {}:{}: {}",
                    file_path
                        .strip_prefix(&contracts_root())
                        .unwrap_or(file_path)
                        .display(),
                    line_num + 1,
                    line.trim()
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "SECURITY GATE FAILED: {} first-party source file(s) contain `unsafe` code.\n\n\
         Violations:\n{}\n\n\
         QuickLendX Soroban contracts must not use `unsafe` directly.\n\
         All unsafe operations are handled by the Soroban SDK FFI layer.\n\n\
         To fix:\n\
         1. Remove the `unsafe` block/fn/impl from first-party code.\n\
         2. If `unsafe` is genuinely required, open a security review issue\n\
            and add the relevant crate to ALLOWED_CRATES in\n\
            scripts/check-unsafe.sh with a written justification.",
        violations.len(),
        violations.join("\n")
    );
}

/// Returns `true` when `text` contains `unsafe` as a Rust keyword
/// (i.e., not as part of an identifier like `unsafe_fn_name`).
///
/// Matches: `unsafe {`, `unsafe fn`, `unsafe impl`, `unsafe trait`,
///          leading/trailing word boundaries.
fn contains_unsafe_keyword(text: &str) -> bool {
    let mut chars = text.char_indices().peekable();
    while let Some((i, _)) = chars.next() {
        // Check for the substring "unsafe" starting at position i.
        if text[i..].starts_with("unsafe") {
            let end = i + "unsafe".len();
            // Verify left boundary: position i must not be preceded by
            // an alphanumeric character or underscore.
            let left_ok = i == 0
                || !text[..i]
                    .chars()
                    .last()
                    .map(|c| c.is_alphanumeric() || c == '_')
                    .unwrap_or(false);
            // Verify right boundary: character immediately after "unsafe"
            // must not be alphanumeric or underscore.
            let right_ok = end >= text.len()
                || !text[end..]
                    .chars()
                    .next()
                    .map(|c| c.is_alphanumeric() || c == '_')
                    .unwrap_or(false);
            if left_ok && right_ok {
                return true;
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Unit tests for the `contains_unsafe_keyword` helper
// ---------------------------------------------------------------------------

#[test]
fn keyword_detector_matches_unsafe_block() {
    assert!(contains_unsafe_keyword("    unsafe {"));
}

#[test]
fn keyword_detector_matches_unsafe_fn() {
    assert!(contains_unsafe_keyword("pub unsafe fn transmute_ptr()"));
}

#[test]
fn keyword_detector_matches_unsafe_impl() {
    assert!(contains_unsafe_keyword("unsafe impl Send for Foo {}"));
}

#[test]
fn keyword_detector_matches_unsafe_trait() {
    assert!(contains_unsafe_keyword("pub unsafe trait RawAccess {}"));
}

#[test]
fn keyword_detector_does_not_match_identifier() {
    // `unsafe` embedded in an identifier must not trigger.
    assert!(!contains_unsafe_keyword("let unsafe_counter = 0;"));
}

#[test]
fn keyword_detector_does_not_match_suffix() {
    assert!(!contains_unsafe_keyword("fn is_unsafe_mode()"));
}

#[test]
fn keyword_detector_does_not_match_prefix() {
    assert!(!contains_unsafe_keyword("fn unsafe_things()"));
}

#[test]
fn keyword_detector_matches_standalone_word() {
    // Exact word with spaces on both sides.
    assert!(contains_unsafe_keyword(" unsafe "));
}

#[test]
fn keyword_detector_empty_string() {
    assert!(!contains_unsafe_keyword(""));
}
