# Security Policy

QuickLendX takes the security of our smart contracts, user funds, and protocol infrastructure seriously. This document outlines our vulnerability disclosure policy, reporting channels, and response procedures.

## Supported Versions

Security fixes and advisories are prioritized for active smart contract releases on the Stellar network.

| Component | Supported / In Scope | Notes |
| :--- | :---: | :--- |
| `quicklendx-contracts/` (Soroban Smart Contracts) | :white_check_mark: | Primary focus of protocol security and fund safety |
| Core protocol contracts on Stellar mainnet/testnet | :white_check_mark: | Active deployment targets |
| Backend & Frontend client packages | Limited | Non-critical client-side issues follow standard PR triage |

## Scope

### In-Scope Vulnerabilities

We are primarily concerned with issues impacting the safety of user funds, contract state integrity, and protocol invariants, including:

- Direct loss or theft of user funds or escrowed collateral
- Unauthorized fund drains, reentrancy attacks, or arithmetic overflows/underflows
- Bypassing access controls or admin authorization checks
- Freezing or locking user funds permanently (griefing / denial of service)
- Logic flaws breaking core state transitions (invoice lifecycle, bidding, settlement, default flow)
- Soroban SDK / WASM execution vulnerabilities and gas exhaustion exploits

### Out-of-Scope Items

The following items are outside the scope of our security vulnerability policy:

- Non-security bugs, UX issues, typo fixes, or documentation errors
- Denial-of-service attacks against third-party RPC endpoints or public infrastructure
- Attacks requiring physical access to a user's device or compromised user private keys
- Vulnerabilities in mock contracts, local test suites, or test harnesses (`tests/`, `fixtures/`)
- Public zero-day vulnerabilities in underlying external platforms (e.g. Stellar Core) before vendor patches are released
- Phishing, social engineering, or scam domains targeting protocol users

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues or public discussions.**

Maintainers will triage incoming reports submitted through GitHub's private vulnerability reporting feature:

1. Navigate to the [QuickLendX Security Advisories page](https://github.com/QuickLendX/quicklendx-protocol/security/advisories) on GitHub.
2. Click **Report a vulnerability** to open a private advisory draft.
3. Provide a detailed report including:
   - A descriptive title and summary of the vulnerability
   - Affected components, functions, or contract files (e.g. `quicklendx-contracts/src/...`)
   - Step-by-step instructions or proof-of-concept (PoC) code to reproduce the issue
   - An assessment of the exploitability, severity, and potential impact
   - Any proposed mitigations or fix PR suggestions

If GitHub private reporting is temporarily unavailable, maintainers will review reports sent via maintainer-approved private reporting channels listed in the repository settings.

## Response Expectations

Our security team and maintainers are committed to timely review:

- **Initial Acknowledgment:** Within 48 hours of report submission.
- **Triage & Assessment:** Initial validation, severity classification, and reproducibility assessment within 3 business days.
- **Remediation & Patching:** High and critical severity issues will be fast-tracked for hotfix development in an isolated private fork.
- **Disclosure:** We observe coordinated vulnerability disclosure. We request reporters refrain from disclosing vulnerabilities publicly until an official patch has been deployed or a fix timeline has been mutually agreed upon.

## Related Resources

- [Security Audit Preparation Guide](docs/AUDIT_PREP.md): Security checklist, threat modeling context, and audit scope guidelines.
- [Incident Response Runbook](docs/RUNBOOK_INCIDENT_RESPONSE.md): Protocol operator playbook for unexpected contract behavior, pause/maintenance mode, and incident recovery.
- [Contract Security Analysis](quicklendx-contracts/SECURITY_ANALYSIS.md): Overview of fuzz testing, property tests, and critical paths analyzed.
