# Invariant Self-Check (Protocol Heartbeat)

`invariant_self_check(env, admin) -> InvariantReport` is a single, admin-callable,
**read-only** entrypoint that aggregates the contract's existing cross-module
integrity checks into one structured report. It exists as a one-call protocol
"heartbeat" for incident response: instead of issuing many separate queries, an
operator calls this once and inspects the result.

- Module: [`quicklendx-contracts/src/invariants.rs`](../quicklendx-contracts/src/invariants.rs)
- Entrypoint: `QuickLendXContract::invariant_self_check` in
  [`src/contract.rs`](../quicklendx-contracts/src/contract.rs)
- Tests: [`src/test_invariant_self_check.rs`](../quicklendx-contracts/src/test_invariant_self_check.rs)

## Report shape

```rust
struct InvariantReport {
    checks: Vec<InvariantCheck>, // one row per invariant, in execution order
    all_passed: bool,            // true only if every row passed
    checked_at: u64,             // ledger timestamp of the report
}

struct InvariantCheck {
    check_name: String, // stable machine-readable id (match on this)
    passed: bool,       // invariant holds?
    evidence: String,   // what was verified / failure mode (diagnostic)
}
```

`all_passed` is the field to alert on. `check_name` is stable and safe to match
on in tooling; `evidence` is human-readable and may change wording.

## Composed checks

| `check_name`               | Invariant                                                                                                   | Source                                                       |
| :------------------------- | :---------------------------------------------------------------------------------------------------------- | :----------------------------------------------------------- |
| `no_orphan_investments`    | Every entry in the active-investment index still has `InvestmentStatus::Active`.                            | `InvestmentStorage::validate_no_orphan_investments`          |
| `audit_chain_integrity`    | Every invoice's audit trail validates (no missing/tampered entries).                                        | `AuditStorage::validate_invoice_audit_integrity` per invoice |
| `solvency`                 | No active investment has non-positive principal; no `Funded` invoice is funded beyond its face value.       | Composed accounting identity over investments + invoices     |
| `storage_index_coherence`  | Each invoice sits in exactly one status index matching its record; de-duplicated counts agree (no drift).   | `InvoiceStorage` status indexes vs. `get_all_invoice_ids`    |

## Security

- **Read-only.** The checks only call `get_*` / `validate_*` helpers — they never
  write storage. A failing or unauthorized call therefore cannot modify the
  ledger (`test_self_check_never_modifies_state` asserts the active index is
  unchanged before/after, and that repeated runs are identical).
- **Admin-gated.** `AdminStorage::require_admin_auth` runs *before* any check:
  it calls `admin.require_auth()` and then verifies the caller equals the stored
  protocol admin. Non-admins receive `QuickLendXError::NotAdmin`
  (`test_non_admin_is_rejected`); calling before initialization yields
  `OperationNotAllowed`.
- **Diagnostic, not remediation.** The report detects and names inconsistencies;
  it never repairs them.

### Incident-response usage

1. On an alert (or routinely), an admin calls `invariant_self_check`.
2. If `all_passed == false`, treat it as a containment signal: pause the
   protocol (see the pause module) and read each failing row's `check_name` /
   `evidence` to localize the violation.
3. Investigate and remediate out-of-band; re-run the heartbeat to confirm
   recovery. Because the call is read-only it is always safe to run, including
   while paused or mid-incident.

> Note: this is the on-chain counterpart to the off-chain monitor described in
> [`invariant-checks.md`](./invariant-checks.md) / [`invariants.md`](./invariants.md).
> Those cover infrastructure-level scheduling and DB consistency; this entrypoint
> verifies in-contract state in a single authenticated call.

## Tests

`cargo test test_invariant_self_check` covers:

- `test_fresh_contract_all_pass` — fresh contract: four checks, all green.
- `test_populated_healthy_state_passes` — populated/healthy ledger (proxy for a
  post-lifecycle state) still passes.
- `test_simulated_tampering_is_detected` — an injected orphan (terminal-status
  record left in the active index) flips `no_orphan_investments` and
  `all_passed` to `false`.
- `test_non_admin_is_rejected` — non-admin caller is rejected.
- `test_self_check_never_modifies_state` — failures/runs never mutate state.
