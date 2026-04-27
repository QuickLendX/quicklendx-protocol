#!/bin/bash
source ~/.cargo/env
cd /mnt/c/Users/fuhad/quicklendx-protocol/quicklendx-contracts
soroban_lib=$(ls target/debug/deps/libsoroban_sdk-*.rlib 2>/dev/null | tail -1)

# Search for non-ASCII chars which cause bad spans
echo "=== Non-ASCII characters in source files ==="
grep -rPn '[^\x00-\x7F]' src/ --include="*.rs" | grep -v 'target/' | head -20

# Also check for any remaining issues in events.rs specifically
echo ""
echo "=== Checking events.rs imports ==="
head -15 src/events.rs

echo ""
echo "=== Checking profits.rs ==="
head -15 src/profits.rs
