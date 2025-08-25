use soroban_sdk::{contracttype, Address, BytesN, Env, symbol_short};
use crate::errors::QuickLendXError;
use soroban_token_sdk::TokenClient;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Held,      // Funds are held in escrow
    Released,  // Funds released to business
    Refunded,  // Funds refunded to investor
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
        env.storage().instance().set(&(symbol_short!("escrow"), &escrow.invoice_id), &escrow.escrow_id);
    }

    pub fn get_escrow(env: &Env, escrow_id: &BytesN<32>) -> Option<Escrow> {
        env.storage().instance().get(escrow_id)
    }

    pub fn get_escrow_by_invoice(env: &Env, invoice_id: &BytesN<32>) -> Option<Escrow> {
        let escrow_id: Option<BytesN<32>> = env.storage().instance().get(&(symbol_short!("escrow"), invoice_id));
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
pub fn release_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError> {
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
pub fn refund_escrow(
    env: &Env,
    invoice_id: &BytesN<32>,
) -> Result<(), QuickLendXError> {
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



pub fn transfer_funds(env: &Env, from: &Address, to: &Address, amount: i128) -> bool {

    if amount <= 0 {
        return false;
    }
    if from == to {
        return false;
    }

    if let Err(_) = from.require_auth() {
        return false;
    }

    // Check if the transfer is for XLM (based on original logic)
    if to == &Address::from_account_id(&env.ledger().network_passphrase()) {
        let balance = env.ledger().account_balance(from).unwrap_or(0);
        if balance < amount {
            return false;
        }

        let result = env.invoke_contract::<()>(
            &env.ledger().network_passphrase(),
            &symbol_short!("transfer"),
            Vec::from_array(env, [from.clone().into(), to.clone().into(), amount.into()]),
        );
        return result.is_ok();
    } else {
        // NB: This is for Test ⛓️
        let usdc_contract_id = BytesN::from_array(
            env,
            &[
                0xCD, 0x7C, 0xF9, 0x83, 0x1E, 0xF8, 0xA2, 0xE8,
                0x9F, 0x16, 0xB8, 0xC3, 0xB6, 0x62, 0xA6, 0xD0,
                0xB9, 0xB2, 0xD2, 0xF4, 0xB9, 0xD6, 0xF8, 0xC7,
                0xF7, 0xC0, 0x83, 0x6C, 0x0F, 0x2B, 0x6F, 0xA5,
            ],
        );

        let token_client = TokenClient::new(env, &usdc_contract_id);

        let balance = token_client.balance(from);
        if balance < amount {
            return false;
        }

        let result = token_client.transfer(from, to, &amount);
        return result.is_ok();
    }
}