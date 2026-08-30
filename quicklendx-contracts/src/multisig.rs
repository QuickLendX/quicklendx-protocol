use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, BytesN, Env, Symbol, Vec,
};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum MultisigError {
    InvalidThreshold = 1,
    NotEnoughSignatures = 2,
    DuplicateSignature = 3,
    InvalidOwnerIndex = 4,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OwnerSignature {
    pub owner_index: u32,
    pub signature: BytesN<64>,
}

const OWNERS_KEY: Symbol = symbol_short!("owners");
const THRESHOLD_KEY: Symbol = symbol_short!("thresh");

#[cfg(not(target_arch = "wasm32"))]
#[contract]
pub struct MultisigContract;

#[cfg(not(target_arch = "wasm32"))]
#[contractimpl]
impl MultisigContract {
    pub fn initialize(
        env: Env,
        owners: Vec<BytesN<32>>,
        threshold: u32,
    ) -> Result<(), MultisigError> {
        let n = owners.len();
        // Quorum threshold must be in range [1, N-1]
        // This implicitly requires N >= 2, because if N <= 1, threshold >= N will always hold
        // since threshold must be >= 1.
        if threshold < 1 || threshold >= n {
            return Err(MultisigError::InvalidThreshold);
        }

        env.storage().instance().set(&OWNERS_KEY, &owners);
        env.storage().instance().set(&THRESHOLD_KEY, &threshold);

        Ok(())
    }

    pub fn verify_op(
        env: Env,
        message_hash: BytesN<32>,
        signatures: Vec<OwnerSignature>,
    ) -> Result<(), MultisigError> {
        let threshold: u32 = env
            .storage()
            .instance()
            .get(&THRESHOLD_KEY)
            .ok_or(MultisigError::InvalidThreshold)?;

        if signatures.len() < threshold {
            return Err(MultisigError::NotEnoughSignatures);
        }

        let owners: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&OWNERS_KEY)
            .ok_or(MultisigError::InvalidOwnerIndex)?;

        let mut seen_owners = Vec::new(&env);

        for sig in signatures.iter() {
            let index = sig.owner_index;
            if index >= owners.len() {
                return Err(MultisigError::InvalidOwnerIndex);
            }
            if seen_owners.contains(index) {
                return Err(MultisigError::DuplicateSignature);
            }
            seen_owners.push_back(index);

            let public_key = owners.get(index).unwrap();
            // Cryptographically verify signature
            env.crypto()
                .ed25519_verify(&public_key, &message_hash.clone().into(), &sig.signature);
        }

        Ok(())
    }
}
