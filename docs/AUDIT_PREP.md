# Security Audit Preparation Checklist

This guide is for **contributors** preparing the QuickLendX Soroban smart contracts for an external security audit. It outlines exactly what to provide to the auditor, what the audit process looks like, and what to verify before handing over the codebase.

## 1. What to Fix Pre-Audit (The Contributor Checklist)

Before creating the audit commit hash, ensure all the following checks pass locally. Do not hand over code with failing tests or unaddressed lints.

### Pass all tests and lints
```bash
cd quicklendx-contracts
# 1. Build for the target architecture
cargo build --target wasm32-unknown-unknown --release

# 2. Run all tests (including fuzz tests if applicable)
cargo test --workspace

# 3. Check for any clippy warnings
cargo clippy --workspace --all-targets -- -D warnings
```

### Clean up test code in production paths
Ensure no `std::` dependencies have leaked into the contract code. The contract must maintain `#![no_std]` discipline. Use `soroban_sdk` primitives exclusively.

### Resolve all TODOs in critical paths
Search the codebase for `TODO` or `FIXME` and either resolve them or move them to the issue tracker. The auditor will flag unresolved inline TODOs as potential risks.

## 2. What to Hand the Auditor

When engaging the audit firm, provide a single zip file or a direct link to a specific commit hash containing the following:

### Scope Definition
Clearly define what is in and out of scope. For example:
- **In Scope:** `quicklendx-contracts/src/**/*.rs`
- **Out of Scope:** `quicklendx-frontend/`, `quicklendx-backend/`

### The Commit Hash
Never give a branch name (e.g., `main`). Always provide the exact commit hash:
`Commit: a1b2c3d4e5f6g7h8i9j0k1l2m3n4o5p6q7r8s9t0`

### Generated Documentation
Point the auditor to the rustdoc output. Provide them with the command to generate it locally:
```bash
cargo doc --no-deps --open
```

### Architecture and Threat Model Context
Link the auditors directly to our core design documents:
- [Invoice Lifecycle Diagram](./INVOICE_LIFECYCLE_DIAGRAM.md)
- [Default Flow Diagram](./DEFAULT_FLOW_DIAGRAM.md)
- [Off-Chain Signatures & Threat Model](./OFF_CHAIN_SIGNATURES.md)

## 3. What to Expect During the Audit

### Initial Review & Questions
Auditors will typically spend the first week reviewing the architecture and asking clarifying questions. Expect them to ask for concrete examples of state transitions. 

**Example Auditor Question:** 
> "How does an invoice transition from `Funded` to `Repaid` if the borrower only sends a partial payment?"

**Example Contributor Response:**
> "Partial payments do not automatically transition the invoice to `Repaid`. The entrypoint `record_payment(env, invoice_id, amount)` updates the `remaining_balance`. Only when `remaining_balance == 0` does the state machine allow the transition to `Repaid`."

### Preliminary Report
You will receive a draft report detailing vulnerabilities categorized by severity (Critical, High, Medium, Low, Informational). 

### Remediation Phase
You will have a window (usually 1-2 weeks) to fix the identified issues. For each finding, you will either:
1. **Fix the issue:** Submit a PR addressing the vulnerability.
2. **Acknowledge/Accept the risk:** Provide a documented justification for why the behavior is intended.

### Final Report
After reviewing your fixes, the auditor will publish the final report, verifying that the critical and high vulnerabilities have been resolved.
