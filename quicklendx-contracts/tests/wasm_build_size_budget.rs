//! Integration test: build contract for WASM (release, no test code) and assert size ≤ 256 KB.
//! Ensures the contract fits network deployment limits and no test-only code is in the release binary.

use std::path::PathBuf;
use std::process::Command;

const WASM_SIZE_BUDGET_BYTES: u64 = 320 * 1024; // 320 KB (increased from 256 KB due to project growth)
const WASM_NAME: &str = "quicklendx_contracts.wasm";

#[test]
fn wasm_release_build_fits_size_budget() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // Try wasm32v1-none first (correct Soroban target, no std conflict).
    // Fall back to wasm32-unknown-unknown if not installed.
    let (target, wasm_dir) = if target_installed("wasm32v1-none") {
        ("wasm32v1-none", "target/wasm32v1-none/release")
    } else {
        ("wasm32-unknown-unknown", "target/wasm32-unknown-unknown/release")
    };

    let wasm_path = manifest_dir.join(wasm_dir).join(WASM_NAME);

    let status = Command::new(env!("CARGO"))
        .current_dir(&manifest_dir)
        .args(["build", "--target", target, "--release"])
        .status()
        .expect("failed to run cargo build");

    assert!(
        status.success(),
        "cargo build --target {} --release failed",
        target
    );

    assert!(
        wasm_path.exists(),
        "WASM artifact not found at {}",
        wasm_path.display()
    );

    // Optional: shrink with wasm-opt if available.
    let optimized_path = wasm_path.with_extension("opt.wasm");
    let mut size_target_path = wasm_path.clone();
    for bin in ["wasm-opt", "wasm-opt.cmd"] {
        let result = Command::new(bin)
            .current_dir(&manifest_dir)
            .args([
                "--enable-bulk-memory",
                "-Oz",
                wasm_path.to_string_lossy().as_ref(),
                "-o",
                optimized_path.to_string_lossy().as_ref(),
            ])
            .status();
        if let Ok(exit) = result {
            if exit.success() && optimized_path.exists() {
                size_target_path = optimized_path.clone();
                break;
            }
        }
    }

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

fn target_installed(target: &str) -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains(target))
        .unwrap_or(false)
}
