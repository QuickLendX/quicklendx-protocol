use soroban_sdk::{contracttype, Address, BytesN, Env, symbol_short,vec};
use crate::errors::QuickLendXError;
use soroban_sdk::{Symbol,IntoVal,TryFromVal};
use soroban_sdk::token;

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

#[contracttype]
#[derive(Clone,Debug,Eq,PartialEq)]
pub struct TokenMetadata{
    pub symbol:BytesN<12>,
    pub decimals:u32,
    pub is_whitelisted: bool,
    pub fee_bps: u32,
}
pub struct CurrencyRegistry;
impl CurrencyRegistry{
    pub fn set_token(env:&Env,token:&Address,metadata:&TokenMetadata)->Option<TokenMetadata>{
        let storage=env.storage().persistent();
        let prev=storage.get(&token);
        storage.set(token,metadata);
        prev
    }
    pub fn get_token(env:&Env,token:&Address)->Option<TokenMetadata>{
        env.storage().instance().get(token)
    }
    pub fn validate_token(env:&Env,token:&Address)->Result<TokenMetadata,QuickLendXError>{
        let metadata=Self::get_token(env,token).ok_or(QuickLendXError::UnsupportedToken)?;
        if !metadata.is_whitelisted{
            return Err(QuickLendXError::UnsupportedToken);
        }
        Ok(metadata)
    }
}
pub struct Client<'a>{
    env:&'a Env,
    contract_id:Address,
}
impl <'a>Client<'a>{
    
    pub fn new(env: &'a Env,contract_id:Address)->Self{
        Self{env,contract_id}
    }
    pub fn transfer(
        &self,
        from:&Address,
        to:&Address,
        amount: &i128,
     ) ->  Result<bool,QuickLendXError>{
        let client=token::Client::new(self.env,&self.contract_id);
        client.transfer(from,to,amount);
            Ok(true)
            
    }
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
    CurrencyRegistry::validate_token(env,currency)?;
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
    // Transfer funds from escrow to business
    transfer_funds(env, &escrow.currency,&escrow.investor, &escrow.business, escrow.amount)?;
    // if transfer_success.is_err() {
    //     return Err(QuickLendXError::InsufficientFunds);
    // }
    //transfer_funds(env,&escrow.currency, &escrow.investor, &escrow.business, escrow.amount)?;

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
    //transfer_funds(env, &escrow.currency, &escrow.business, &escrow.investor, escrow.amount)?;
    // Refund funds to investor
    transfer_funds(env,&escrow.currency,&escrow.business, &escrow.investor, escrow.amount)?;
    // if transfer_success.is_err() {
    //     return Err(QuickLendXError::InsufficientFunds);
    // }
    // Update escrow status
    escrow.status = EscrowStatus::Refunded;
    EscrowStorage::update_escrow(env, &escrow);

    Ok(())
}
pub fn native_xlm_address(env:&Env)->Address{
    //let zero_bytes=BytesN::from_array(env,&[0u8;32]);
    env.current_contract_address()
}
/// Transfer funds between addresses
/// TODO: Integrate with Soroban payment primitives for XLM/USDC
/// For now, this is a stub that always returns true
/// Replace with actual payment logic when implementing token transfers
pub fn transfer_funds(env: &Env,currency: &Address,from: &Address, to: &Address, amount: i128) -> Result<(),QuickLendXError> {
    // Placeholder for actual token transfer implementation
    // This should integrate with Soroban's token interface
    // Example implementation would involve:
    // 1. Get token contract instance
    // 2. Call transfer method on token contract
    // 3. Handle success/failure appropriately
    // if currency==&native_xlm_address(env){
    //     let payment_success=env.invoke_contract(
    //         &contract_id,
    //             amount.into_val(env),
    //         ];
    //         // let args_vec=soroban_sdk::Vec::from_array(env,args);
    //         // env.invoke_contract(&contract_id,&method_name,args_vec);
    //     ).is_ok();
    //     if payment_success{
    //         return true
    //     } else{
    //         Err(QuickLendXError::PaymentFailed)
    //     }
    // } else{
    let metadata=CurrencyRegistry::validate_token(env,currency)?;
    let fee_amount=amount*(metadata.fee_bps as i128)/10_000;
    let amount_after_fee=amount-fee_amount;
    let client=token::Client::new(env,currency);
    client.transfer(from,to,&amount_after_fee);
        
    
    // if fee_amount>0{
    //     let treasury=env.current_contract_address();
    //         client
    //     .transfer(from,&treasury,&fee_amount)
    //     .map_err(|_| QuickLendXError::PaymentFailed)?;
    // }
    Ok(())
    //let client=token::Client::new(env,currency.clone());
    // client
    //     .transfer(from,to,&amount_after_fee)
    //     .map_err(|_| QuickLendXError::PaymentFailed)
}
   