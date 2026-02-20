#!/bin/bash
# QuickLendX Fuzz Testing Quick Reference
# Run this script to execute fuzz tests with various configurations

set -e

echo "🔬 QuickLendX Fuzz Testing Suite"
echo "================================"
echo ""

# Change to contracts directory
cd "$(dirname "$0")/quicklendx-contracts"

# Function to run tests with timing
run_test() {
    local description=$1
    local command=$2
    
    echo "📋 $description"
    echo "   Command: $command"
    echo ""
    
    start_time=$(date +%s)
    eval "$command"
    end_time=$(date +%s)
    duration=$((end_time - start_time))
    
    echo ""
    echo "   ✅ Completed in ${duration}s"
    echo ""
}

# Parse command line arguments
case "${1:-quick}" in
    quick)
        echo "Running quick fuzz tests (50 cases per test)..."
        echo ""
        run_test "Quick Fuzz Test" "cargo test --features fuzz-tests fuzz_ --lib"
        ;;
    
    standard)
        echo "Running standard fuzz tests (1,000 cases per test)..."
        echo ""
        run_test "Standard Fuzz Test" "PROPTEST_CASES=1000 cargo test --features fuzz-tests fuzz_ --lib"
        ;;
    
    extended)
        echo "Running extended fuzz tests (10,000 cases per test)..."
        echo "⚠️  This may take 30+ minutes"
        echo ""
        run_test "Extended Fuzz Test" "PROPTEST_CASES=10000 cargo test --features fuzz-tests fuzz_ --lib"
        ;;
    
    thorough)
        echo "Running thorough fuzz tests (100,000 cases per test)..."
        echo "⚠️  This may take several hours"
        echo ""
        run_test "Thorough Fuzz Test" "PROPTEST_CASES=100000 cargo test --features fuzz-tests fuzz_ --lib"
        ;;
    
    invoice)
        echo "Running invoice creation fuzz tests..."
        echo ""
        run_test "Invoice Fuzz Tests" "cargo test --features fuzz-tests fuzz_store_invoice --lib"
        ;;
    
    bid)
        echo "Running bid placement fuzz tests..."
        echo ""
        run_test "Bid Fuzz Tests" "cargo test --features fuzz-tests fuzz_place_bid --lib"
        ;;
    
    settlement)
        echo "Running settlement fuzz tests..."
        echo ""
        run_test "Settlement Fuzz Tests" "cargo test --features fuzz-tests fuzz_settle_invoice --lib"
        ;;
    
    all)
        echo "Running ALL tests (including non-fuzz)..."
        echo ""
        run_test "All Tests" "cargo test --features fuzz-tests --lib"
        ;;
    
    help|--help|-h)
        echo "Usage: $0 [mode]"
        echo ""
        echo "Modes:"
        echo "  quick      - Run with 50 cases per test (~30s, default)"
        echo "  standard   - Run with 1,000 cases per test (~5min)"
        echo "  extended   - Run with 10,000 cases per test (~30min)"
        echo "  thorough   - Run with 100,000 cases per test (hours)"
        echo "  invoice    - Run only invoice creation tests"
        echo "  bid        - Run only bid placement tests"
        echo "  settlement - Run only settlement tests"
        echo "  all        - Run all tests including non-fuzz"
        echo "  help       - Show this help message"
        echo ""
        echo "Examples:"
        echo "  $0              # Quick test (default)"
        echo "  $0 standard     # Standard test"
        echo "  $0 invoice      # Only invoice tests"
        echo ""
        echo "Environment Variables:"
        echo "  PROPTEST_CASES=N    - Set number of test cases"
        echo "  PROPTEST_SEED=N     - Reproduce specific test case"
        echo ""
        echo "Note: Fuzz tests require the 'fuzz-tests' feature flag"
        echo ""
        echo "Documentation:"
        echo "  See FUZZ_TESTING.md for detailed guide"
        echo "  See SECURITY_ANALYSIS.md for security details"
        exit 0
        ;;
    
    *)
        echo "❌ Unknown mode: $1"
        echo "Run '$0 help' for usage information"
        exit 1
        ;;
esac

echo ""
echo "✅ Fuzz testing complete!"
echo ""
echo "📚 For more information:"
echo "   - FUZZ_TESTING.md - Comprehensive testing guide"
echo "   - SECURITY_ANALYSIS.md - Security assessment"
echo "   - CONTRIBUTING.md - Contribution guidelines"
