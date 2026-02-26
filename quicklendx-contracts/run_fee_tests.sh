#!/bin/bash
# Script to run fee structure tests

echo "Running update_fee_structure tests..."
cargo test test_update_fee_structure --lib -- --nocapture

echo ""
echo "Running validate_fee_parameters tests..."
cargo test test_validate_fee_parameters --lib -- --nocapture

echo ""
echo "Test execution complete!"
