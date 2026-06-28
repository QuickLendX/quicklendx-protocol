use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use soroban_sdk::{symbol_short, Address, Env, String, Symbol, Vec, vec};

const PAUSED_KEY: Symbol = symbol_short!("paused");
const PAUSED_AT_KEY: Symbol = symbol_short!("paused_at");
const MAX_PAUSE_DURATION: u64 = 7 * 24 * 3600;

fn guarded_entrypoints(env: &Env) -> Vec<String> {
    vec![
        env,
        String::from_str(env, "store_invoice"),
        String::from_str(env, "upload_invoice"),
        String::from_str(env, "accept_bid_and_fund"),
        String::from_str(env, "verify_invoice"),
        String::from_str(env, "cancel_invoice"),
        String::from_str(env, "place_bid"),
        String::from_str(env, "accept_bid"),
        String::from_str(env, "withdraw_bid"),
        String::from_str(env, "cancel_bid"),
        String::from_str(env, "settle_invoice"),
        String::from_str(env, "process_partial_payment"),
        String::from_str(env, "make_payment"),
        String::from_str(env, "release_escrow_funds"),
        String::from_str(env, "refund_escrow_funds"),
        String::from_str(env, "refund_escrow"),
        String::from_str(env, "withdraw_investment"),
        String::from_str(env, "add_investment_insurance"),
        String::from_str(env, "mark_invoice_defaulted"),
        String::from_str(env, "handle_default"),
        String::from_str(env, "create_dispute"),
        String::from_str(env, "update_dispute_evidence"),
        String::from_str(env, "put_dispute_under_review"),
        String::from_str(env, "resolve_dispute"),
        String::from_str(env, "resolve_dispute_structured"),
        String::from_str(env, "submit_kyc_application"),
        String::from_str(env, "submit_investor_kyc"),
        String::from_str(env, "verify_investor"),
        String::from_str(env, "verify_business"),
        String::from_str(env, "create_backup"),
        String::from_str(env, "restore_backup"),
        String::from_str(env, "create_vesting_schedule"),
        String::from_str(env, "release_vested_tokens"),
        String::from_str(env, "initiate_emergency_withdraw"),
        String::from_str(env, "execute_emergency_withdraw"),
        String::from_str(env, "add_currency"),
        String::from_str(env, "remove_currency"),
        String::from_str(env, "add_currencies_batch"),
        String::from_str(env, "remove_currencies_batch"),
        String::from_str(env, "set_currencies"),
        String::from_str(env, "clear_currencies"),
        String::from_str(env, "set_platform_fee"),
        String::from_str(env, "update_platform_fee_bps"),
        String::from_str(env, "configure_treasury"),
        String::from_str(env, "distribute_revenue"),
        String::from_str(env, "distribute_revenue_vested"),
        String::from_str(env, "update_invoice_category"),
        String::from_str(env, "add_invoice_tag"),
        String::from_str(env, "remove_invoice_tag"),
        String::from_str(env, "update_invoice_metadata"),
        String::from_str(env, "clear_invoice_metadata"),
        String::from_str(env, "clear_all_invoices"),
        String::from_str(env, "add_invoice_rating"),
    ]
}

pub struct PauseControl;

impl PauseControl {
    pub fn is_paused(env: &Env) -> bool {
        if !env.storage().instance().get(&PAUSED_KEY).unwrap_or(false) {
            return false;
        }
        let paused_at: u64 = env
            .storage()
            .instance()
            .get(&PAUSED_AT_KEY)
            .unwrap_or(0);
        if paused_at > 0 && env.ledger().timestamp() >= paused_at + MAX_PAUSE_DURATION {
            env.storage().instance().set(&PAUSED_KEY, &false);
            return false;
        }
        true
    }

    pub fn set_paused(
        env: &Env,
        admin: &Address,
        paused: bool,
    ) -> Result<(), QuickLendXError> {
        admin.require_auth();
        AdminStorage::require_admin(env, admin)?;
        let current: bool = Self::is_paused(env);
        if current == paused {
            return Ok(());
        }
        Self::apply_paused(env, paused);
        if paused {
            crate::events::emit_paused(env, admin);
        } else {
            crate::events::emit_unpaused(env, admin);
        }
        Ok(())
    }

    pub(crate) fn apply_paused(env: &Env, paused: bool) {
        env.storage().instance().set(&PAUSED_KEY, &paused);
        if paused {
            env.storage()
                .instance()
                .set(&PAUSED_AT_KEY, &env.ledger().timestamp());
        }
    }

    pub fn require_not_paused(env: &Env) -> Result<(), QuickLendXError> {
        if Self::is_paused(env) {
            return Err(QuickLendXError::ContractPaused);
        }
        Ok(())
    }

    pub fn is_entrypoint_paused(env: &Env, entrypoint: String) -> bool {
        if !Self::is_paused(env) {
            return false;
        }
        let guarded = guarded_entrypoints(env);
        for ep in guarded.iter() {
            if ep == entrypoint {
                return true;
            }
        }
        false
    }
}