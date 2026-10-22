//! QuickLendX contract entrypoints.
//!
//! This file is the single `#[contract]` type for the crate. It currently
//! exposes the protocol-initialization and admin-management slice (issue
//! #2925); the other modules (invoices, bids, escrow, settlement, disputes,
//! KYC, fees, pause, backup) are wired in by their own issues.
//!
//! Every entrypoint is a thin wrapper: authentication, validation, storage
//! and events all live in [`crate::init::ProtocolInitializer`] and
//! [`crate::admin::AdminStorage`]. The contract layer only forwards `&env`.
//!
//! There is deliberately no standalone `initialize_admin(admin)` entrypoint.
//! It would let anyone who can sign for *some* address install that address
//! as admin before the deployer calls `initialize`, which would then leave
//! the protocol without a treasury, fee or config. Admin is set only through
//! `initialize`.

use soroban_sdk::{contract, contractimpl, Address, Env, Vec};

use crate::admin::AdminStorage;
use crate::errors::QuickLendXError;
use crate::init::{
    InitializationParams, ProtocolConfig, ProtocolConfigDiff, ProtocolConfigParams,
    ProtocolInitializer,
};

#[contract]
pub struct QuickLendXContract;

#[contractimpl]
impl QuickLendXContract {
    // ── Protocol initialization ─────────────────────────────────────────────

    /// Initialize the protocol. `params.admin` must sign. Can succeed once;
    /// repeating the call with identical params returns `Ok(())`, any other
    /// params return `OperationNotAllowed`.
    pub fn initialize(env: Env, params: InitializationParams) -> Result<(), QuickLendXError> {
        ProtocolInitializer::initialize(&env, &params)
    }

    /// `true` once `initialize` has completed.
    pub fn is_initialized(env: Env) -> bool {
        ProtocolInitializer::is_initialized(&env)
    }

    /// Protocol version recorded at initialization (or the current constant
    /// before initialization).
    pub fn get_version(env: Env) -> u32 {
        ProtocolInitializer::get_version(&env)
    }

    /// Current protocol configuration, `None` before initialization.
    pub fn get_protocol_config(env: Env) -> Option<ProtocolConfig> {
        ProtocolInitializer::get_protocol_config(&env)
    }

    /// Dry-run a configuration change (admin only, read-only).
    pub fn preview_protocol_config(
        env: Env,
        admin: Address,
        params: ProtocolConfigParams,
    ) -> Result<ProtocolConfigDiff, QuickLendXError> {
        ProtocolInitializer::preview_protocol_config(&env, &admin, params)
    }

    /// Update protocol configuration (admin only).
    pub fn set_protocol_config(
        env: Env,
        admin: Address,
        min_invoice_amount: i128,
        max_due_date_days: u64,
        grace_period_seconds: u64,
        backfill_max_batch_size: u32,
    ) -> Result<(), QuickLendXError> {
        ProtocolInitializer::set_protocol_config(
            &env,
            &admin,
            min_invoice_amount,
            max_due_date_days,
            grace_period_seconds,
            backfill_max_batch_size,
        )
    }

    /// Update the protocol fee in basis points (admin only).
    pub fn set_fee_config(env: Env, admin: Address, fee_bps: u32) -> Result<(), QuickLendXError> {
        ProtocolInitializer::set_fee_config(&env, &admin, fee_bps)
    }

    /// Update the treasury address (admin only).
    pub fn set_treasury(
        env: Env,
        admin: Address,
        treasury: Address,
    ) -> Result<(), QuickLendXError> {
        ProtocolInitializer::set_treasury(&env, &admin, &treasury)
    }

    pub fn get_fee_bps(env: Env) -> u32 {
        ProtocolInitializer::get_fee_bps(&env)
    }

    pub fn get_treasury(env: Env) -> Option<Address> {
        ProtocolInitializer::get_treasury(&env)
    }

    pub fn get_min_invoice_amount(env: Env) -> i128 {
        ProtocolInitializer::get_min_invoice_amount(&env)
    }

    pub fn get_max_due_date_days(env: Env) -> u64 {
        ProtocolInitializer::get_max_due_date_days(&env)
    }

    pub fn get_grace_period_seconds(env: Env) -> u64 {
        ProtocolInitializer::get_grace_period_seconds(&env)
    }

    pub fn get_corridors(env: Env) -> Vec<Address> {
        ProtocolInitializer::get_corridors(&env)
    }

    // ── Admin management ────────────────────────────────────────────────────

    /// Current admin, `None` before initialization.
    pub fn get_current_admin(env: Env) -> Option<Address> {
        AdminStorage::get_admin(&env)
    }

    /// Transfer admin. With two-step mode enabled this only records
    /// `new_admin` as pending; `accept_admin_transfer` completes it.
    pub fn transfer_admin(
        env: Env,
        current_admin: Address,
        new_admin: Address,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::transfer_admin(&env, &current_admin, &new_admin)
    }

    /// Start a two-step transfer to `pending_admin`.
    pub fn initiate_admin_transfer(
        env: Env,
        current_admin: Address,
        pending_admin: Address,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::initiate_admin_transfer(&env, &current_admin, &pending_admin)
    }

    /// Complete a two-step transfer; `pending_admin` must sign.
    pub fn accept_admin_transfer(env: Env, pending_admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::accept_admin_transfer(&env, &pending_admin)
    }

    /// Cancel a pending two-step transfer (current admin only).
    pub fn cancel_admin_transfer(env: Env, current_admin: Address) -> Result<(), QuickLendXError> {
        AdminStorage::cancel_admin_transfer(&env, &current_admin)
    }

    /// Enable or disable two-step admin transfers (admin only).
    pub fn set_two_step_enabled(
        env: Env,
        admin: Address,
        enabled: bool,
    ) -> Result<(), QuickLendXError> {
        AdminStorage::set_two_step_enabled(&env, &admin, enabled)
    }

    pub fn is_two_step_enabled(env: Env) -> bool {
        AdminStorage::is_two_step_enabled(&env)
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        AdminStorage::get_pending_admin(&env)
    }
}
