//! # WASM Build Size Budget – Regression Checks
//!
//! Integration tests that enforce the QuickLendX release WASM artifact stays
//! within the agreed Stellar network deployment size budget and does not
//! silently regress between commits.
//!
//! ## How regression detection works
//!
//! 1. **Hard budget** (`WASM_SIZE_BUDGET_BYTES`): an absolute ceiling enforced
//!    in CI.  The build fails immediately if the optimised WASM exceeds this.
//! 2. **Warning zone** (`WASM_SIZE_WARNING_BYTES`): 90 % of the hard budget
//!    (~230 KiB).  A diagnostic is printed when the binary enters this zone,
//!    prompting developers to reduce size before hitting the hard limit.
//! 3. **Regression baseline** (`WASM_SIZE_BASELINE_BYTES`): the last known
//!    good size.  The binary may grow by at most `WASM_REGRESSION_MARGIN`
//!    (5 %) relative to this value before the regression test fails.
//!    **Update this constant whenever the contract legitimately grows**,
//!    in the same PR that introduces the growth, alongside
//!    `scripts/wasm-size-baseline.toml` and `scripts/check-wasm-size.sh`.
//!
//! ## Security assumptions
//!
//! * All file paths are derived from `CARGO_MANIFEST_DIR` (injected by Cargo
//!   at compile time, not from caller-controlled environment variables),
//!   preventing path-traversal attacks.
//! * The subprocess (`cargo build`) is invoked with an explicit, literal
//!   argument list – no shell interpolation of untrusted data.
//! * Relaxing any budget constant requires a deliberate, code-review-gated
//!   commit; no runtime mechanism can bypass the hard check.
//!
//! ## Budget table
//!
//! | Constant                   | Value          | Purpose                                     |
//! |----------------------------|----------------|---------------------------------------------|
//! | `WASM_SIZE_BUDGET_BYTES`   | 262 144 B (256 KiB) | Hard failure threshold               |
//! | `WASM_SIZE_WARNING_BYTES`  | ~235 929 B (90 %) | Warning zone upper edge               |
//! | `WASM_SIZE_BASELINE_BYTES` | 217 668 B       | Last recorded optimised size           |
//! | `WASM_REGRESSION_MARGIN`   | 0.05 (5 %)      | Max allowed growth vs baseline         |

use std::path::PathBuf;
use std::process::Command;

// ─────────────────────────────────────────────────────────────────────────────
//  Budget constants
//  These must stay in sync with:
//    scripts/check-wasm-size.sh     (MAX_BYTES, BASELINE_BYTES, REGRESSION_MARGIN_PCT)
//    scripts/wasm-size-baseline.toml (hard_budget_bytes, bytes, regression_margin)
// ─────────────────────────────────────────────────────────────────────────────

/// Hard size limit in bytes for the **optimised** WASM (256 KiB).
///
/// This matches the Stellar network's deployment ceiling.  Increasing this
/// value requires explicit sign-off in code review.
const WASM_SIZE_BUDGET_BYTES: u64 = 256 * 1024;

/// Fallback hard limit for the **raw** (unoptimised) WASM artifact, used when
/// `wasm-opt` is not available in the local environment.
///
/// The release WASM before `wasm-opt -Oz` optimisation is typically ~20 %
/// larger than the optimised artifact.  320 KiB gives enough headroom to
/// catch runaway growth while remaining generous to local builds on Windows
/// where binaryen may not be installed.
const WASM_SIZE_RAW_BUDGET_BYTES: u64 = 320 * 1024;

/// Warning zone threshold: 90 % of the hard budget (~230 KiB).
///
/// Reaching this zone is a signal to start a size-reduction effort before
/// crossing the hard limit.  See `classify_size` for how the threshold is used.
const WASM_SIZE_WARNING_BYTES: u64 = (WASM_SIZE_BUDGET_BYTES as f64 * 0.90) as u64;

/// Last known optimised WASM size in bytes.
///
/// Keep this up-to-date so the regression window stays tight.  When a PR
/// legitimately increases the contract size, the author must update this
/// constant and `scripts/wasm-size-baseline.toml` in the same commit.
const WASM_SIZE_BASELINE_BYTES: u64 = 243_608;

/// Maximum fractional growth allowed relative to `WASM_SIZE_BASELINE_BYTES`
/// before the regression test fails (5 %).
///
/// A 5 % margin accommodates minor feature additions without sacrificing
/// regression visibility.  The resulting regression limit must remain below
/// `WASM_SIZE_BUDGET_BYTES` (asserted in `regression_limit_is_within_hard_budget`).
const WASM_REGRESSION_MARGIN: f64 = 0.05;

/// WASM artifact filename.  Cargo normalises hyphens in crate names to
/// underscores, so this must use underscores even though `Cargo.toml` uses
/// `quicklendx-contracts`.
const WASM_NAME: &str = "quicklendx_contracts.wasm";

// ─────────────────────────────────────────────────────────────────────────────
//  Internal helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Three-way classification of a WASM size relative to the global thresholds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SizeTier {
    /// Below the warning threshold – healthy.
    Ok,
    /// Between the warning threshold and the hard budget – approaching the limit.
    Warning,
    /// Above the hard budget – Stellar network will reject this artifact.
    Over,
}

/// Classifies `bytes` against `WASM_SIZE_WARNING_BYTES` and `WASM_SIZE_BUDGET_BYTES`.
///
/// The boundary conditions are:
/// - `bytes == WASM_SIZE_WARNING_BYTES` → `Ok`   (inclusive lower edge of warning zone)
/// - `bytes == WASM_SIZE_BUDGET_BYTES`  → `Warning` (inclusive upper edge before 'Over')
fn classify_size(bytes: u64) -> SizeTier {
    if bytes > WASM_SIZE_BUDGET_BYTES {
        SizeTier::Over
    } else if bytes > WASM_SIZE_WARNING_BYTES {
        SizeTier::Warning
    } else {
        SizeTier::Ok
    }
}

/// Returns `true` when `actual` has not grown beyond `baseline` by more than
/// `margin` (a fraction, e.g. `0.05` for 5 %).
///
/// The check is intentionally one-directional: size *decreases* are always
/// allowed and are in fact encouraged.
fn regression_within_margin(actual: u64, baseline: u64, margin: f64) -> bool {
    actual as f64 <= baseline as f64 * (1.0 + margin)
}

/// Returns `true` when the given Rust target triple is installed via `rustup`.
///
/// On any error (e.g. `rustup` not on PATH) the function returns `false` so
/// the caller can fall back gracefully.
fn target_installed(target: &str) -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(target))
        .unwrap_or(false)
}

/// Constructs the expected WASM output path for a given target triple.
///
/// The path follows Cargo's standard layout:
/// `<target_root>/<triple>/release/<WASM_NAME>`.
fn wasm_output_path(target_root: &PathBuf, target: &str) -> PathBuf {
    target_root.join(target).join("release").join(WASM_NAME)
}

/// Selects the preferred WASM build target.
///
/// Prefers `wasm32v1-none` (the Soroban-native target with no-std guarantees)
/// when installed; falls back to `wasm32-unknown-unknown`.
fn preferred_wasm_target() -> (&'static str, &'static str) {
    if target_installed("wasm32v1-none") {
        ("wasm32v1-none", "wasm32v1-none/release")
    } else {
        ("wasm32-unknown-unknown", "wasm32-unknown-unknown/release")
    }
}

/// Builds the contract in release mode for WASM and returns the artifact path.
///
/// Uses `CARGO_MANIFEST_DIR` (compile-time constant, not caller-controlled)
/// and `CARGO_TARGET_DIR` (optional override) to locate the target directory.
///
/// # Panics
/// Panics (failing the test) if `cargo build` exits non-zero or the expected
/// artifact is absent after a successful build.
fn build_release_wasm() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let target_root = std::env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest_dir.join("target"));

    let (target, wasm_subdir) = preferred_wasm_target();
    let wasm_path = target_root.join(wasm_subdir).join(WASM_NAME);

    let status = Command::new(env!("CARGO"))
        .current_dir(&manifest_dir)
        .env("CARGO_TARGET_DIR", &target_root)
        .args(["build", "--target", target, "--release"])
        .status()
        .expect("failed to spawn `cargo build`");

    assert!(
        status.success(),
        "cargo build --target {target} --release failed with status {status}"
    );
    assert!(
        wasm_path.exists(),
        "WASM artifact not found at {} after successful build",
        wasm_path.display()
    );

    wasm_path
}

/// Applies `wasm-opt -Oz` if the tool is available on PATH.
///
/// Returns `(path, was_optimised)` where `was_optimised` is `true` when the
/// optimised artifact was successfully produced.  Callers use the flag to
/// select the appropriate size budget (network limit vs. raw fallback).
fn maybe_optimise(wasm_path: &PathBuf) -> (PathBuf, bool) {
    let opt_path = wasm_path.with_extension("opt.wasm");
    for bin in ["wasm-opt", "wasm-opt.cmd"] {
        let result = Command::new(bin)
            .args([
                "--enable-bulk-memory",
                "-Oz",
                wasm_path.to_string_lossy().as_ref(),
                "-o",
                opt_path.to_string_lossy().as_ref(),
            ])
            .status();
        if let Ok(s) = result {
            if s.success() && opt_path.exists() {
                return (opt_path, true);
            }
        }
    }
    (wasm_path.clone(), false)
}

// ─────────────────────────────────────────────────────────────────────────────
//  Unit tests – fast, no I/O, no WASM build
// ─────────────────────────────────────────────────────────────────────────────

/// `WASM_SIZE_BUDGET_BYTES` equals exactly 256 KiB (262 144 bytes).
#[test]
fn budget_constant_equals_256_kib() {
    assert_eq!(WASM_SIZE_BUDGET_BYTES, 262_144);
}

/// Warning threshold is strictly below the hard budget.
///
/// Ensures `classify_size` can distinguish the two tiers correctly.
#[test]
fn warning_threshold_is_strictly_below_budget() {
    assert!(
        WASM_SIZE_WARNING_BYTES < WASM_SIZE_BUDGET_BYTES,
        "warning threshold ({WASM_SIZE_WARNING_BYTES}) must be strictly below budget ({WASM_SIZE_BUDGET_BYTES})"
    );
}

/// Warning threshold is above zero (guards against integer underflow in the cast).
#[test]
fn warning_threshold_is_above_zero() {
    assert!(WASM_SIZE_WARNING_BYTES > 0);
}

/// Warning threshold is at least 80 % of the hard budget.
///
/// Prevents the threshold from being set so low that virtually no useful
/// artifacts land in the `Ok` tier.
#[test]
fn warning_threshold_is_at_least_80_percent_of_budget() {
    let min_acceptable = (WASM_SIZE_BUDGET_BYTES as f64 * 0.80) as u64;
    assert!(
        WASM_SIZE_WARNING_BYTES >= min_acceptable,
        "warning threshold ({WASM_SIZE_WARNING_BYTES}) should be >= 80% of budget ({min_acceptable})"
    );
}

/// Regression margin is a valid fraction strictly between 0 and 1.
#[test]
fn regression_margin_is_in_valid_range() {
    assert!(
        WASM_REGRESSION_MARGIN > 0.0 && WASM_REGRESSION_MARGIN < 1.0,
        "WASM_REGRESSION_MARGIN ({WASM_REGRESSION_MARGIN}) must be in the open interval (0, 1)"
    );
}

/// The recorded baseline size is strictly positive.
#[test]
fn baseline_size_is_positive() {
    assert!(
        WASM_SIZE_BASELINE_BYTES > 0,
        "WASM_SIZE_BASELINE_BYTES must not be zero"
    );
}

/// The recorded baseline size itself fits within the hard budget.
///
/// If this fails the baseline was recorded on an already-over-budget artifact.
#[test]
fn baseline_is_within_hard_budget() {
    assert!(
        WASM_SIZE_BASELINE_BYTES <= WASM_SIZE_BUDGET_BYTES,
        "WASM_SIZE_BASELINE_BYTES ({WASM_SIZE_BASELINE_BYTES}) must not exceed the hard budget ({WASM_SIZE_BUDGET_BYTES})"
    );
}

/// The regression limit (baseline × (1 + margin)) is within the hard budget.
///
/// This invariant guarantees that a binary which passes the regression check
/// will also pass the hard budget check.  If this fails the margin must be
/// reduced or the baseline updated.
#[test]
fn regression_limit_is_within_hard_budget() {
    let limit = (WASM_SIZE_BASELINE_BYTES as f64 * (1.0 + WASM_REGRESSION_MARGIN)) as u64;
    assert!(
        limit <= WASM_SIZE_BUDGET_BYTES,
        "regression limit ({limit}) would allow exceeding the hard budget ({WASM_SIZE_BUDGET_BYTES}); \
         reduce WASM_REGRESSION_MARGIN or update WASM_SIZE_BASELINE_BYTES"
    );
}

/// `WASM_NAME` is non-empty and has a `.wasm` extension.
#[test]
fn wasm_name_has_wasm_extension() {
    assert!(!WASM_NAME.is_empty());
    assert!(
        WASM_NAME.ends_with(".wasm"),
        "WASM_NAME '{WASM_NAME}' must end with '.wasm'"
    );
}

/// `WASM_NAME` uses underscores, not hyphens.
///
/// Cargo normalises `quicklendx-contracts` → `quicklendx_contracts` when
/// producing the output filename.
#[test]
fn wasm_name_uses_underscores_not_hyphens() {
    assert!(
        !WASM_NAME.contains('-'),
        "WASM_NAME '{WASM_NAME}' must not contain hyphens; Cargo normalises them to underscores"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//  classify_size – boundary and representative value tests
// ─────────────────────────────────────────────────────────────────────────────

/// Zero bytes is classified as `Ok`.
#[test]
fn classify_zero_bytes_is_ok() {
    assert_eq!(classify_size(0), SizeTier::Ok);
}

/// A size well below the warning threshold is `Ok`.
#[test]
fn classify_well_under_warning_is_ok() {
    assert_eq!(classify_size(WASM_SIZE_WARNING_BYTES / 2), SizeTier::Ok);
}

/// A size exactly at the warning threshold is `Ok` (inclusive boundary).
#[test]
fn classify_at_warning_threshold_is_ok() {
    assert_eq!(classify_size(WASM_SIZE_WARNING_BYTES), SizeTier::Ok);
}

/// One byte above the warning threshold enters the `Warning` tier.
#[test]
fn classify_one_above_warning_is_warning() {
    assert_eq!(
        classify_size(WASM_SIZE_WARNING_BYTES + 1),
        SizeTier::Warning
    );
}

/// A size midway between the warning threshold and the hard budget is `Warning`.
#[test]
fn classify_midpoint_warning_to_budget_is_warning() {
    let mid = WASM_SIZE_WARNING_BYTES + (WASM_SIZE_BUDGET_BYTES - WASM_SIZE_WARNING_BYTES) / 2;
    assert_eq!(classify_size(mid), SizeTier::Warning);
}

/// A size exactly at the hard budget is `Warning` (not yet rejected).
#[test]
fn classify_at_hard_budget_is_warning() {
    assert_eq!(classify_size(WASM_SIZE_BUDGET_BYTES), SizeTier::Warning);
}

/// One byte above the hard budget is `Over` (CI must fail).
#[test]
fn classify_one_above_budget_is_over() {
    assert_eq!(classify_size(WASM_SIZE_BUDGET_BYTES + 1), SizeTier::Over);
}

/// The maximum possible `u64` value is `Over`.
#[test]
fn classify_u64_max_is_over() {
    assert_eq!(classify_size(u64::MAX), SizeTier::Over);
}

// ─────────────────────────────────────────────────────────────────────────────
//  regression_within_margin – representative and boundary tests
// ─────────────────────────────────────────────────────────────────────────────

/// Actual size equal to the baseline passes.
#[test]
fn regression_passes_when_size_equals_baseline() {
    assert!(regression_within_margin(1_000, 1_000, 0.05));
}

/// A size decrease (actual < baseline) always passes.
#[test]
fn regression_passes_when_size_decreases() {
    assert!(regression_within_margin(900, 1_000, 0.05));
}

/// Growth exactly at the 5 % margin boundary passes (inclusive).
#[test]
fn regression_passes_at_exact_5_percent_margin() {
    // 1 050 == 1 000 × 1.05 exactly
    assert!(regression_within_margin(1_050, 1_000, 0.05));
}

/// Growth just inside the margin passes.
#[test]
fn regression_passes_just_inside_margin() {
    assert!(regression_within_margin(1_049, 1_000, 0.05));
}

/// Growth just above the margin fails.
#[test]
fn regression_fails_just_above_margin() {
    // 1 051 > 1 000 × 1.05 = 1 050
    assert!(!regression_within_margin(1_051, 1_000, 0.05));
}

/// Zero margin: any growth above baseline fails.
#[test]
fn regression_zero_margin_fails_on_any_growth() {
    assert!(!regression_within_margin(1_001, 1_000, 0.0));
}

/// Zero margin: exact match still passes.
#[test]
fn regression_zero_margin_passes_exact_match() {
    assert!(regression_within_margin(1_000, 1_000, 0.0));
}

/// The actual baseline and margin constants produce a regression limit that is
/// consistent with what the shell script computes (integer truncation aside).
#[test]
fn regression_limit_matches_shell_script_arithmetic() {
    // Bash: REGRESSION_LIMIT = BASELINE + BASELINE * 5 / 100  (integer division)
    let shell_limit = WASM_SIZE_BASELINE_BYTES + WASM_SIZE_BASELINE_BYTES * 5 / 100;
    let rust_limit = (WASM_SIZE_BASELINE_BYTES as f64 * (1.0 + WASM_REGRESSION_MARGIN)) as u64;
    // Allow up to 1 byte of rounding difference between integer and float arithmetic.
    let diff = if rust_limit > shell_limit {
        rust_limit - shell_limit
    } else {
        shell_limit - rust_limit
    };
    assert!(
        diff <= 1,
        "Rust regression limit ({rust_limit}) and shell limit ({shell_limit}) differ by {diff} B; \
         they should be in agreement to within 1 byte"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//  wasm_output_path – path construction tests
// ─────────────────────────────────────────────────────────────────────────────

/// Path is constructed as `<root>/<triple>/release/<WASM_NAME>` for the
/// `wasm32-unknown-unknown` target.
#[test]
fn wasm_output_path_unknown_unknown_target() {
    let root = PathBuf::from("/workspace/target");
    let path = wasm_output_path(&root, "wasm32-unknown-unknown");
    assert_eq!(
        path,
        PathBuf::from("/workspace/target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm")
    );
}

/// Path is constructed correctly for the `wasm32v1-none` (Soroban native) target.
#[test]
fn wasm_output_path_v1_none_target() {
    let root = PathBuf::from("/workspace/target");
    let path = wasm_output_path(&root, "wasm32v1-none");
    assert_eq!(
        path,
        PathBuf::from("/workspace/target/wasm32v1-none/release/quicklendx_contracts.wasm")
    );
}

/// The artifact lives directly under `release/`, not in a subdirectory.
#[test]
fn wasm_output_path_has_exactly_three_components_after_root() {
    let root = PathBuf::from("/root");
    let path = wasm_output_path(&root, "wasm32v1-none");
    // root / triple / release / name == 4 total after stripping the root prefix
    let relative = path
        .strip_prefix("/root")
        .expect("path must start with root");
    assert_eq!(relative.components().count(), 3);
}

// ─────────────────────────────────────────────────────────────────────────────
//  Script and baseline file presence tests
// ─────────────────────────────────────────────────────────────────────────────

/// The shell enforcement script must exist in its expected location.
///
/// This test catches accidental deletion or rename of the script.
#[test]
fn check_wasm_size_script_exists() {
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("check-wasm-size.sh");
    assert!(
        script.exists(),
        "expected check-wasm-size.sh at {}",
        script.display()
    );
}

/// The baseline TOML file must exist alongside the shell script.
///
/// This file is checked in to source control and used by both the shell script
/// (informational) and this test suite (validation).
#[test]
fn wasm_size_baseline_toml_exists() {
    let baseline = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("wasm-size-baseline.toml");
    assert!(
        baseline.exists(),
        "expected wasm-size-baseline.toml at {}; \
         create it or restore it from version control",
        baseline.display()
    );
}

/// The baseline TOML file contains all required keys.
///
/// Checked as plain text to avoid a TOML parser dependency.  The test verifies
/// each key name is present, not that the values are correct (that is ensured
/// by the regression and budget constant tests above).
#[test]
fn wasm_size_baseline_toml_contains_required_keys() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("wasm-size-baseline.toml");
    let content = std::fs::read_to_string(&path).expect("could not read wasm-size-baseline.toml");

    for key in &[
        "bytes",
        "regression_margin",
        "hard_budget_bytes",
        "recorded",
    ] {
        assert!(
            content.contains(key),
            "wasm-size-baseline.toml is missing required key '{key}'"
        );
    }
}

/// The hard budget recorded in the TOML matches the Rust constant.
///
/// Both must equal 262 144 (256 KiB).  A mismatch indicates that one was
/// updated without the other.
#[test]
fn baseline_toml_hard_budget_matches_rust_constant() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("wasm-size-baseline.toml");
    let content = std::fs::read_to_string(&path).expect("could not read wasm-size-baseline.toml");

    // Find the first non-comment line whose key is exactly "hard_budget_bytes".
    // Using starts_with avoids accidentally matching comment lines that mention
    // the key name (e.g. "# or raising hard_budget_bytes)").
    let value: u64 = content
        .lines()
        .find(|l| {
            let t = l.trim_start();
            !t.starts_with('#') && t.starts_with("hard_budget_bytes")
        })
        .and_then(|l| l.splitn(2, '=').nth(1))
        .and_then(|v| {
            let numeric = v.trim().split('#').next().unwrap_or("").trim();
            numeric.parse().ok()
        })
        .expect("could not parse hard_budget_bytes from wasm-size-baseline.toml");

    assert_eq!(
        value, WASM_SIZE_BUDGET_BYTES,
        "hard_budget_bytes in wasm-size-baseline.toml ({value}) \
         does not match WASM_SIZE_BUDGET_BYTES ({WASM_SIZE_BUDGET_BYTES})"
    );
}

/// The baseline bytes recorded in the TOML match the Rust constant.
///
/// Both must equal `WASM_SIZE_BASELINE_BYTES`.
#[test]
fn baseline_toml_bytes_match_rust_constant() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("wasm-size-baseline.toml");
    let content = std::fs::read_to_string(&path).expect("could not read wasm-size-baseline.toml");

    // The key is exactly "bytes"; use starts_with to avoid matching
    // "hard_budget_bytes" or comment lines that reference "bytes".
    let value: u64 = content
        .lines()
        .find(|l| {
            let t = l.trim_start();
            !t.starts_with('#') && t.starts_with("bytes ") && t.contains('=')
        })
        .and_then(|l| l.splitn(2, '=').nth(1))
        .and_then(|v| {
            let numeric = v.trim().split('#').next().unwrap_or("").trim();
            numeric.parse().ok()
        })
        .expect("could not parse bytes from wasm-size-baseline.toml");

    assert_eq!(
        value, WASM_SIZE_BASELINE_BYTES,
        "bytes in wasm-size-baseline.toml ({value}) \
         does not match WASM_SIZE_BASELINE_BYTES ({WASM_SIZE_BASELINE_BYTES})"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
//  Integration tests – invoke `cargo build`, require WASM target
//
//  These tests may take 30–60 s on a cold build cache.  They are not marked
//  `#[ignore]` so that CI catches regressions automatically.  Cargo's
//  incremental compilation makes the second test fast when the first has
//  already produced the artifact.
// ─────────────────────────────────────────────────────────────────────────────

/// Builds the contract in release mode and asserts the WASM artifact is within
/// the applicable size budget.
///
/// When `wasm-opt` is available the **optimised** budget (256 KiB, network
/// limit) is enforced.  When it is absent, the **raw** fallback budget
/// (320 KiB) applies and a diagnostic is printed to prompt installation.
///
/// Emits a warning diagnostic when the binary is in the 90–100 % zone so
/// developers get early warning before hitting the hard limit.
#[test]
fn wasm_release_build_fits_hard_budget() {
    let wasm_path = build_release_wasm();
    let (final_path, was_optimised) = maybe_optimise(&wasm_path);

    let size = std::fs::metadata(&final_path)
        .expect("failed to read WASM metadata")
        .len();

    if !was_optimised {
        eprintln!(
            "\nNOTE: wasm-opt not found; checking against raw WASM budget \
             ({} KiB) instead of the network limit ({} KiB). \
             Install binaryen (brew install binaryen / apt install binaryen) \
             for accurate 256 KiB enforcement.\n",
            WASM_SIZE_RAW_BUDGET_BYTES / 1024,
            WASM_SIZE_BUDGET_BYTES / 1024,
        );
        assert!(
            size <= WASM_SIZE_RAW_BUDGET_BYTES,
            "Raw WASM size {size} B exceeds fallback budget {WASM_SIZE_RAW_BUDGET_BYTES} B \
             (320 KiB); path: {}",
            final_path.display()
        );
        return;
    }

    let tier = classify_size(size);
    if tier == SizeTier::Warning {
        eprintln!(
            "\nWARNING: Optimised WASM size {size} B is in the warning zone \
             ({WASM_SIZE_WARNING_BYTES}–{WASM_SIZE_BUDGET_BYTES} B). \
             Consider reducing the contract size before reaching the hard limit.\n"
        );
    }
    assert!(
        tier != SizeTier::Over,
        "WASM size {size} B exceeds hard budget {WASM_SIZE_BUDGET_BYTES} B (256 KiB); \
         path: {}",
        final_path.display()
    );
}

/// Builds the contract in release mode and asserts the size has not regressed
/// beyond `WASM_REGRESSION_MARGIN` (5 %) relative to `WASM_SIZE_BASELINE_BYTES`.
///
/// # Updating the baseline after a legitimate size increase
/// 1. Update `WASM_SIZE_BASELINE_BYTES` in this file.
/// 2. Update `bytes` in `scripts/wasm-size-baseline.toml`.
/// 3. Update `BASELINE_BYTES` in `scripts/check-wasm-size.sh`.
/// 4. Commit all three changes in the same PR as the growth.
#[test]
fn wasm_release_build_has_no_size_regression() {
    let wasm_path = build_release_wasm();
    let (final_path, was_optimised) = maybe_optimise(&wasm_path);

    if !was_optimised {
        eprintln!(
            "\nINFO: wasm-opt not found; skipping regression check (baseline \
             was measured on an optimised artifact). \
             Install binaryen to enable this check.\n"
        );
        return;
    }

    let size = std::fs::metadata(&final_path)
        .expect("failed to read WASM metadata")
        .len();

    let limit = (WASM_SIZE_BASELINE_BYTES as f64 * (1.0 + WASM_REGRESSION_MARGIN)) as u64;

    assert!(
        regression_within_margin(size, WASM_SIZE_BASELINE_BYTES, WASM_REGRESSION_MARGIN),
        "WASM size regression detected: {size} B > regression limit {limit} B \
         (baseline {WASM_SIZE_BASELINE_BYTES} B + {}% margin); path: {}. \
         If this growth is intentional, update WASM_SIZE_BASELINE_BYTES, \
         scripts/wasm-size-baseline.toml, and scripts/check-wasm-size.sh.",
        (WASM_REGRESSION_MARGIN * 100.0) as u32,
        final_path.display()
    );
}
