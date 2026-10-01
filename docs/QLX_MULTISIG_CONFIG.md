# QLX Multisig Configuration

> **Audience:** Protocol operators — the people who deploy, configure, and manage QuickLendX Soroban contracts.

This document covers the on-chain multisig contract: how to initialize it, rotate signers, and use it to authorize critical operations. It complements the [Operator Handbook](OPERATOR_HANDBOOK.md) and the [Emergency Recovery](contracts/emergency-recovery.md) guide.

## Overview

The `MultisigContract` (`quicklendx-contracts/src/multisig.rs`) provides a generic ed25519 threshold-signature verifier. It stores a set of owner public keys and a quorum threshold in contract instance storage. Any off-chain or on-chain caller can submit a batch of signatures and have them verified against the stored owner set.

| Concept | Detail |
|---------|--------|
| Owners | Ed25519 public keys (`BytesN<32>`), stored as a `Vec` |
| Threshold | Minimum number of distinct signatures required (`u32`, range `[1, N-1]`) |
| Signature scheme | Ed25519, 64-byte signatures |
| Storage keys | `owners` (`OWNERS_KEY`), `thresh` (`THRESHOLD_KEY`) |

## Error Codes

| Error | Code | When it fires |
|-------|------|---------------|
| `InvalidThreshold` | 1 | Threshold is `< 1` or `>= N` (number of owners) |
| `NotEnoughSignatures` | 2 | Fewer signatures submitted than the threshold |
| `DuplicateSignature` | 3 | The same owner index appears more than once in a single request |
| `InvalidOwnerIndex` | 4 | An `owner_index` in the signature batch is `>= N` |

## 1. Initialize the Multisig

Call `initialize` with the list of owner public keys and the quorum threshold. This is a one-time setup entrypoint; re-initialization overwrites the previous owner set and threshold.

**Rust entrypoint signature:**

```rust
pub fn initialize(
    env: Env,
    owners: Vec<BytesN<32>>,
    threshold: u32,
) -> Result<(), MultisigError>;
```

**Constraints:**

- `owners.len()` must be `>= 2` (a single-owner setup is not permitted because `threshold >= 1` and `threshold < N` would be unsatisfiable with `N = 1`).
- `threshold` must be in `[1, N-1]`.

### Concrete example — 3 owners, threshold 2

```rust
use soroban_sdk::{BytesN, Env, Vec};
use quicklendx_contracts::multisig::{MultisigContract, MultisigContractClient};

let env = Env::default();
let contract_id = env.register(MultisigContract, ());
let client = MultisigContractClient::new(&env, &contract_id);

// Generate three Ed25519 keypairs
let (owner0_pub, _priv0) = generate_keypair(&env, 1);
let (owner1_pub, _priv1) = generate_keypair(&env, 2);
let (owner2_pub, _priv2) = generate_keypair(&env, 3);

let mut owners = Vec::new(&env);
owners.push_back(owner0_pub);
owners.push_back(owner1_pub);
owners.push_back(owner2_pub);

// threshold = 2: any 2 of 3 owners must sign
let res = client.initialize(&owners, &2);
assert!(res.is_ok());
```

### Boundary checks (tested in `test_multisig.rs`)

| Scenario | Result |
|----------|--------|
| `threshold = 0` | `Err(InvalidThreshold)` |
| `threshold = 1`, `N = 3` | Ok |
| `threshold = N - 1`, `N = 3` | Ok |
| `threshold = N` | `Err(InvalidThreshold)` |
| `threshold = N + 1` | `Err(InvalidThreshold)` |
| `N = 1`, `threshold = 1` | `Err(InvalidThreshold)` |

## 2. Rotate Signers

To rotate an owner key or change the threshold, call `initialize` again with the updated owner list and threshold. Because `initialize` overwrites the stored state atomically, rotation is a single on-chain transaction.

### Operator rotation workflow

1. **Prepare the new owner list.** Gather the Ed25519 public keys of all new signers. Remove the compromised or departed signer's key; add the replacement key at the same or a new index.
2. **Choose a new threshold** if needed. The new threshold must still satisfy `1 <= threshold < N` where `N` is the new owner count.
3. **Execute the rotation transaction.** Call `initialize` from the admin account.

```rust
// Rotation: replace owner index 1 with a new key, keep threshold = 2
let (new_owner1_pub, _new_priv1) = generate_keypair(&env, 10);

let mut new_owners = Vec::new(&env);
new_owners.push_back(owner0_pub);      // unchanged
new_owners.push_back(new_owner1_pub);  // replaced
new_owners.push_back(owner2_pub);      // unchanged

let res = client.initialize(&new_owners, &2);
assert!(res.is_ok());
```

4. **Verify the rotation.** Re-initialize a client and confirm the new threshold and owner count.

```rust
let stored_owners: Vec<BytesN<32>> = env
    .storage()
    .instance()
    .get(&OWNERS_KEY)
    .unwrap();
assert_eq!(stored_owners.len(), 3);
```

> **See also:** The treasury rotation pattern used in [QLX_TREASURY_ROTATION.md](QLX_TREASURY_ROTATION.md) applies the same two-step principles (initiate, then confirm) but with a timelock. Multisig rotation does **not** use a timelock — it overwrites state immediately. If your deployment requires a delay, wrap the `initialize` call in a governance proposal or timelock contract.

## 3. Verify a Multisig Signature (verify_op)

After initialization, any caller can submit a message hash and a batch of `OwnerSignature` entries. The contract checks that the number of signatures meets the threshold, that each `owner_index` is valid and non-duplicate, and that each signature is cryptographically valid against the corresponding owner's public key.

**Rust entrypoint signature:**

```rust
pub fn verify_op(
    env: Env,
    message_hash: BytesN<32>,
    signatures: Vec<OwnerSignature>,
) -> Result<(), MultisigError>;
```

**`OwnerSignature` struct:**

```rust
pub struct OwnerSignature {
    pub owner_index: u32,    // index into the owners Vec
    pub signature: BytesN<64>, // ed25519 signature bytes
}
```

### Concrete example — 2 of 3 owners sign a message hash

```rust
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{BytesN, Env, Vec};
use quicklendx_contracts::multisig::{MultisigContract, MultisigContractClient, OwnerSignature};

let env = Env::default();
let contract_id = env.register(MultisigContract, ());
let client = MultisigContractClient::new(&env, &contract_id);

let (pub0, priv0) = generate_keypair(&env, 1);
let (pub1, _priv1) = generate_keypair(&env, 2);
let (pub2, priv2) = generate_keypair(&env, 3);

let mut owners = Vec::new(&env);
owners.push_back(pub0);
owners.push_back(pub1);
owners.push_back(pub2);

client.initialize(&owners, &2);

// The message hash to sign (32 bytes)
let message_hash = BytesN::from_array(&env, &[9u8; 32]);
let message_bytes: [u8; 32] = [9u8; 32];

// Owners 0 and 2 sign the message
let sig0_bytes = priv0.sign(&message_bytes).to_bytes();
let sig2_bytes = priv2.sign(&message_bytes).to_bytes();

let sig0 = BytesN::from_array(&env, &sig0_bytes);
let sig2 = BytesN::from_array(&env, &sig2_bytes);

let mut signatures = Vec::new(&env);
signatures.push_back(OwnerSignature {
    owner_index: 0,
    signature: sig0,
});
signatures.push_back(OwnerSignature {
    owner_index: 2,
    signature: sig2,
});

let res = client.verify_op(&message_hash, &signatures);
assert!(res.is_ok());
```

### Common failure scenarios

| Scenario | Error returned |
|----------|---------------|
| Submit 1 signature when threshold = 2 | `NotEnoughSignatures` |
| Submit the same owner index twice | `DuplicateSignature` |
| Use `owner_index = N` (out of bounds) | `InvalidOwnerIndex` |
| Sign the wrong message (mismatched hash) | Panics with `HostError: Error(Crypto, InvalidHash)` |
| Submit a signature for an owner not in the current set | `InvalidOwnerIndex` (if index >= N) |

## 4. Using Multisig in Critical Operations

The multisig contract is designed to be called by other contracts or by off-chain services that need threshold authorization for sensitive protocol actions. Typical patterns include:

1. **Pre-authorization:** An admin builds a `message_hash` from the operation parameters (e.g., `sha256(treasury_address + amount + nonce)`), collects `threshold` signatures from the owner set off-chain, then calls `verify_op` on-chain before executing the action.
2. **On-chain composability:** A governance or timelock contract calls `verify_op` as a sub-step before mutating state, ensuring that no single signer can unilaterally trigger critical changes.
3. **Rotation safety:** Because `initialize` overwrites the entire owner set atomically, rotate signers in a single transaction rather than incremental updates to avoid intermediate states with reduced security.

> **See also:**
> - [Emergency Recovery](contracts/emergency-recovery.md) — describes where multisig fits into the incident-response path
> - [QLX_TREASURY_ROTATION.md](QLX_TREASURY_ROTATION.md) — example of a protected rotation with timelock
> - [Security](contracts/security.md) — reentrancy guard and pause circuit breaker that complement multisig controls
> - [OPERATOR_HANDBOOK.md](OPERATOR_HANDBOOK.md) — full CLI reference for on-chain operations

## 5. Quick Reference — CLI Example

```bash
# Initialize multisig with 3 owners, threshold 2
soroban contract invoke \
  --id $CONTRACT_ID \
  --source admin \
  --network testnet \
  -- \
  initialize \
  --owners "$OWNER0,$OWNER1,$OWNER2" \
  --threshold 2

# Verify a set of signatures against a message hash
soroban contract invoke \
  --id $CONTRACT_ID \
  --network testnet \
  -- \
  verify_op \
  --message_hash "$MESSAGE_HASH" \
  --signatures "$SIGNATURES_JSON"
```