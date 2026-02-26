//! Integration test: build contract for WASM (release, no test code) and assert size ≤ 256 KB.
//! Ensures the contract fits network deployment limits and no test-only code is in the release binary.

use std::path::PathBuf;
use std::process::Command;

const WASM_SIZE_BUDGET_BYTES: u64 = 256 * 1024; // 256 KB
const WASM_NAME: &str = "quicklendx_contracts.wasm";

#[test]
fn wasm_release_build_fits_size_budget() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let wasm_path = manifest_dir
        .join("target/wasm32-unknown-unknown/release")
        .join(WASM_NAME);

    // Build release lib only (no test code included)
    let status = Command::new(env!("CARGO"))
        .current_dir(&manifest_dir)
        .args([
            "build",
            "--target",
            "wasm32-unknown-unknown",
            "--release",
            "--lib",
        ])
        .status()
        .expect("failed to run cargo build");

    assert!(
        status.success(),
        "cargo build --target wasm32-unknown-unknown --release --lib failed"
    );

    assert!(
        wasm_path.exists(),
        "WASM artifact not found at {}",
        wasm_path.display()
    );

    // Match CI behavior: if wasm-opt is available, optimize before size check.
    let optimized_path = wasm_path.with_extension("opt.wasm");
    let mut size_target_path = wasm_path.clone();
    let mut optimized = false;
    for bin in ["wasm-opt", "wasm-opt.cmd"] {
        let status = Command::new(bin)
            .current_dir(&manifest_dir)
            .args([
                "--enable-bulk-memory",
                "-Oz",
                wasm_path.to_string_lossy().as_ref(),
                "-o",
                optimized_path.to_string_lossy().as_ref(),
            ])
            .status();

        if let Ok(exit) = status {
            if exit.success() && optimized_path.exists() {
                size_target_path = optimized_path.clone();
                optimized = true;
                break;
            }
        }
    }

    let _ = optimized;

    let size = std::fs::metadata(&size_target_path)
        .expect("failed to read WASM metadata")
        .len();

    assert!(
        size <= WASM_SIZE_BUDGET_BYTES,
        "WASM size {} bytes exceeds budget {} bytes (256 KB); path: {}",
        size,
        WASM_SIZE_BUDGET_BYTES,
        size_target_path.display()
    );
}
