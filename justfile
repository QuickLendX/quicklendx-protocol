# QuickLendX protocol — justfile
# Run `just` or `just help` for available recipes.

# List available recipes
help:
    @just --list

# ── Contracts ────────────────────────────────────────────────────────────

# Build all Soroban contracts
build:
    cargo build --target wasm32-unknown-unknown --release

# Run contract tests
test:
    cargo test -p quicklendx-contracts

# Run fuzz tests (quick | standard | extended | thorough)
fuzz level="standard":
    ./run_fuzz_tests.sh {{level}}

# Check WASM binary size (256 KB budget)
check-wasm:
    ./scripts/check-wasm-size.sh

# Clippy lint (deny warnings)
clippy:
    cargo clippy --workspace --all-targets -- -D warnings

# Format all code
fmt:
    cargo fmt --all

# Check formatting without writing
fmt-check:
    cargo fmt --all -- --check

# ── Frontend ─────────────────────────────────────────────────────────────

# Install frontend dependencies
frontend-install:
    cd quicklendx-frontend && npm ci

# Start local dev server
frontend-dev:
    cd quicklendx-frontend && npm run dev

# Production build (includes type checks)
frontend-build:
    cd quicklendx-frontend && npm run build

# Lint frontend
frontend-lint:
    cd quicklendx-frontend && npm run lint

# Type-check frontend
frontend-typecheck:
    cd quicklendx-frontend && npx tsc --noEmit

# ── Combined ─────────────────────────────────────────────────────────────

# Run all checks (format, clippy, tests, WASM size, frontend lint + typecheck)
check: fmt-check clippy test check-wasm frontend-lint frontend-typecheck
