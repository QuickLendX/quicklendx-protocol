//! Held-escrow reserve accumulator, its repair flow, and locked-value views.

use super::escrow::{EscrowStatus, EscrowStorage};
use crate::errors::QuickLendXError;
use crate::storage::{extend_persistent_ttl, InvoiceStorage};
use crate::types::RebuildReport;
use soroban_sdk::{contracttype, symbol_short, Address, BytesN, Env, Symbol, TryFromVal, Val, Vec};

#[contracttype]
#[derive(Clone, Eq, PartialEq)]
#[cfg_attr(test, derive(Debug))]
pub(super) struct HeldEscrowReserve {
    amount: i128,
    complete: bool,
    repair_next_offset: u32,
}

const HELD_ESCROW_RESERVE_KEY: Symbol = symbol_short!("esc_res");
const ESCROW_RESERVE_MARKER_KEY: Symbol = symbol_short!("esc_acc");
const HELD_RESERVE_REPAIR_IDS_KEY: Symbol = symbol_short!("esc_rids");
#[cfg(not(test))]
const MAX_REPAIR_SNAPSHOT_IDS: u64 = 1_000;
#[cfg(test)]
const MAX_REPAIR_SNAPSHOT_IDS: u64 = 3;

impl EscrowStorage {
    fn held_reserve_key(currency: &Address) -> (Symbol, Address) {
        (HELD_ESCROW_RESERVE_KEY.clone(), currency.clone())
    }

    fn reserve_marker_key(escrow_id: &BytesN<32>) -> (Symbol, BytesN<32>) {
        (ESCROW_RESERVE_MARKER_KEY.clone(), escrow_id.clone())
    }

    fn held_reserve_repair_ids_key(currency: &Address) -> (Symbol, Address) {
        (HELD_RESERVE_REPAIR_IDS_KEY.clone(), currency.clone())
    }

    fn empty_reserve() -> HeldEscrowReserve {
        HeldEscrowReserve {
            amount: 0,
            complete: false,
            repair_next_offset: 0,
        }
    }

    fn get_held_reserve_record(env: &Env, currency: &Address) -> Option<HeldEscrowReserve> {
        let key = Self::held_reserve_key(currency);
        let raw: Option<Val> = env.storage().persistent().get(&key);
        let raw = raw?;
        extend_persistent_ttl(env, &key);

        if let Ok(mut reserve) = HeldEscrowReserve::try_from_val(env, &raw) {
            if reserve.amount < 0 {
                reserve.amount = 0;
                reserve.complete = false;
                reserve.repair_next_offset = 0;
            }
            return Some(reserve);
        }

        i128::try_from_val(env, &raw)
            .ok()
            .map(|amount| HeldEscrowReserve {
                amount: amount.max(0),
                complete: false,
                repair_next_offset: 0,
            })
    }

    pub fn get_held_reserve(env: &Env, currency: &Address) -> i128 {
        Self::get_held_reserve_record(env, currency)
            .map(|reserve| reserve.amount)
            .unwrap_or(0)
    }

    pub fn is_held_reserve_complete(env: &Env, currency: &Address) -> bool {
        Self::get_held_reserve_record(env, currency)
            .map(|reserve| reserve.complete)
            .unwrap_or(false)
    }

    pub(crate) fn require_no_active_reserve_repair(
        env: &Env,
        currency: &Address,
    ) -> Result<(), QuickLendXError> {
        let repair_in_progress = Self::get_held_reserve_record(env, currency)
            .map(|reserve| !reserve.complete && reserve.repair_next_offset != 0)
            .unwrap_or(false);
        if repair_in_progress {
            return Err(QuickLendXError::InvalidStatus);
        }
        Ok(())
    }

    pub(super) fn set_held_reserve_record(
        env: &Env,
        currency: &Address,
        reserve: &HeldEscrowReserve,
    ) {
        let key = Self::held_reserve_key(currency);
        env.storage().persistent().set(&key, reserve);
        extend_persistent_ttl(env, &key);
    }

    fn set_repair_snapshot(env: &Env, currency: &Address, ids: &Vec<BytesN<32>>) {
        let key = Self::held_reserve_repair_ids_key(currency);
        env.storage().persistent().set(&key, ids);
        extend_persistent_ttl(env, &key);
    }

    fn get_repair_snapshot(env: &Env, currency: &Address) -> Option<Vec<BytesN<32>>> {
        let key = Self::held_reserve_repair_ids_key(currency);
        let ids: Option<Vec<BytesN<32>>> = env.storage().persistent().get(&key);
        if ids.is_some() {
            extend_persistent_ttl(env, &key);
        }
        ids
    }

    fn clear_repair_snapshot(env: &Env, currency: &Address) {
        let key = Self::held_reserve_repair_ids_key(currency);
        env.storage().persistent().remove(&key);
    }

    pub(super) fn held_reserve_after_increase(
        env: &Env,
        currency: &Address,
        amount: i128,
    ) -> Result<HeldEscrowReserve, QuickLendXError> {
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let mut reserve =
            Self::get_held_reserve_record(env, currency).unwrap_or_else(Self::empty_reserve);
        // Preserve `complete` as-is. Missing/legacy reserve state remains incomplete
        // and therefore emergency withdrawal stays fail-closed until repair completes.
        reserve.amount = reserve
            .amount
            .checked_add(amount)
            .ok_or(QuickLendXError::ArithmeticOverflow)?;
        Ok(reserve)
    }

    pub(super) fn held_reserve_after_decrease(
        env: &Env,
        currency: &Address,
        amount: i128,
    ) -> Result<HeldEscrowReserve, QuickLendXError> {
        if amount <= 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let mut reserve =
            Self::get_held_reserve_record(env, currency).unwrap_or_else(Self::empty_reserve);
        if reserve.amount < amount {
            // If the reserve undercounts the escrow amount, dirty the reserve instead of
            // blocking user release/refund. Emergency withdrawal remains fail-closed.
            reserve.amount = 0;
            reserve.complete = false;
            reserve.repair_next_offset = 0;
            return Ok(reserve);
        }

        // `reserve.amount >= amount` is guaranteed here, so this cannot underflow;
        // `checked_sub` is used to make the invariant explicit and panic-safe.
        reserve.amount = reserve
            .amount
            .checked_sub(amount)
            .ok_or(QuickLendXError::ArithmeticOverflow)?;
        Ok(reserve)
    }

    pub(super) fn mark_reserve_accounted(env: &Env, escrow_id: &BytesN<32>) {
        let key = Self::reserve_marker_key(escrow_id);
        env.storage().persistent().set(&key, &true);
        extend_persistent_ttl(env, &key);
    }

    pub(super) fn is_reserve_accounted(env: &Env, escrow_id: &BytesN<32>) -> bool {
        let key = Self::reserve_marker_key(escrow_id);
        let accounted: bool = env.storage().persistent().get(&key).unwrap_or(false);
        if accounted {
            extend_persistent_ttl(env, &key);
        }
        accounted
    }

    pub(super) fn clear_reserve_accounted(env: &Env, escrow_id: &BytesN<32>) {
        let key = Self::reserve_marker_key(escrow_id);
        env.storage().persistent().remove(&key);
    }

    pub fn repair_held_reserve_page(
        env: &Env,
        currency: &Address,
        offset: u32,
        limit: u32,
    ) -> Result<RebuildReport, QuickLendXError> {
        const MAX_REBUILD_PAGE: u32 = 100;

        if limit == 0 {
            return Err(QuickLendXError::InvalidAmount);
        }

        let capped = limit.min(MAX_REBUILD_PAGE);
        let ids = if offset == 0 {
            if InvoiceStorage::get_total_count(env) > MAX_REPAIR_SNAPSHOT_IDS {
                return Err(QuickLendXError::OperationNotAllowed);
            }
            let ids = InvoiceStorage::get_all_invoice_ids(env);
            if ids.len() as u64 > MAX_REPAIR_SNAPSHOT_IDS {
                return Err(QuickLendXError::OperationNotAllowed);
            }
            Self::set_repair_snapshot(env, currency, &ids);
            ids
        } else {
            Self::get_repair_snapshot(env, currency).ok_or(QuickLendXError::InvalidStatus)?
        };
        let total = ids.len();

        if offset > total {
            return Err(QuickLendXError::InvalidStatus);
        }

        let mut reserve = if offset == 0 {
            HeldEscrowReserve {
                amount: 0,
                complete: false,
                repair_next_offset: 0,
            }
        } else {
            let reserve = Self::get_held_reserve_record(env, currency)
                .ok_or(QuickLendXError::InvalidStatus)?;
            if reserve.complete || reserve.repair_next_offset != offset {
                return Err(QuickLendXError::InvalidStatus);
            }
            reserve
        };

        let start = offset;
        let end = start.saturating_add(capped).min(total);
        let mut reindexed = 0u32;
        let mut i = start;

        while i < end {
            if let Some(invoice_id) = ids.get(i) {
                if let Some(escrow) = Self::get_escrow_by_invoice(env, &invoice_id) {
                    if &escrow.currency == currency {
                        if escrow.status == EscrowStatus::Held {
                            if escrow.amount <= 0 {
                                return Err(QuickLendXError::InvalidAmount);
                            }

                            reserve.amount = reserve
                                .amount
                                .checked_add(escrow.amount)
                                .ok_or(QuickLendXError::ArithmeticOverflow)?;
                            Self::mark_reserve_accounted(env, &escrow.escrow_id);
                            reindexed = reindexed.saturating_add(1);
                        } else {
                            Self::clear_reserve_accounted(env, &escrow.escrow_id);
                        }
                    }
                }
            }
            i = i.saturating_add(1);
        }

        reserve.repair_next_offset = if end >= total { 0 } else { end };
        reserve.complete = end >= total;
        Self::set_held_reserve_record(env, currency, &reserve);
        if reserve.complete {
            Self::clear_repair_snapshot(env, currency);
        }

        Ok(RebuildReport {
            scanned: end.saturating_sub(start),
            reindexed,
            next_offset: reserve.repair_next_offset,
        })
    }
}

impl EscrowStorage {
    /// Return the total locked escrow value for `currency` across all held escrows.
    ///
    /// Uses the pre-computed `HeldEscrowReserve` accumulator stored by
    /// `store_escrow`/`update_escrow`, so this is an O(1) read — no unbounded scan.
    /// Returns 0 if no escrows have been created for this currency.
    pub fn get_total_locked_escrow_for_currency(env: &Env, currency: &Address) -> i128 {
        Self::get_held_reserve(env, currency)
    }

    /// Return the total locked escrow value summed across up to `max_currencies`
    /// currencies found in the held-reserve index.
    ///
    /// `currencies` is a caller-supplied bounded list of currency addresses to
    /// aggregate. Callers should obtain the list from `get_whitelisted_currencies`
    /// or an off-chain index and paginate to stay within resource limits.
    pub fn get_total_locked_escrow_bounded(
        env: &Env,
        currencies: &Vec<Address>,
        max_currencies: u32,
    ) -> i128 {
        let mut total: i128 = 0;
        let limit = currencies.len().min(max_currencies);
        for i in 0..limit {
            let currency = currencies.get_unchecked(i);
            let held = Self::get_held_reserve(env, &currency);
            total = total.saturating_add(held);
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payments::test_support::contract_env;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn test_held_reserve_legacy_migration_compatibility() {
        let (env, contract_id) = contract_env();
        let currency = Address::generate(&env);

        env.as_contract(&contract_id, || {
            // Write raw legacy i128 directly to persistent storage key
            let key = (HELD_ESCROW_RESERVE_KEY.clone(), currency.clone());
            env.storage().persistent().set(&key, &50_000i128);

            // Fetching through EscrowStorage should deserialize successfully
            let amount = EscrowStorage::get_held_reserve(&env, &currency);
            assert_eq!(amount, 50_000);

            // It should be marked incomplete because it hasn't been verified/repaired yet
            assert!(!EscrowStorage::is_held_reserve_complete(&env, &currency));
        });
    }

    #[test]
    fn test_held_reserve_malformed_negative_amount_fails_closed() {
        let (env, contract_id) = contract_env();
        let currency = Address::generate(&env);

        env.as_contract(&contract_id, || {
            // Write corrupted negative reserve record
            let key = (HELD_ESCROW_RESERVE_KEY.clone(), currency.clone());
            env.storage().persistent().set(&key, &-10_000i128);

            let amount = EscrowStorage::get_held_reserve(&env, &currency);
            assert_eq!(amount, 0);
            assert!(!EscrowStorage::is_held_reserve_complete(&env, &currency));
        });
    }
}
