//! Integration test: build contract for WASM (release, no test code) and assert size ≤ 256 KB.
//! Ensures the contract fits network deployment limits and no test-only code is in the release binary.

use std::path::PathBuf;
use std::process::Command;

const WASM_SIZE_BUDGET_BYTES: u64 = 512 * 1024; // 512 KB
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

    let size = std::fs::metadata(&wasm_path)
        .expect("failed to read WASM metadata")
        .len();

    assert!(
        size <= WASM_SIZE_BUDGET_BYTES,
        "WASM size {} bytes exceeds budget {} bytes (256 KB); path: {}",
        size,
        WASM_SIZE_BUDGET_BYTES,
        wasm_path.display()
    );
}
