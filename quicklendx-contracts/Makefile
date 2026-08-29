# ── Targets ───────────────────────────────────────────────────────────────────
default: build

# Full pipeline: build WASM + regenerate TypeScript client bindings + run tests.
all: test

# Run tests (compiles WASM first, skips bindings for fast TDD iteration).
test: wasm
	cargo test

# Build the contract WASM and auto-generate TypeScript client bindings.
# The bindings step is idempotent — safe to rerun without side effects.
build: bindings

# Compile the WASM artifact without regenerating bindings.
wasm:
	stellar contract build
	@ls -l target/wasm32v1-none/release/*.wasm 2>/dev/null || ls -l target/wasm32-unknown-unknown/release/*.wasm

# Generate TypeScript client bindings from the compiled WASM.
# Uses stellar contract bindings typescript to produce type-safe client code
# consumed by the frontend (quicklendx-frontend).
# The command probes both possible WASM paths (wasm32v1-none and wasm32-unknown-unknown)
# and falls back gracefully if neither is found.
bindings:
	stellar contract build
	@ls -l target/wasm32v1-none/release/*.wasm 2>/dev/null || ls -l target/wasm32-unknown-unknown/release/*.wasm 2>/dev/null || true
	@WASM_PATH=""; \
	if [ -f target/wasm32v1-none/release/quicklendx_contracts.wasm ]; then \
		WASM_PATH="target/wasm32v1-none/release/quicklendx_contracts.wasm"; \
	elif [ -f target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm ]; then \
		WASM_PATH="target/wasm32-unknown-unknown/release/quicklendx_contracts.wasm"; \
	fi; \
	if [ -n "$$WASM_PATH" ]; then \
		mkdir -p bindings; \
		echo "==> Generating TypeScript bindings from $$WASM_PATH..."; \
		stellar contract bindings typescript \
			--wasm "$$WASM_PATH" \
			--output-dir bindings \
			--overwrite 2>&1 || echo "==> Bindings generation skipped (not supported by this CLI version)."; \
	else \
		echo "==> No WASM artifact found; skipping bindings generation."; \
	fi

fmt:
	cargo fmt --all

clean:
	rm -rf bindings
	cargo clean
