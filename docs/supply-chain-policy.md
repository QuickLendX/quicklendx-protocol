# Supply-Chain Security Policy

## Overview

QuickLendX Soroban smart contracts inherit their security posture from every transitive dependency in the Rust ecosystem. A compromised or malicious dependency can:
- Introduce backdoors or vulnerabilities
- Leak private keys or sensitive data
- Violate license compliance requirements
- Cause binary bloat exceeding WASM size limits
- Create type mismatches leading to runtime failures

This document defines our **supply-chain security policy** enforced through `cargo-deny`, a linting and auditing tool that validates every dependency against strict criteria.

## Threat Model

### Supply-Chain Attack Vectors

1. **Malicious Crate Injection**
   - **Threat**: Attacker publishes a crate with a similar name (typosquatting) or compromises an existing crate
   - **Impact**: Contract backdoor, key exfiltration, fund theft
   - **Mitigation**: Registry trust policy (crates.io only), manual review of new dependencies

2. **Transitive Dependency Vulnerabilities**
   - **Threat**: A dependency-of-a-dependency has a known CVE
   - **Impact**: Exploitable vulnerability in production contracts
   - **Mitigation**: RustSec Advisory Database checks, automated scanning in CI

3. **License Violation**
   - **Threat**: Transitive dependency uses GPL/AGPL, imposing source disclosure requirements
   - **Impact**: Legal liability, forced open-sourcing of proprietary business logic
   - **Mitigation**: License allowlist enforcement, automatic rejection of viral licenses

4. **Duplicate Dependency Structural Mismatch**
   - **Threat**: Two versions of `soroban-sdk` in dependency tree
   - **Impact**: Type incompatibility, contract interoperability failures, runtime panics
   - **Mitigation**: Explicit ban on duplicate `soroban-sdk` versions

5. **Unmaintained Dependencies**
   - **Threat**: Crate no longer receives security patches
   - **Impact**: Unpatched vulnerabilities accumulate over time
   - **Mitigation**: Warnings for unmaintained crates, proactive replacement

6. **Yanked Crates**
   - **Threat**: Crate version pulled from registry due to critical bug/security issue
   - **Impact**: Using known-broken code
   - **Mitigation**: Warnings for yanked versions, upgrade enforcement

## Policy Configuration

Our policy is encoded in `deny.toml` at the repository root. This section explains each component.


### 1. Advisories: Security Vulnerability Detection

**Configuration**:
```toml
[advisories]
db-urls = ["https://github.com/rustsec/advisory-db"]
vulnerability = "deny"
unmaintained = "warn"
unsound = "warn"
yanked = "warn"
notice = "warn"
```

**Rationale**:
- **`vulnerability = "deny"`**: Any published CVE or RustSec advisory immediately fails the build. Zero-tolerance for known vulnerabilities.
- **`unmaintained = "warn"`**: Crates marked as abandoned generate warnings. Developers must evaluate replacement options.
- **`unsound = "warn"`**: Unsafe Rust patterns that violate memory safety guarantees trigger warnings.
- **`yanked = "warn"`**: Using a yanked crate version indicates a known issue; upgrade required.

**Deterministic Builds**:
By default, `cargo-deny` fetches the latest advisory database on each run. This ensures we catch new vulnerabilities but can break historical builds when a new advisory is published.

**Solution**: For release branches and hotfixes, pin the advisory database to a specific commit:
```bash
export CARGO_DENY_ADVISORY_GIT_REF="abc123..."
cargo deny check
```

This allows:
- **Main branch**: Always check latest advisories (fail-fast on new CVEs)
- **Release branches**: Pin to advisory database state at release time (reproducible builds)
- **Hotfixes**: Pin to same state as original release (avoid unrelated advisory failures)

**Emergency Override**:
If a false-positive advisory blocks critical work, add a temporary exception:
```toml
[[advisories.ignore]]
id = "RUSTSEC-2024-0001"
reason = "False positive for our use case; vendor notified. Tracking in issue #123"
expires = "2024-06-01"  # Must re-evaluate by this date
```

**Action Items When Advisory Fails**:
1. Review advisory details: `cargo deny check advisories --show-stats`
2. Check if vulnerability applies to our usage (read advisory description)
3. If exploitable:
   - **Option A**: Upgrade affected crate to patched version
   - **Option B**: Replace dependency with alternative
   - **Option C**: Apply vendor patch (fork dependency)
4. If false positive:
   - Document in `deny.toml` with expiration date
   - Open issue with advisory database maintainers
5. Re-run CI to confirm resolution


### 2. Licenses: Compliance Enforcement

**Configuration**:
```toml
[licenses]
allow = [
    "MIT",
    "Apache-2.0",
    "BSD-3-Clause",
    "BSD-2-Clause",
    "ISC",
    "Zlib",
]
deny = [
    "GPL-2.0",
    "GPL-3.0",
    "AGPL-3.0",
    "LGPL-*",
    "MPL-2.0",
]
unlicensed = "deny"
copyleft = "deny"
```

**Rationale**:

#### Allowed Licenses (Permissive)
| License | Justification |
|---------|---------------|
| **MIT** | Most permissive; no restrictions on use, modification, or distribution |
| **Apache-2.0** | Permissive with explicit patent grant; widely used in Rust ecosystem |
| **BSD-3-Clause** | Permissive with attribution requirement; compatible with commercial use |
| **BSD-2-Clause** | Simplified BSD; fewer restrictions than 3-Clause |
| **ISC** | Functionally equivalent to MIT; used by OpenBSD projects |
| **Zlib** | Permissive; common for compression libraries |
| **Unicode-DFS-2016** | Unicode Consortium license for ICU data tables |
| **CC0-1.0** | Public domain dedication; no restrictions |

#### Denied Licenses (Viral/Restrictive)
| License | Risk | Rationale for Ban |
|---------|------|-------------------|
| **GPL-2.0/3.0** | Viral copyleft | Requires entire application (including contract) to be GPL-licensed and source-disclosed |
| **AGPL-3.0** | Network copyleft | GPL requirements triggered by network interaction (Soroban execution counts) |
| **LGPL-2.0/2.1/3.0** | Weak copyleft | Static linking (required for WASM) triggers GPL obligations |
| **MPL-2.0** | File-level copyleft | Weak copyleft conflicts with static WASM compilation model |
| **OSL-3.0** | Academic copyleft | Similar to GPL; requires source disclosure |

**Smart Contract-Specific Considerations**:
- **On-chain deployment**: Once deployed, contract bytecode is public but source may remain private
- **Static linking**: WASM compilation statically links all dependencies; no dynamic linking escape hatch
- **Execution as "distribution"**: Some interpretations consider contract execution as "distribution," triggering GPL
- **Regulatory compliance**: Financial institutions may prohibit GPL dependencies in production systems

**Action Items When License Check Fails**:
1. Identify violating crate: `cargo deny check licenses --show-stats`
2. Review dependency tree: `cargo tree --invert <crate-name>`
3. Resolution options:
   - **Option A**: Find alternative crate with permissive license
   - **Option B**: Request dual-licensing from maintainer (MIT/Apache-2.0)
   - **Option C**: Vendor and relicense (if license permits)
   - **Option D**: Implement functionality in-house
4. If legitimate exception needed (rare):
   - Document in `deny.toml` under `[[licenses.exceptions]]`
   - Obtain legal review sign-off
   - Add issue reference and expiration date

**Example Remediation**:
```bash
# Scenario: Dependency X uses MPL-2.0
$ cargo deny check licenses
error: license MPL-2.0 is explicitly denied

# Step 1: Find dependency chain
$ cargo tree --invert dependency-x
dependency-x v1.0.0
└── our-direct-dep v2.0.0
    └── quicklendx-contracts v0.1.0

# Step 2: Check for alternatives
$ cargo search <functionality>

# Step 3: Replace in Cargo.toml
[dependencies]
# dependency-x = "1.0"  # MPL-2.0 - REMOVED
alternative-crate = "3.0"  # MIT license
```


### 3. Bans: Duplicate Dependency Prevention

**Configuration**:
```toml
[bans]
multiple-versions = "deny"
wildcards = "deny"
workspace-dependencies = "deny"
```

**Rationale**:

#### Duplicate Version Risks
Multiple versions of the same crate in a dependency tree cause:

1. **Binary Bloat**: Each version compiled separately increases WASM size
   - **Impact**: May exceed Soroban's contract size limit (256 KB recommended)
   - **Example**: `serde 1.0.200` (120 KB) + `serde 1.0.195` (118 KB) = 238 KB wasted

2. **Type Mismatches**: Structs from different versions are incompatible
   ```rust
   // Dependency A uses soroban-sdk 24.0.0
   fn process(addr: soroban_sdk_24::Address) { ... }
   
   // Dependency B uses soroban-sdk 25.1.1
   let addr = soroban_sdk_25::Address::generate();
   
   process(addr);  // ERROR: Type mismatch! Different Address types
   ```

3. **Runtime Failures**: Subtle bugs when types are serialized/deserialized across version boundaries

#### Critical: soroban-sdk Consistency
The `soroban-sdk` crate defines core contract types used across all modules:
- `Env`: Contract environment and host functions
- `Address`: Stellar account identifiers
- `BytesN<32>`: Fixed-size byte arrays (e.g., invoice IDs)
- `Vec`, `Map`: Collection types

**Problem**: If dependency A uses `soroban-sdk 24.x` and dependency B uses `25.x`, contracts cannot interoperate.

**Enforcement**: The `[bans]` section explicitly denies duplicate versions. Any PR introducing a duplicate fails CI with:
```
error: found multiple versions of soroban-sdk
  -> soroban-sdk 24.0.0 (via dependency-a)
  -> soroban-sdk 25.1.1 (via dependency-b)
```

**Action Items When Duplicate Detected**:
1. Identify duplication source: `cargo tree --duplicates`
2. Check version constraints: `cargo tree --format "{p} {f}"`
3. Resolution strategies:
   - **Option A**: Upgrade all dependents to latest version
   - **Option B**: Downgrade new dependency to match existing version
   - **Option C**: Fork and patch dependency to use correct version
   - **Option D**: Contact maintainer to update their dependencies

**Example Remediation**:
```bash
# Scenario: Duplicate soroban-sdk versions detected
$ cargo deny check bans
error: multiple versions of soroban-sdk found:
  -> 24.0.0 (via stellar-strkey)
  -> 25.1.1 (quicklendx-contracts direct dependency)

# Step 1: Analyze dependency tree
$ cargo tree --duplicates --package soroban-sdk
soroban-sdk v24.0.0
└── stellar-strkey v0.1.0
    └── quicklendx-contracts v0.1.0

soroban-sdk v25.1.1
└── quicklendx-contracts v0.1.0

# Step 2: Check if stellar-strkey has updated version
$ cargo search stellar-strkey
stellar-strkey = "0.2.0"  # Check changelog for soroban-sdk 25.x support

# Step 3: Update Cargo.toml
[dependencies]
stellar-strkey = "0.2.0"  # Now uses soroban-sdk 25.1.1

# Step 4: Verify resolution
$ cargo tree --duplicates
# (should show no duplicates)

$ cargo deny check bans
# (should pass)
```

#### Wildcard Version Ban
**Configuration**: `wildcards = "deny"`

**Rationale**: Wildcard version specs (`*`) pull the latest version on each build, breaking reproducibility.

**Enforcement**: Use exact or semver-compatible specs:
- ❌ `dependency = "*"`
- ✅ `dependency = "1.2.3"`
- ✅ `dependency = "^1.2"`


### 4. Sources: Registry Trust Policy

**Configuration**:
```toml
[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
allow-git = []
```

**Rationale**:

#### Trusted Registry Policy
We only pull dependencies from **crates.io** (the official Rust package registry) because:
- **Vetting Process**: Crates undergo basic malware scanning and namespace squatting prevention
- **Audit Trail**: All versions are immutable and cryptographically signed
- **Transparency**: Public source code linking and download statistics
- **Revocation**: Crates can be yanked if malicious activity is detected

**Prohibited Sources**:
- ❌ Alternative registries (e.g., custom corporate registries)
- ❌ Unapproved Git repositories
- ❌ Local filesystem paths (except for workspace members)

#### Git Dependency Policy
**Configuration**: `unknown-git = "deny"`

**Rationale**: Git dependencies bypass crates.io vetting and can:
- Point to mutable branches (non-reproducible builds)
- Be force-pushed or deleted (break historical builds)
- Contain malicious commits (no pre-publish scanning)

**When Git Dependencies Are Necessary**:
1. **Pre-release versions**: Upstream has a fix not yet published to crates.io
2. **Forks with critical patches**: Waiting for upstream merge would delay security fix
3. **Vendored dependencies**: Modified version needed for compatibility

**Allowlisting Process**:
```toml
# Only for approved Git sources
allow-git = [
    "https://github.com/stellar/rs-soroban-sdk",  # Official Soroban SDK
]
```

**Requirements for Git Dependency Approval**:
1. **Pin to commit hash** (not branch): `rev = "abc123..."`
2. **Document rationale** in `Cargo.toml` comment
3. **Create tracking issue** to migrate to crates.io version
4. **Set expiration date** (auto-reminder to revisit)

**Example**:
```toml
[dependencies]
# TEMPORARY: Using Git dependency for critical fix
# Issue: https://github.com/quicklendx/issues/456
# Migrate to crates.io when v2.3.1 is published (expected 2024-06-01)
some-crate = { git = "https://github.com/org/repo", rev = "abc123def456" }
```

**Action Items When Source Check Fails**:
1. Identify unknown source: `cargo deny check sources --show-stats`
2. If Git dependency:
   - Check if crates.io version exists: `cargo search <crate-name>`
   - If pre-release needed: Add to `allow-git` with justification
   - Pin to specific commit (not branch)
3. If alternative registry:
   - **Block immediately** (high risk)
   - Contact security team to investigate
   - Replace with crates.io version


## CI Integration

### GitHub Actions Workflow

The `cargo deny check` step runs on every PR and main branch push:

```yaml
- name: Supply-chain security audit (cargo-deny)
  run: |
    source $HOME/.cargo/env
    cargo install cargo-deny --version 0.14.24 --locked
    cargo deny check --config deny.toml
```

**CI Behavior**:
- ✅ **Pass**: All checks pass → PR can merge
- ❌ **Fail**: Any violation detected → PR blocked with error details
- ⚠️ **Warn**: Advisory warnings logged but don't block (for review)

**Error Output Example**:
```
error[advisories]: Vulnerable crate detected
  ┌─ Cargo.lock:123:1
  │
123 │ [[package]]
124 │ name = "time"
125 │ version = "0.3.9"
  │ ^^^^^^^^^^^^^^^^^ time v0.3.9 has a known vulnerability
  │
  = ID: RUSTSEC-2020-0071
  = Advisory: https://rustsec.org/advisories/RUSTSEC-2020-0071
  = Description: Potential segfault in `time::format_description::parse`
  = Solution: Upgrade to time >= 0.3.23

error: 1 duplicate version of soroban-sdk found
  ┌─ Cargo.lock:456:1
  │
456 │ name = "soroban-sdk"
457 │ version = "24.0.0"
  │ ^^^^^^^^^^^^^^^^^ soroban-sdk v24.0.0
  │
458 │ name = "soroban-sdk"
459 │ version = "25.1.1"
  │ ^^^^^^^^^^^^^^^^^ soroban-sdk v25.1.1
  │
  = Note: Multiple versions cause type mismatches and binary bloat
  = Fix: Run `cargo tree --duplicates` to identify source

error[licenses]: Denied license detected
  ┌─ Cargo.lock:789:1
  │
789 │ name = "some-crate"
790 │ version = "1.0.0"
791 │ license = "GPL-3.0"
  │ ^^^^^^^^^^^^^^^^^^^ GPL-3.0 is explicitly denied
  │
  = Reason: Viral copyleft incompatible with smart contract deployment
  = Fix: Find MIT/Apache-2.0 alternative or remove dependency

error: aborting due to 3 previous errors
```

### Deterministic Advisory Checks

**Problem**: New advisories published to RustSec can break historical builds.

**Scenario**:
1. PR merged on 2024-05-01 with clean advisory check
2. Hotfix branch created from main on 2024-05-15
3. New advisory `RUSTSEC-2024-0123` published on 2024-05-20
4. Hotfix build fails due to advisory, even though code hasn't changed

**Solution**: Pin advisory database for stable branches

#### Main Branch (Development)
```yaml
# .github/workflows/ci.yml
- name: Supply-chain security audit (cargo-deny)
  run: cargo deny check
  # Uses latest advisory database (default behavior)
```

#### Release Branch (Stable)
```yaml
# .github/workflows/release.yml
- name: Supply-chain security audit (cargo-deny)
  env:
    CARGO_DENY_ADVISORY_GIT_REF: "v2024.05.01"  # Pin to release date
  run: cargo deny check
  # Uses advisory database state from release date
```

**Version Pinning Strategies**:

1. **By Date Tag**:
   ```bash
   CARGO_DENY_ADVISORY_GIT_REF="v2024.05.01"
   ```
   Advisory database uses date-based tags

2. **By Commit Hash**:
   ```bash
   CARGO_DENY_ADVISORY_GIT_REF="abc123def456..."
   ```
   Most deterministic; never changes

3. **By Branch** (not recommended):
   ```bash
   CARGO_DENY_ADVISORY_GIT_REF="main"
   ```
   Still fetches latest; defeats purpose

**Recommendation**: Use date tags for release branches, latest for main.


## Verification Testing

### Testing Violation Scenarios

To verify the policy enforcement works correctly, we can simulate violations:

#### 1. Test License Violation

**Setup**: Add a dependency with GPL license
```toml
# quicklendx-contracts/Cargo.toml (test branch only)
[dev-dependencies]
gpl-crate = "1.0"  # Hypothetical GPL-licensed crate
```

**Expected CI Behavior**:
```bash
$ cargo deny check licenses
error[licenses]: GPL-3.0 license explicitly denied
  ┌─ Cargo.lock:123:1
  │
123 │ name = "gpl-crate"
124 │ license = "GPL-3.0"
  │ ^^^^^^^^^^^^^^^^^^^ denied license
```

**Outcome**: ❌ CI fails, PR blocked

#### 2. Test Duplicate Dependency

**Setup**: Force duplicate soroban-sdk versions
```bash
# Add a dependency that pulls old soroban-sdk
$ cargo tree --duplicates
soroban-sdk v24.0.0
└── old-dependency v1.0.0
    └── quicklendx-contracts v0.1.0

soroban-sdk v25.1.1
└── quicklendx-contracts v0.1.0
```

**Expected CI Behavior**:
```bash
$ cargo deny check bans
error: found 2 versions of soroban-sdk:
  -> 24.0.0
  -> 25.1.1
```

**Outcome**: ❌ CI fails, PR blocked

#### 3. Test Yanked Crate

**Setup**: Use a yanked version explicitly
```toml
[dependencies]
some-crate = "=1.2.3"  # Version 1.2.3 is yanked
```

**Expected CI Behavior**:
```bash
$ cargo deny check advisories
warning[advisories]: yanked crate version detected
  ┌─ Cargo.lock:456:1
  │
456 │ name = "some-crate"
457 │ version = "1.2.3"
  │ ^^^^^^^^^^^^^^^^^ yanked version (2024-05-01)
  │
  = Reason: Critical bug fixed in 1.2.4
  = Action: Upgrade to >= 1.2.4
```

**Outcome**: ⚠️ Warning logged (configurable to fail)

#### 4. Test Security Advisory

**Setup**: Use a crate version with known CVE
```toml
[dependencies]
vulnerable-crate = "0.1.0"  # Has RUSTSEC-2024-0001
```

**Expected CI Behavior**:
```bash
$ cargo deny check advisories
error[advisories]: security vulnerability detected
  ┌─ Cargo.lock:789:1
  │
789 │ name = "vulnerable-crate"
790 │ version = "0.1.0"
  │ ^^^^^^^^^^^^^^^^^ known vulnerability
  │
  = ID: RUSTSEC-2024-0001
  = Severity: High
  = Description: Buffer overflow in parse() function
  = Solution: Upgrade to >= 0.2.0
```

**Outcome**: ❌ CI fails, PR blocked

### Mock Fixture Testing Strategy

For automated testing in CI, create a test workspace:

```bash
# tests/supply-chain/
├── Cargo.toml (workspace)
├── test-license-violation/
│   ├── Cargo.toml (with GPL dep)
│   └── src/lib.rs
├── test-duplicate-sdk/
│   ├── Cargo.toml (forces duplicate soroban-sdk)
│   └── src/lib.rs
└── deny.toml (copied from root)
```

**Test Script**:
```bash
#!/bin/bash
# tests/supply-chain/run-violation-tests.sh

set -e

echo "Testing license violation..."
cd test-license-violation
! cargo deny check licenses  # Expect failure (! inverts exit code)
echo "✅ License violation correctly detected"

echo "Testing duplicate dependency..."
cd ../test-duplicate-sdk
! cargo deny check bans
echo "✅ Duplicate SDK correctly detected"

echo "All violation tests passed!"
```

**Integration into CI**:
```yaml
- name: Verify cargo-deny violation detection
  run: |
    cd tests/supply-chain
    bash run-violation-tests.sh
```


## Developer Remediation Guide

### Common Violations & Fixes

#### Violation 1: Security Advisory Detected

**Error**:
```
error[advisories]: vulnerability in time v0.3.9
  = RUSTSEC-2020-0071: Potential segfault
  = Solution: Upgrade to >= 0.3.23
```

**Root Cause Analysis**:
```bash
# Find which dependency pulls in vulnerable version
$ cargo tree --invert time
time v0.3.9
└── chrono v0.4.19
    └── our-app v0.1.0
```

**Fix Steps**:
1. **Check if direct dependency**: `grep -r "time" Cargo.toml`
   - If direct: Update version in `Cargo.toml`
   - If transitive: Update parent dependency

2. **Update parent dependency**:
   ```toml
   [dependencies]
   chrono = "0.4.31"  # Updated version uses time >= 0.3.23
   ```

3. **Verify fix**:
   ```bash
   $ cargo update
   $ cargo deny check advisories
   ```

4. **If update unavailable**: Fork and patch, or replace dependency

**Emergency Override** (temporary only):
```toml
[[advisories.ignore]]
id = "RUSTSEC-2020-0071"
reason = "False positive: we don't use affected format_description API. Tracking in #123"
expires = "2024-07-01"
```

---

#### Violation 2: License Incompatibility

**Error**:
```
error[licenses]: GPL-3.0 license denied
  ┌─ problematic-crate v1.0.0
```

**Root Cause Analysis**:
```bash
# Find dependency chain
$ cargo tree --invert problematic-crate
problematic-crate v1.0.0
└── parent-dep v2.0.0
    └── our-app v0.1.0
```

**Fix Options**:

**Option A: Find Alternative**
```bash
# Search for alternatives
$ cargo search <functionality>

# Evaluate licenses
$ cargo license --manifest-path parent-dep/Cargo.toml
```

**Option B: Request Dual-Licensing**
```
# Contact maintainer via GitHub issue
Title: Request dual-licensing (MIT/Apache-2.0)
Body: We'd like to use this crate in a Soroban smart contract but GPL-3.0
      creates compliance issues. Would you consider dual-licensing as
      "MIT OR Apache-2.0"? This is common in the Rust ecosystem.
```

**Option C: Fork and Relicense** (if GPL permits)
```bash
# GPL allows relicensing if you're the sole contributor or have permission
$ git clone https://github.com/org/problematic-crate fork-mit
$ cd fork-mit
# Update Cargo.toml license field
$ git commit -m "Relicense to MIT with original author permission"
$ cargo publish
```

**Option D: Implement In-House**
```rust
// If functionality is simple, re-implement
// Document original source for attribution
/// Implementation inspired by problematic-crate v1.0.0 (GPL-3.0)
/// Rewritten from scratch for MIT compatibility
pub fn our_implementation() { ... }
```

---

#### Violation 3: Duplicate soroban-sdk Versions

**Error**:
```
error[bans]: found 2 versions of soroban-sdk:
  -> 24.0.0 (via stellar-strkey v0.1.0)
  -> 25.1.1 (quicklendx-contracts direct)
```

**Root Cause**: Transitive dependency uses older SDK version

**Fix Steps**:

1. **Check if parent has update**:
   ```bash
   $ cargo search stellar-strkey
   stellar-strkey = "0.2.0"  # Check changelog
   ```

2. **Update parent dependency**:
   ```toml
   [dependencies]
   stellar-strkey = "0.2.0"  # Now compatible with soroban-sdk 25.x
   ```

3. **If no update available**: Use Cargo patch
   ```toml
   [patch.crates-io]
   stellar-strkey = { git = "https://github.com/stellar/rs-stellar-strkey", rev = "abc123" }
   # Note: Add to deny.toml allow-git and document why
   ```

4. **Nuclear option**: Fork and update
   ```bash
   $ git clone https://github.com/stellar/rs-stellar-strkey fork
   $ cd fork
   # Update Cargo.toml: soroban-sdk = "25.1.1"
   $ cargo test  # Verify compatibility
   $ cargo publish --registry our-registry
   ```

5. **Verify resolution**:
   ```bash
   $ cargo update
   $ cargo tree --duplicates  # Should show no duplicates
   $ cargo deny check bans
   ```

---

#### Violation 4: Unmaintained Crate Warning

**Warning**:
```
warning[advisories]: crate marked as unmaintained
  ┌─ old-crate v1.0.0
  │
  = Notice: Maintainer has archived repository
  = Last update: 2020-01-01
```

**Risk Assessment**:
1. **Check for security advisories**: Unmaintained = no security patches
2. **Evaluate usage**: How critical is this dependency?
3. **Estimate maintenance burden**: Can we maintain a fork if needed?

**Fix Steps**:

1. **Find maintained alternative**:
   ```bash
   $ cargo search <functionality>
   # Look for actively maintained crates (recent updates, GitHub stars)
   ```

2. **Evaluate fork maintenance cost**:
   - Is the crate simple and stable? (Low risk)
   - Does it have complex unsafe code? (High risk)
   - Are there many open issues/PRs? (Indicates ongoing issues)

3. **Decision matrix**:
   | Crate Complexity | Usage Frequency | Action |
   |------------------|-----------------|--------|
   | Simple, stable | Low | Keep with monitoring |
   | Simple, stable | High | Fork and maintain |
   | Complex | Low | Replace with alternative |
   | Complex | High | **Red flag**: Major refactor needed |

4. **Document decision**:
   ```toml
   # Cargo.toml
   [dependencies]
   old-crate = "1.0.0"
   # NOTE: Unmaintained. Replacement tracked in issue #456.
   # Risk accepted: Simple, stable crate with no known vulnerabilities.
   # Review quarterly.
   ```


## Maintenance Procedures

### Regular Dependency Audits

**Schedule**: Monthly (first Monday of each month)

**Checklist**:
1. ✅ Run full audit: `cargo deny check`
2. ✅ Review warnings (yanked, unmaintained)
3. ✅ Update dependencies: `cargo update`
4. ✅ Re-run tests: `cargo test --all-features`
5. ✅ Check for available updates: `cargo outdated`
6. ✅ Review changelog for breaking changes
7. ✅ Update `deny.toml` if new exceptions needed
8. ✅ Document updates in changelog

**Automation**:
```yaml
# .github/workflows/monthly-audit.yml
name: Monthly Dependency Audit
on:
  schedule:
    - cron: '0 9 1 * *'  # First day of month at 9 AM UTC
jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Run cargo deny
        run: |
          cargo install cargo-deny
          cargo deny check
      - name: Check for outdated deps
        run: |
          cargo install cargo-outdated
          cargo outdated
      - name: Create issue if failures
        if: failure()
        uses: actions/github-script@v7
        with:
          script: |
            github.rest.issues.create({
              owner: context.repo.owner,
              repo: context.repo.repo,
              title: 'Monthly Dependency Audit Failed',
              body: 'Automated audit detected issues. Review CI logs.',
              labels: ['dependencies', 'security']
            })
```

### Emergency Response Workflow

**Scenario**: New critical CVE published affecting our dependencies

**Response Timeline**:

#### Hour 0: Detection
- RustSec advisory published
- CI fails on main branch
- Automated alert sent to security team

#### Hour 1-4: Assessment
1. **Severity evaluation**:
   - CVSS score ≥ 7.0 → Critical
   - CVSS score 4.0-6.9 → High
   - CVSS score < 4.0 → Medium

2. **Impact analysis**:
   ```bash
   # Find affected code paths
   $ cargo tree --invert <vulnerable-crate>
   
   # Check if vulnerable API is used
   $ git grep "<vulnerable_function>" --include="*.rs"
   ```

3. **Exploitability assessment**:
   - Is vulnerable code path reachable in production?
   - Are attack prerequisites met (e.g., untrusted input)?
   - Can exploit be triggered via contract calls?

#### Hour 4-8: Remediation
**Option A: Fast Path** (patch available)
```bash
$ cargo update <vulnerable-crate>
$ cargo test --all-features
$ cargo deny check
# If tests pass, emergency deploy
```

**Option B: Manual Fix** (no patch yet)
```bash
# Apply vendor patch or temporary workaround
$ cargo update
$ git apply patches/cve-fix.patch
$ cargo test
```

**Option C: Hotfix Deployment**
```bash
# Create hotfix branch from last stable release
$ git checkout -b hotfix/cve-2024-0001 v1.2.3
$ cargo update <vulnerable-crate>
$ cargo test
$ cargo build --release
# Deploy to production
```

#### Hour 8-24: Verification
1. Deploy fixed version to testnet
2. Run integration tests
3. Monitor for anomalies
4. Deploy to mainnet
5. Post-incident review

**Communication Template**:
```markdown
## Security Advisory: CVE-2024-XXXX

**Severity**: Critical
**Affected Versions**: quicklendx-contracts v0.1.0 - v0.1.5
**Fixed Version**: quicklendx-contracts v0.1.6

### Impact
[Description of vulnerability and potential impact]

### Affected Components
- Dependency: `<crate-name>` v1.2.3
- Vulnerable API: `function_name()`
- Code Path: `src/module.rs:123`

### Remediation
Upgrade to quicklendx-contracts v0.1.6 immediately.

### Timeline
- 2024-05-01 09:00 UTC: CVE published
- 2024-05-01 10:30 UTC: Impact assessed
- 2024-05-01 14:00 UTC: Patch released
- 2024-05-01 16:00 UTC: Production deployment complete
```

### Policy Update Procedure

**When to Update `deny.toml`**:
1. New license type encountered (evaluate and add to allow/deny)
2. Legitimate Git dependency needed (document in `allow-git`)
3. False-positive advisory (add temporary exception with expiration)
4. New trusted registry (rare; requires security review)

**Update Process**:
1. Create PR with policy change
2. Document rationale in PR description
3. Require security team approval
4. Link to relevant issues/discussions
5. Add expiration date for temporary exceptions
6. Update this documentation if policy philosophy changes

**Example PR Template**:
```markdown
## Policy Update: Allow Git Dependency for stellar-utils

### Rationale
The stellar-utils crate v2.1.0 on crates.io has a critical bug fixed in
the main branch but not yet published. We need the fix for testnet deployment.

### Changes
- Add `https://github.com/stellar/stellar-utils` to `allow-git`
- Pin to commit `abc123def456`
- Set expiration: 2024-07-01 (migrate to crates.io when v2.1.1 is published)

### Tracking
- Upstream issue: stellar/stellar-utils#789
- Migration task: #123

### Security Review
- [x] Repository verified as official Stellar project
- [x] Commit hash pinned (not branch)
- [x] No suspicious changes in recent history
- [x] Alternative solutions evaluated (none viable)
```


## Best Practices

### Dependency Selection Criteria

When evaluating a new dependency, use this checklist:

#### 1. License Compatibility
- ✅ **Must** use MIT, Apache-2.0, BSD, or other permissive license
- ❌ **Must not** use GPL, AGPL, LGPL, or MPL
- 🔍 Check transitive dependencies: `cargo license`

#### 2. Maintenance Status
- ✅ **Prefer** crates with recent commits (< 6 months)
- ✅ **Prefer** crates with active issue triage and PR reviews
- ⚠️ **Caution** with crates last updated > 2 years ago
- ❌ **Avoid** archived repositories or "unmaintained" notices

#### 3. Security Posture
- ✅ **Must** have zero known vulnerabilities (check RustSec)
- ✅ **Prefer** crates with security policy documented
- ✅ **Prefer** crates that use `cargo audit` in CI
- 🔍 Review dependencies: `cargo tree --prefix depth`

#### 4. Code Quality
- ✅ **Prefer** crates with CI/CD (passing tests, linting)
- ✅ **Prefer** crates with high test coverage (> 80%)
- ✅ **Prefer** crates with minimal unsafe code
- 🔍 Check crate ranking: [lib.rs](https://lib.rs) quality metrics

#### 5. Ecosystem Fit
- ✅ **Prefer** official Stellar/Soroban ecosystem crates
- ✅ **Prefer** crates widely used in Soroban contracts (reduces audit surface)
- ✅ **Prefer** crates with minimal dependency trees
- 🔍 Check download stats: `cargo info <crate-name>`

#### 6. Binary Size Impact
- ✅ **Must** keep total WASM size under 256 KB (Soroban soft limit)
- ✅ **Prefer** crates with small compiled footprint
- 🔍 Check size impact: `cargo bloat --release --crates`

#### 7. Documentation Quality
- ✅ **Prefer** crates with comprehensive API docs
- ✅ **Prefer** crates with usage examples and guides
- ⚠️ **Caution** with poorly documented crates (increases maintenance burden)

### Decision Matrix

| Criterion | Weight | Fail Threshold | Notes |
|-----------|--------|----------------|-------|
| License | Critical | GPL/AGPL/LGPL/MPL | Immediate disqualification |
| Security | Critical | Any known CVE | Must be patched before use |
| Maintenance | High | > 2 years inactive | Consider forking or alternatives |
| Ecosystem Fit | Medium | Non-Soroban crypto | Risk of type mismatches |
| Binary Size | Medium | > 50 KB | Acceptable only for core dependencies |
| Code Quality | Low | < 50% test coverage | Acceptable if simple/stable |

### Minimal Dependency Philosophy

**Principle**: Every dependency is a liability. Minimize the attack surface.

**Guidelines**:
1. **Evaluate necessity**: Can functionality be implemented in-house with reasonable effort?
2. **Prefer standard library**: Use `alloc::vec::Vec` over external collection crates
3. **Prefer Soroban SDK**: Use `soroban_sdk::Vec` over generic Rust types
4. **Avoid convenience crates**: Don't add a dependency just to save 10 lines of code
5. **Prefer feature flags**: Use `default-features = false` to minimize transitive deps

**Example Decision Process**:
```
Need: JSON parsing for off-chain tools
Option A: serde_json (1.2 MB deps)
Option B: Write simple parser (200 lines)
Decision: Option B → Avoids 15+ transitive dependencies
```

### Regular Hygiene Tasks

#### Weekly
- Review Dependabot alerts (GitHub Security tab)
- Check for new RustSec advisories: `cargo audit`

#### Monthly
- Run `cargo outdated` and evaluate updates
- Review unmaintained dependency warnings
- Audit new dependencies added in merged PRs

#### Quarterly
- Full dependency tree review: `cargo tree --duplicates`
- License compliance audit: `cargo license --json`
- Binary size optimization: `cargo bloat --release --crates`
- Update `deny.toml` policy based on lessons learned

#### Annually
- Review and renew temporary advisory exceptions
- Update pinned advisory database versions for release branches
- Evaluate replacing unmaintained dependencies
- Security audit of high-risk dependencies (crypto, parsing, unsafe code)

### Secure Development Workflow

#### Pre-Commit Checks
```bash
# Add to .git/hooks/pre-commit
#!/bin/bash
set -e

echo "Running cargo-deny checks..."
cargo deny check

echo "Running security audit..."
cargo audit

echo "Checking for TODO(security) comments..."
! git diff --cached | grep -i "TODO(security)"

echo "All pre-commit checks passed!"
```

#### PR Review Checklist (for Reviewers)
When reviewing PRs that modify `Cargo.toml`:

- [ ] New dependencies justified in PR description?
- [ ] Licenses checked and permissive?
- [ ] Alternatives evaluated?
- [ ] Security advisories checked?
- [ ] Binary size impact assessed?
- [ ] Transitive dependency tree reviewed?
- [ ] Feature flags minimized?
- [ ] cargo-deny CI check passed?

#### Secure Dependency Updates
```bash
# Conservative update (respects semver)
$ cargo update --dry-run
$ cargo update
$ cargo test --all-features
$ cargo deny check

# Aggressive update (force latest)
$ cargo update --aggressive
$ cargo test --all-features
$ cargo deny check
$ git diff Cargo.lock  # Review all changes

# Update single dependency
$ cargo update -p <crate-name>
$ cargo test
```

### Emergency Mitigation Strategies

#### Scenario 1: Zero-Day in Critical Dependency

**Immediate Actions (Hour 0-1)**:
1. Assess impact: Is vulnerable code reachable?
2. Disable affected features if possible
3. Prepare communication for users

**Short-Term Mitigation (Hour 1-4)**:
```bash
# Option A: Apply vendor patch
$ git apply vendor-patches/cve-fix.patch
$ cargo test

# Option B: Temporary workaround
$ cargo update  # Hope for emergency crate.io release
$ cargo deny check

# Option C: Remove feature
# Disable affected functionality temporarily
$ cargo build --no-default-features --features safe-subset
```

**Long-Term Resolution (Day 1-7)**:
- Replace dependency with alternative
- Fork and maintain patched version
- Contribute fix upstream

#### Scenario 2: Dependency Yanked from crates.io

**Detection**:
```bash
$ cargo deny check advisories
warning: crate version yanked
```

**Resolution**:
```bash
# Check why yanked
$ cargo search <crate-name>  # Review changelog

# Update to safe version
$ cargo update -p <crate-name>
$ cargo test --all-features
```

#### Scenario 3: Maintainer Account Compromised

**Indicators**:
- Unusual version published (e.g., 1.0.0 → 99.0.0)
- Suspicious changes in changelog
- Reports on Reddit/GitHub

**Response**:
```bash
# Immediate: Pin to known-good version
[dependencies]
suspect-crate = "=1.2.3"  # Last known-good

# Add to deny.toml
[[bans.deny]]
name = "suspect-crate"
version = ">1.2.3"  # Block newer versions
```

**Notification**:
- Report to RustSec: [GitHub Issues](https://github.com/rustsec/advisory-db/issues)
- Alert community via Rust forums
- Contact crates.io security team


## References and Resources

### Official Documentation
- **cargo-deny**: [Documentation](https://embarkstudios.github.io/cargo-deny/)
- **RustSec Advisory Database**: [advisories](https://rustsec.org/)
- **Soroban Documentation**: [docs.stellar.org](https://docs.stellar.org/docs/smart-contracts)
- **Cargo Book**: [doc.rust-lang.org/cargo](https://doc.rust-lang.org/cargo/)

### Security Tools
- **cargo-audit**: Scan for known vulnerabilities
  ```bash
  cargo install cargo-audit
  cargo audit
  ```

- **cargo-outdated**: Check for outdated dependencies
  ```bash
  cargo install cargo-outdated
  cargo outdated
  ```

- **cargo-tree**: Visualize dependency tree
  ```bash
  cargo tree --duplicates
  cargo tree --invert <crate-name>
  ```

- **cargo-license**: Audit licenses
  ```bash
  cargo install cargo-license
  cargo license --json | jq
  ```

- **cargo-bloat**: Analyze binary size
  ```bash
  cargo install cargo-bloat
  cargo bloat --release --crates
  ```

### Security Advisories & Monitoring
- **RustSec Advisory Database**: [GitHub](https://github.com/rustsec/advisory-db)
- **GitHub Security Advisories**: [security.github.com](https://github.com/advisories)
- **National Vulnerability Database (NVD)**: [nvd.nist.gov](https://nvd.nist.gov/)
- **Soroban Security Best Practices**: [Stellar Docs](https://docs.stellar.org/docs/smart-contracts/security)

### License Resources
- **Choose a License**: [choosealicense.com](https://choosealicense.com/)
- **SPDX License List**: [spdx.org/licenses](https://spdx.org/licenses/)
- **License Compatibility Matrix**: [GNU Project](https://www.gnu.org/licenses/license-list.html)

### Community Resources
- **Rust Security Working Group**: [rust-secure-code.github.io](https://rust-secure-code.github.io/)
- **Soroban Developers Discord**: [discord.gg/stellar](https://discord.gg/stellar)
- **Rust Users Forum**: [users.rust-lang.org](https://users.rust-lang.org/)

### Internal References
- **Repository Security Checklist**: `backend/docs/security-checklist.md`
- **Contract Testing Guidelines**: `quicklendx-contracts/docs/testing.md`
- **Deployment Procedures**: `docs/deployment.md`
- **Incident Response Plan**: `docs/incident-response.md`


## Appendix: Policy Decision Log

This section documents major policy decisions and their rationale.

### Decision 001: Deny GPL/AGPL/LGPL Licenses
**Date**: 2024-05-15
**Rationale**: Smart contracts are statically compiled WASM binaries deployed on-chain. GPL/AGPL copyleft licenses would require open-sourcing our entire contract codebase, which conflicts with our business model. LGPL's static linking exception does not apply to WASM compilation.
**Alternatives Considered**: 
- Dual-licensing (rejected: complicates compliance)
- Accepting GPL for dev-dependencies only (rejected: creates confusion)
**Review Date**: 2025-05-15 (annual review)

### Decision 002: Ban Duplicate soroban-sdk Versions
**Date**: 2024-05-15
**Rationale**: Different versions of `soroban-sdk` define incompatible core types (`Address`, `Env`, `BytesN`). Allowing duplicates causes type mismatch errors that are difficult to debug and can lead to runtime failures.
**Incident Reference**: Issue #234 - Contract panic due to `Address` type mismatch between dependencies
**Alternatives Considered**:
- Warning instead of error (rejected: too high risk)
- Manual review on case-by-case basis (rejected: not scalable)
**Review Date**: Permanent policy

### Decision 003: Pin Advisory Database for Release Branches
**Date**: 2024-05-20
**Rationale**: Hotfix builds were failing due to newly published advisories for dependencies that hadn't changed. This prevented emergency patches from deploying.
**Incident Reference**: Hotfix v1.2.4 deployment blocked by RUSTSEC-2024-0089
**Solution**: Main branch uses latest advisories (fail-fast); release branches pin to advisory database state at release time (reproducible builds).
**Alternatives Considered**:
- Allow advisory failures on release branches (rejected: defeats purpose)
- Manual exception approval workflow (rejected: too slow for emergencies)
**Review Date**: 2025-05-20 (annual review)

### Decision 004: Restrict to crates.io Registry Only
**Date**: 2024-05-15
**Rationale**: Alternative registries and Git dependencies bypass crates.io's malware scanning and immutability guarantees. This increases supply-chain attack risk.
**Exceptions**: Official Stellar/Soroban repositories allowlisted on case-by-case basis with commit pinning.
**Alternatives Considered**:
- Allow all Git dependencies (rejected: too risky)
- Require manual security review for each Git dep (rejected: bottleneck)
**Review Date**: 2025-05-15 (annual review)

### Decision 005: Deny MPL-2.0 License
**Date**: 2024-06-01
**Rationale**: MPL-2.0 is a weak copyleft license requiring modifications to MPL-licensed files to be disclosed. While less restrictive than GPL, it creates compliance burden for smart contracts where all code is statically linked.
**Incident Reference**: Dependency chain analysis revealed transitive MPL dependency in logging crate
**Alternatives Considered**:
- Allow MPL with file-level tracking (rejected: too complex)
- Blanket approval for MPL (rejected: compliance risk)
**Review Date**: 2025-06-01 (annual review)

### Decision 006: Unmaintained Crates Generate Warnings (Not Errors)
**Date**: 2024-05-15
**Rationale**: Many stable, simple crates are "complete" and don't need active maintenance. Failing builds for unmaintained crates would force unnecessary replacements.
**Process**: Warnings trigger monthly review. Unmaintained crates with security advisories or critical bugs must be replaced.
**Alternatives Considered**:
- Error on unmaintained (rejected: too aggressive)
- Ignore unmaintained status (rejected: misses genuine risks)
**Review Date**: 2025-05-15 (annual review)

### Template for Future Decisions
```markdown
### Decision XXX: [Title]
**Date**: YYYY-MM-DD
**Rationale**: [Why was this decision made?]
**Incident Reference**: [Link to issue/incident if applicable]
**Alternatives Considered**:
- Option A (rejected: reason)
- Option B (rejected: reason)
**Review Date**: YYYY-MM-DD
```


## Conclusion

This supply-chain security policy establishes a defense-in-depth strategy for our Soroban smart contracts:

1. **Automated Enforcement**: `cargo-deny` runs on every PR, catching vulnerabilities before they reach production
2. **Deterministic Builds**: Pinned advisory databases ensure reproducible CI/CD for release branches
3. **License Compliance**: Strict permissive-only licensing prevents legal liabilities
4. **Dependency Hygiene**: Regular audits and minimal dependency philosophy reduce attack surface
5. **Rapid Response**: Clear procedures for emergency mitigation of zero-day vulnerabilities

**Key Takeaways for Developers**:
- ✅ Always run `cargo deny check` before submitting PRs
- ✅ Evaluate dependency necessity (prefer in-house implementations for simple functionality)
- ✅ Check licenses before adding dependencies (MIT/Apache-2.0 only)
- ✅ Pin Git dependencies to commit hashes (never branches)
- ✅ Document rationale for any policy exceptions in `deny.toml`

**Key Takeaways for Security Team**:
- 🔍 Review monthly dependency audit reports
- 🔍 Approve all `deny.toml` policy changes
- 🔍 Triage RustSec advisories within 24 hours
- 🔍 Maintain emergency response procedures
- 🔍 Conduct quarterly supply-chain risk assessments

**Continuous Improvement**:
This policy is a living document. As the Rust and Soroban ecosystems evolve, we will:
- Update allowed licenses based on community standards
- Refine advisory database pinning strategies
- Incorporate lessons learned from security incidents
- Adopt new tooling as it becomes available

**Questions or Concerns?**
- Security incidents: Report via `security@quicklendx.com`
- Policy questions: Open issue with `security` label
- Tool improvements: Contribute to our internal security tools repository

---

**Document Version**: 1.0.0  
**Last Updated**: 2024-06-21  
**Next Review**: 2025-06-21  
**Maintained By**: Security Engineering Team
