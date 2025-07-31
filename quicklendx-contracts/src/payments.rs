use crate::errors::QuickLendXError;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Held,     // Funds are held in escrow
    Released, // Funds released to business
    Refunded, // Funds refunded to investor
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Escrow {
    pub escrow_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub business: Address,
    pub amount: i128,
    pub currency: Address,
    pub created_at: u64,
    pub status: EscrowStatus,
}

pub struct EscrowStorage;

impl EscrowStorage {
    pub fn store_escrow(env: &Env, escrow: &Escrow) {
        env.storage().instance().set(&escrow.escrow_id, escrow);
        // Also store by invoice_id for easy lookup
        env.storage().instance().set(
            &(symbol_short!("escrow"), &escrow.invoice_id),
            &escrow.escrow_id,
        );
    }

    pub fn get_escrow(env: &Env, escrow_id: &BytesN<32>) -> Option<Escrow> {
        env.storage().instance().get(escrow_id)
    }

    pub fn get_escrow_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Escrow> {
        let escrow_id: Option<BytesN<32>> = env
            .storage()
            .instance()
            .get(&(symbol_short!("escrow"), invoice_id));
        if let Some(id) = escrow_id {
            Self::get_escrow(env, &id)
        } else {
            None
        }
    }

    pub fn update_escrow(env: &Env, escrow: &Escrow) {
        env.storage().instance().set(&escrow.escrow_id, escrow);
    }

    pub fn generate_unique_escrow_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let counter_key = symbol_short!("esc_cnt");
        let counter: u64 = env.storage().instance().get(&counter_key).unwrap_or(0u64);
        env.storage().instance().set(&counter_key, &(counter + 1));

        let mut id_bytes = [0u8; 32];
        // Add escrow prefix to distinguish from other entity types
        id_bytes[0] = 0xE5; // 'E' for Escrow
        id_bytes[1] = 0xC0; // 'C' for sCrow
                            // Embed timestamp in next 8 bytes
        id_bytes[2..10].copy_from_slice(&timestamp.to_be_bytes());
        // Embed counter in next 8 bytes
        id_bytes[10..18].copy_from_slice(&counter.to_be_bytes());
        // Fill remaining bytes with a pattern to ensure uniqueness
        for i in 18..32 {
            id_bytes[i] = ((timestamp + counter + 0xE5C0) % 256) as u8;
        }

        BytesN::from_array(env, &id_bytes)
    }
}

/// Create escrow when bid is accepted
pub fn create_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
    investor: &Address,
    business: &Address,
    amount: i128,
    currency: &Address,
) -> Result<BytesN<32>, QuickLendXError> {
    let escrow_id = EscrowStorage::generate_unique_escrow_id(env);
    let escrow = Escrow {
        escrow_id: escrow_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        business: business.clone(),
        amount,
        currency: currency.clone(),
        created_at: env.ledger().timestamp(),
        status: EscrowStatus::Held,
    };

    EscrowStorage::store_escrow(env, &escrow);
    Ok(escrow_id)
}

/// Release escrow funds to business upon invoice verification
pub fn release_escrow(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut escrow = EscrowStorage::get_escrow_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    if escrow.status != EscrowStatus::Held {
        return Err(QuickLendXError::InvalidStatus);
    }

    // Transfer funds from escrow to business
    let transfer_success = transfer_funds(env, &escrow.investor, &escrow.business, escrow.amount);
    if !transfer_success {
        return Err(QuickLendXError::InsufficientFunds);
    }

    // Update escrow status
    escrow.status = EscrowStatus::Released;
    EscrowStorage::update_escrow(env, &escrow);

    Ok(())
}

/// Refund escrow funds to investor if verification fails
pub fn refund_escrow(env: &Env, invoice_id: &BytesN<32>) -> Result<(), QuickLendXError> {
    let mut escrow = EscrowStorage::get_escrow_by_invoice(env, invoice_id)
        .ok_or(QuickLendXError::StorageKeyNotFound)?;

    if escrow.status != EscrowStatus::Held {
        return Err(QuickLendXError::InvalidStatus);
    }

    // Refund funds to investor
    let transfer_success = transfer_funds(env, &escrow.business, &escrow.investor, escrow.amount);
    if !transfer_success {
        return Err(QuickLendXError::InsufficientFunds);
    }

    // Update escrow status
    escrow.status = EscrowStatus::Refunded;
    EscrowStorage::update_escrow(env, &escrow);

    Ok(())
}

/// Transfer funds between addresses
/// TODO: Integrate with Soroban payment primitives for XLM/USDC
/// For now, this is a stub that always returns true
/// Replace with actual payment logic when implementing token transfers
pub fn transfer_funds(env: &Env, from: &Address, to: &Address, amount: i128) -> bool {
    // Placeholder for actual token transfer implementation
    // This should integrate with Soroban's token interface
    // Example implementation would involve:
    // 1. Get token contract instance
    // 2. Call transfer method on token contract
    // 3. Handle success/failure appropriately
    true
}
