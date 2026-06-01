# Contract Types Fuzz Testing

This document describes the cargo-fuzz setup for testing XDR round-trip stability of all `#[contracttype]` structs in the QuickLendX protocol.

## Overview

The fuzzer validates that contract types can safely handle arbitrary byte inputs without panicking and maintain data integrity through XDR encoding/decoding cycles.

## Security Motivation

Contract types cross the host boundary via XDR encoding, making them a potential attack surface for:
- Malformed XDR payloads causing panics
- Data corruption during encoding/decoding cycles
- Memory safety issues with hostile inputs

## Tested Types

The fuzzer covers all 16 contract types from `src/types.rs`:

### Enums
- `InvoiceStatus` - Invoice lifecycle states
- `BidStatus` - Bid lifecycle states  
- `InvestmentStatus` - Investment lifecycle states
- `DisputeStatus` - Dispute resolution states
- `InvoiceCategory` - Invoice classification
- `SearchRank` - Search result relevance

### Structs
- `LineItemRecord` - Invoice line item data
- `PaymentRecord` - Payment history entry
- `Dispute` - Dispute resolution data
- `InvoiceRating` - Invoice rating/feedback
- `Invoice` - Core invoice structure
- `InvoiceMetadata` - Invoice metadata helper
- `Bid` - Investment bid data
- `Investment` - Investment tracking
- `InsuranceCoverage` - Insurance policy data
- `PlatformFeeConfig` - Fee configuration
- `SearchResult` - Search result entry

## Test Strategy

For each type, the fuzzer:
1. Attempts to decode random bytes as XDR
2. For successful decodes, re-encodes and verifies bit-identical output
3. Ensures no panics occur on invalid input

## Running the Fuzzer

```bash
# From quicklendx-contracts/ directory
cargo +nightly fuzz run contracttype_roundtrip -- -runs=100000

# Extended testing
cargo +nightly fuzz run contracttype_roundtrip -- -runs=1000000
```

## Expected Behavior

- **Valid XDR**: Should decode, re-encode to identical bytes
- **Invalid XDR**: Should fail gracefully without panicking
- **Edge cases**: Empty input, oversized payloads, malformed structures

## Security Properties

The fuzzer validates:
- No panics on hostile input (DoS resistance)
- XDR round-trip integrity (data consistency)
- Memory safety with arbitrary payloads

## Integration

This fuzzer complements existing test coverage:
- Unit tests validate business logic
- Property tests validate invariants
- Fuzz tests validate input handling safety