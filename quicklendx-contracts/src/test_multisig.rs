#![cfg(test)]

use super::multisig::{
    MultisigContract, MultisigContractClient, MultisigError, OwnerSignature,
};
use ed25519_dalek::{Signer, SigningKey};
use soroban_sdk::{BytesN, Env, Vec};

fn generate_keypair(env: &Env, seed_byte: u8) -> (BytesN<32>, SigningKey) {
    let mut seed = [0u8; 32];
    seed[0] = seed_byte;
    let signing_key = SigningKey::from_bytes(&seed);
    let verifying_key = signing_key.verifying_key();
    let pubkey = BytesN::from_array(env, &verifying_key.to_bytes());
    (pubkey, signing_key)
}

#[test]
fn test_initialize_boundary_checks() {
    let env = Env::default();
    let contract_id = env.register(MultisigContract, ());
    let client = MultisigContractClient::new(&env, &contract_id);

    // Create 3 owners (N = 3)
    let (owner0, _) = generate_keypair(&env, 1);
    let (owner1, _) = generate_keypair(&env, 2);
    let (owner2, _) = generate_keypair(&env, 3);

    let mut owners = Vec::new(&env);
    owners.push_back(owner0);
    owners.push_back(owner1);
    owners.push_back(owner2);

    // Boundary conditions: threshold range is [1, N-1] = [1, 2]
    
    // Lower bound: 1 (succeeds)
    let res = client.try_initialize(&owners, &1);
    assert!(res.is_ok());

    // Upper bound: N - 1 = 2 (succeeds)
    let res = client.try_initialize(&owners, &2);
    assert!(res.is_ok());

    // Boundary: N = 3 (fails)
    let res = client.try_initialize(&owners, &3);
    assert_eq!(res.unwrap_err().ok(), Some(MultisigError::InvalidThreshold));

    // Boundary: N + 1 = 4 (fails)
    let res = client.try_initialize(&owners, &4);
    assert_eq!(res.unwrap_err().ok(), Some(MultisigError::InvalidThreshold));

    // Below lower bound: 0 (fails)
    let res = client.try_initialize(&owners, &0);
    assert_eq!(res.unwrap_err().ok(), Some(MultisigError::InvalidThreshold));

    // Corner case: N = 1 owner
    let mut single_owner = Vec::new(&env);
    let (owner_single, _) = generate_keypair(&env, 4);
    single_owner.push_back(owner_single);

    // Threshold = 1 (fails, since range [1, N-1] = [1, 0] is empty)
    let res = client.try_initialize(&single_owner, &1);
    assert_eq!(res.unwrap_err().ok(), Some(MultisigError::InvalidThreshold));
}

#[test]
fn test_verify_op_success() {
    let env = Env::default();
    let contract_id = env.register(MultisigContract, ());
    let client = MultisigContractClient::new(&env, &contract_id);

    // Generate keys for N = 3 owners
    let (pub0, priv0) = generate_keypair(&env, 1);
    let (pub1, _priv1) = generate_keypair(&env, 2);
    let (pub2, priv2) = generate_keypair(&env, 3);

    let mut owners = Vec::new(&env);
    owners.push_back(pub0);
    owners.push_back(pub1);
    owners.push_back(pub2);

    // Initialize with threshold = 2
    client.initialize(&owners, &2);

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

    // Verification should succeed
    let res = client.try_verify_op(&message_hash, &signatures);
    assert!(res.is_ok());
}

#[test]
fn test_verify_op_insufficient_signatures() {
    let env = Env::default();
    let contract_id = env.register(MultisigContract, ());
    let client = MultisigContractClient::new(&env, &contract_id);

    let (pub0, priv0) = generate_keypair(&env, 1);
    let (pub1, _priv1) = generate_keypair(&env, 2);

    let mut owners = Vec::new(&env);
    owners.push_back(pub0);
    owners.push_back(pub1);

    client.initialize(&owners, &1); // threshold = 1

    let message_hash = BytesN::from_array(&env, &[9u8; 32]);

    // Send 0 signatures (threshold is 1)
    let signatures = Vec::new(&env);
    let res = client.try_verify_op(&message_hash, &signatures);
    assert_eq!(res.unwrap_err().ok(), Some(MultisigError::NotEnoughSignatures));
}

#[test]
fn test_verify_op_duplicate_signatures() {
    let env = Env::default();
    let contract_id = env.register(MultisigContract, ());
    let client = MultisigContractClient::new(&env, &contract_id);

    let (pub0, priv0) = generate_keypair(&env, 1);
    let (pub1, _priv1) = generate_keypair(&env, 2);
    let (pub2, _priv2) = generate_keypair(&env, 3);

    let mut owners = Vec::new(&env);
    owners.push_back(pub0);
    owners.push_back(pub1);
    owners.push_back(pub2);

    client.initialize(&owners, &2); // threshold = 2

    let message_hash = BytesN::from_array(&env, &[9u8; 32]);
    let message_bytes: [u8; 32] = [9u8; 32];

    // Owner 0 signs twice
    let sig0_bytes = priv0.sign(&message_bytes).to_bytes();
    let sig0 = BytesN::from_array(&env, &sig0_bytes);

    let mut signatures = Vec::new(&env);
    signatures.push_back(OwnerSignature {
        owner_index: 0,
        signature: sig0.clone(),
    });
    signatures.push_back(OwnerSignature {
        owner_index: 0,
        signature: sig0,
    });

    let res = client.try_verify_op(&message_hash, &signatures);
    assert_eq!(res.unwrap_err().ok(), Some(MultisigError::DuplicateSignature));
}

#[test]
fn test_verify_op_invalid_owner_index() {
    let env = Env::default();
    let contract_id = env.register(MultisigContract, ());
    let client = MultisigContractClient::new(&env, &contract_id);

    let (pub0, priv0) = generate_keypair(&env, 1);
    let (pub1, _priv1) = generate_keypair(&env, 2);

    let mut owners = Vec::new(&env);
    owners.push_back(pub0);
    owners.push_back(pub1);

    client.initialize(&owners, &1);

    let message_hash = BytesN::from_array(&env, &[9u8; 32]);
    let message_bytes: [u8; 32] = [9u8; 32];

    let sig0_bytes = priv0.sign(&message_bytes).to_bytes();
    let sig0 = BytesN::from_array(&env, &sig0_bytes);

    // Use out-of-bounds owner index 2 (N = 2 owners)
    let mut signatures = Vec::new(&env);
    signatures.push_back(OwnerSignature {
        owner_index: 2,
        signature: sig0,
    });

    let res = client.try_verify_op(&message_hash, &signatures);
    assert_eq!(res.unwrap_err().ok(), Some(MultisigError::InvalidOwnerIndex));
}

#[test]
#[should_panic(expected = "HostError: Error(Crypto, InvalidHash)")]
fn test_verify_op_invalid_signature() {
    let env = Env::default();
    let contract_id = env.register(MultisigContract, ());
    let client = MultisigContractClient::new(&env, &contract_id);

    let (pub0, priv0) = generate_keypair(&env, 1);
    let (pub1, _priv1) = generate_keypair(&env, 2);

    let mut owners = Vec::new(&env);
    owners.push_back(pub0);
    owners.push_back(pub1);

    client.initialize(&owners, &1);

    let message_hash = BytesN::from_array(&env, &[9u8; 32]);
    let wrong_message_bytes: [u8; 32] = [0u8; 32]; // Sign a different message

    let sig0_bytes = priv0.sign(&wrong_message_bytes).to_bytes();
    let sig0 = BytesN::from_array(&env, &sig0_bytes);

    let mut signatures = Vec::new(&env);
    signatures.push_back(OwnerSignature {
        owner_index: 0,
        signature: sig0,
    });

    // Verification must fail cryptographically (panics with HostError)
    client.verify_op(&message_hash, &signatures);
}

#[test]
fn test_verify_op_over_quorum() {
    let env = Env::default();
    let contract_id = env.register(MultisigContract, ());
    let client = MultisigContractClient::new(&env, &contract_id);

    // Generate keys for N = 3 owners
    let (pub0, priv0) = generate_keypair(&env, 1);
    let (pub1, priv1) = generate_keypair(&env, 2);
    let (pub2, priv2) = generate_keypair(&env, 3);

    let mut owners = Vec::new(&env);
    owners.push_back(pub0);
    owners.push_back(pub1);
    owners.push_back(pub2);

    // Initialize with threshold = 2
    client.initialize(&owners, &2);

    let message_hash = BytesN::from_array(&env, &[9u8; 32]);
    let message_bytes: [u8; 32] = [9u8; 32];

    // All 3 owners sign the message (over quorum: 3 > threshold 2)
    let sig0_bytes = priv0.sign(&message_bytes).to_bytes();
    let sig1_bytes = priv1.sign(&message_bytes).to_bytes();
    let sig2_bytes = priv2.sign(&message_bytes).to_bytes();

    let sig0 = BytesN::from_array(&env, &sig0_bytes);
    let sig1 = BytesN::from_array(&env, &sig1_bytes);
    let sig2 = BytesN::from_array(&env, &sig2_bytes);

    let mut signatures = Vec::new(&env);
    signatures.push_back(OwnerSignature {
        owner_index: 0,
        signature: sig0,
    });
    signatures.push_back(OwnerSignature {
        owner_index: 1,
        signature: sig1,
    });
    signatures.push_back(OwnerSignature {
        owner_index: 2,
        signature: sig2,
    });

    // Verification should succeed even with more signatures than threshold
    let res = client.try_verify_op(&message_hash, &signatures);
    assert!(res.is_ok());
}
