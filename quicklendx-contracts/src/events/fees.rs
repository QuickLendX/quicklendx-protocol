#![allow(deprecated)]

use crate::fees::FeeType;
use crate::types::PlatformFeeConfig;
use soroban_sdk::{contractevent, symbol_short, Address, BytesN, Env};

#[contractevent]
pub struct InsuranceAdded {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub investor: Address,
    pub provider: Address,
    pub coverage_percentage: u32,
    pub coverage_amount: i128,
    pub premium_amount: i128,
}

#[contractevent]
pub struct InsurancePremiumCollected {
    pub investment_id: BytesN<32>,
    pub provider: Address,
    pub premium_amount: i128,
}

#[contractevent]
pub struct InsuranceClaimed {
    pub investment_id: BytesN<32>,
    pub invoice_id: BytesN<32>,
    pub provider: Address,
    pub coverage_amount: i128,
}

#[contractevent]
pub struct PlatformFeeUpdated {
    pub fee_bps: u32,
    pub updated_at: u64,
    pub updated_by: Address,
}

#[contractevent]
pub struct FeeStructureUpdated {
    pub fee_type: FeeType,
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct PlatformFeeRouted {
    pub invoice_id: BytesN<32>,
    pub recipient: Address,
    pub fee_amount: i128,
    pub timestamp: u64,
}

#[contractevent]
pub struct PlatformFeeConfigUpdated {
    pub old_fee_bps: u32,
    pub new_fee_bps: u32,
    pub updated_by: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct TreasuryConfigured {
    pub treasury_address: Address,
    pub configured_by: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct TreasuryRotationInitiated {
    pub new_address: Address,
    pub initiated_by: Address,
    pub confirmation_deadline: u64,
    pub timestamp: u64,
}

#[contractevent]
pub struct TreasuryRotationConfirmed {
    pub old_address: Address,
    pub new_address: Address,
    pub timestamp: u64,
}

#[contractevent]
pub struct ProfitFeeBreakdown {
    pub invoice_id: BytesN<32>,
    pub investment_amount: i128,
    pub payment_amount: i128,
    pub gross_profit: i128,
    pub platform_fee: i128,
    pub investor_return: i128,
    pub fee_bps_applied: i128,
    pub timestamp: u64,
}

#[contractevent]
pub struct RevenueDistributed {
    pub period: u64,
    pub treasury_amount: i128,
    pub developer_amount: i128,
    pub platform_amount: i128,
}

pub fn emit_insurance_added(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    investor: &Address,
    provider: &Address,
    coverage_percentage: u32,
    coverage_amount: i128,
    premium_amount: i128,
) {
    InsuranceAdded {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        investor: investor.clone(),
        provider: provider.clone(),
        coverage_percentage,
        coverage_amount,
        premium_amount,
    }
    .publish(env);
}

pub fn emit_insurance_premium_collected(
    env: &Env,
    investment_id: &BytesN<32>,
    provider: &Address,
    premium_amount: i128,
) {
    InsurancePremiumCollected {
        investment_id: investment_id.clone(),
        provider: provider.clone(),
        premium_amount,
    }
    .publish(env);
}

pub fn emit_insurance_claimed(
    env: &Env,
    investment_id: &BytesN<32>,
    invoice_id: &BytesN<32>,
    provider: &Address,
    coverage_amount: i128,
) {
    InsuranceClaimed {
        investment_id: investment_id.clone(),
        invoice_id: invoice_id.clone(),
        provider: provider.clone(),
        coverage_amount,
    }
    .publish(env);
}

pub fn emit_platform_fee_updated(env: &Env, config: &PlatformFeeConfig) {
    PlatformFeeUpdated {
        fee_bps: config.fee_bps,
        updated_at: config.updated_at,
        updated_by: config.updated_by.clone(),
    }
    .publish(env);
}

pub fn emit_fee_structure_updated(
    env: &Env,
    fee_type: &FeeType,
    old_fee_bps: u32,
    new_fee_bps: u32,
    updated_by: &Address,
) {
    FeeStructureUpdated {
        fee_type: fee_type.clone(),
        old_fee_bps,
        new_fee_bps,
        updated_by: updated_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_platform_fee_routed(
    env: &Env,
    invoice_id: &BytesN<32>,
    recipient: &Address,
    fee_amount: i128,
) {
    PlatformFeeRouted {
        invoice_id: invoice_id.clone(),
        recipient: recipient.clone(),
        fee_amount,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_platform_fee_config_updated(
    env: &Env,
    old_fee_bps: u32,
    new_fee_bps: u32,
    updated_by: &Address,
) {
    PlatformFeeConfigUpdated {
        old_fee_bps,
        new_fee_bps,
        updated_by: updated_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_treasury_configured(env: &Env, treasury_address: &Address, configured_by: &Address) {
    TreasuryConfigured {
        treasury_address: treasury_address.clone(),
        configured_by: configured_by.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

#[allow(dead_code)]
pub fn emit_profit_fee_breakdown(
    env: &Env,
    invoice_id: &BytesN<32>,
    investment_amount: i128,
    payment_amount: i128,
    gross_profit: i128,
    platform_fee: i128,
    investor_return: i128,
    fee_bps_applied: i128,
) {
    ProfitFeeBreakdown {
        invoice_id: invoice_id.clone(),
        investment_amount,
        payment_amount,
        gross_profit,
        platform_fee,
        investor_return,
        fee_bps_applied,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

pub fn emit_revenue_distributed(
    env: &Env,
    period: u64,
    treasury_amount: i128,
    developer_amount: i128,
    platform_amount: i128,
) {
    RevenueDistributed {
        period,
        treasury_amount,
        developer_amount,
        platform_amount,
    }
    .publish(env);
}

pub fn emit_treasury_rotation_initiated(
    env: &Env,
    admin: &Address,
    new_address: &Address,
    deadline: u64,
) {
    env.events().publish(
        (symbol_short!("tr_rot_i"), admin.clone()),
        (new_address.clone(), deadline),
    );
}

pub fn emit_treasury_rotation_confirmed(env: &Env, admin: &Address, new_address: &Address) {
    env.events().publish(
        (symbol_short!("tr_rot_f"), admin.clone()),
        (new_address.clone(), env.ledger().timestamp()),
    );
}

pub fn treasury_rotation_cancelled(env: &Env, admin: &Address) {
    env.events()
        .publish((symbol_short!("tr_rot_cn"), admin.clone()), ());
}
