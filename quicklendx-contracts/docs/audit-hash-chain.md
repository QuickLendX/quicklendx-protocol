# Audit hash chain

`AuditLogEntry` now stores `prev_hash`, an invoice-local link to the previous
audit entry. The first entry for an invoice uses a fixed 32-byte zero sentinel.
Every later entry stores the domain-separated SHA-256 hash of the previous
entry's fields.

## Domain separation

Audit link hashes prepend the `QLX_AUDIT_CHAIN_V1` domain tag before serializing
entry fields. This prevents an audit-link digest from being confused with other
protocol hashes that may contain similar IDs, addresses, amounts, or timestamps.

## Verification

Use `verify_audit_chain(env, invoice_id)` to return a boolean for healthy versus
divergent chains. Use `first_audit_chain_divergence(env, invoice_id)` for an
admin/debug tool that returns the zero-based first divergent entry index.

The verifier detects:

- missing entries referenced from an invoice audit trail;
- tampering with an entry that changes the next entry's expected `prev_hash`;
- malformed entries that fail the existing integrity predicate; and
- broken genesis links on the first entry.

## Edge cases

- **Empty chain**: valid; there is no evidence to verify and no divergence.
- **Single entry**: valid when `prev_hash` equals the fixed genesis sentinel.
- **Tampered middle**: invalid at the first successor whose stored `prev_hash` no
  longer matches the recomputed hash of the tampered predecessor.

## Config-change entries

Admin configuration changes are recorded on a dedicated virtual trail keyed by
`CONFIG_AUDIT_SENTINEL` (`[0xCFu8; 32]`), distinct from the genesis sentinel
(`[0u8; 32]`) used for per-invoice trails. All five admin config functions append
to this single shared trail so the entire configuration changelog forms one
tamper-evident chain.

### Operations and tags

| Operation                        | Tag | Function                        |
|----------------------------------|-----|---------------------------------|
| `ConfigProtocolChanged`          |  16 | `set_protocol_config`           |
| `ConfigFeeChanged`               |  17 | `set_fee_config`                |
| `ConfigTreasuryChanged`          |  18 | `set_treasury`                  |
| `ConfigFeeStructureChanged`      |  19 | `update_fee_structure`          |
| `ConfigRevenueDistributionChanged` | 20 | `configure_revenue_distribution` |

### Entry shape

| Field             | Value                                                                 |
|-------------------|-----------------------------------------------------------------------|
| `invoice_id`      | `CONFIG_AUDIT_SENTINEL` = `[0xCF; 32]` (all bytes `0xCF`)            |
| `operation`       | One of the five `Config*` variants above                              |
| `actor`           | Admin `Address` that authorized the change                            |
| `old_value`       | Serialized previous config value (see format below); `None` on first set |
| `new_value`       | Serialized new config value (always `Some`)                           |
| `amount`          | Always `None` (config changes are not monetary operations)            |
| `additional_data` | Parameter name: `"proto_cfg"`, `"fee_bps"`, `"treasury"`, fee-type label, or `"rev_dist"` |
| `prev_hash`       | SHA-256 of previous config-chain entry; genesis sentinel on first     |

### Serialization formats

- **`set_protocol_config`**: `"min_inv:{i128};max_days:{u64};grace:{u64}"`
  — e.g., `"min_inv:1000000;max_days:365;grace:604800"`

- **`set_fee_config`**: decimal u32 string — e.g., `"200"`

- **`set_treasury`**: first-18-byte XDR hex of the address — e.g., `"00000000..."`
  (36 hex characters; unique for any Stellar account or contract in practice)

- **`update_fee_structure`**: `"bps:{u32};min:{i128};max:{i128};active:{bool}"`
  — e.g., `"bps:200;min:100;max:1000000;active:true"`

- **`configure_revenue_distribution`**: `"t:{u32};d:{u32};p:{u32};min:{i128}"`
  — e.g., `"t:5000;d:3000;p:2000;min:0"` (treasury / developer / platform bps + min amount)

### Atomicity guarantee

`log_config_change` calls the infallible `log_operation` function. Soroban
transaction semantics guarantee that the storage write and the audit append either
both commit or both roll back — there is no partial-success scenario.

### Querying the config trail

```rust
// All config-change audit IDs (ordered chronologically):
let ids = get_invoice_audit_trail(env, BytesN::from_array(env, &CONFIG_AUDIT_SENTINEL));

// All protocol-config changes specifically:
let proto_ids = get_audit_entries_by_operation(env, AuditOperation::ConfigProtocolChanged);

// Verify the config changelog has not been tampered with:
let valid = verify_audit_chain(env, BytesN::from_array(env, &CONFIG_AUDIT_SENTINEL));
```

## Security note

The chain provides tamper evidence, not proof of who performed tampering. Evidence
quality depends on retaining historical entries and comparing the verifier result
against a trusted invoice ID. Any storage mutation, reorder, or deletion in the
middle of the trail becomes detectable by re-running the verifier.
