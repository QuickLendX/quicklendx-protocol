#!/bin/bash

echo "Running invoice count tests..."
echo "=============================="
echo ""

# Run the invoice count tests
cargo test --lib test_get_invoice_count_by_status_all_statuses \
           test_get_total_invoice_count_equals_sum_of_status_counts \
           test_invoice_counts_after_status_transitions \
           test_invoice_counts_after_cancellation \
           test_invoice_counts_with_multiple_status_updates \
           test_invoice_count_consistency \
           -- --nocapture

echo ""
echo "=============================="
echo "Test run complete!"
