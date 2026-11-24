#![allow(deprecated)]

use crate::types::InvestorFreezeReason;
use crate::verification::{InvestorRiskLevel, InvestorTier, InvestorVerification};
use soroban_sdk::{contractevent, Address, Env, String};

/// Emitted when an investor is verified by an admin.
///
/// Topic: [`TOPIC_INVESTOR_VERIFIED`] (if defined) or the struct topic.
///
/// # Fields
/// - `investor` – Address of the verified investor.
/// - `investment_limit` – Computed investment limit for this investor.
/// - `verified_at` – Ledger timestamp when verification occurred.
/// - `verified_by` – Address of the admin who performed the verification.
/// - `tier` – Investor tier (serialized as u32: 0=Basic, 1=Silver, 2=Gold, 3=Platinum, 4=VIP).
/// - `risk_level` – Risk level (serialized as u32: 0=Low, 1=Medium, 2=High, 3=VeryHigh).
/// - `risk_score` – Numeric risk score (0–100).
#[contractevent]
pub struct InvestorVerified {
    pub investor: Address,
    pub investment_limit: i128,
    pub verified_at: u64,
    pub verified_by: Address,
    pub tier: u32,
    pub risk_level: u32,
    pub risk_score: u32,
}

/// Emitted when a business or investor submits a KYC application.
///
/// Topic: [`TOPIC_KYC_SUBMITTED`] (`"kyc_submitted"`)
///
/// # Fields
/// - `subject` – Address of the business or investor submitting KYC.
/// - `is_resubmit` – `true` if this is a resubmission after a prior rejection.
/// - `timestamp` – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct KycSubmitted {
    pub subject: Address,
    pub is_resubmit: bool,
    pub timestamp: u64,
}

/// Emitted when an admin verifies (approves) a business or investor KYC record.
///
/// Topic: [`TOPIC_KYC_VERIFIED`] (`"kyc_verified"`)
///
/// # Fields
/// - `subject` – Address of the verified business or investor.
/// - `admin` – Address of the admin who performed the verification.
/// - `timestamp` – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct KycVerified {
    pub subject: Address,
    pub admin: Address,
    pub timestamp: u64,
}

/// Emitted when an admin rejects a pending business or investor KYC record.
///
/// Topic: [`TOPIC_KYC_REJECTED`] (`"kyc_rejected"`)
///
/// # Fields
/// - `subject` – Address of the rejected business or investor.
/// - `admin` – Address of the admin who performed the rejection.
/// - `reason` – Auditable rejection reason.
/// - `timestamp` – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct KycRejected {
    pub subject: Address,
    pub admin: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Emitted when an admin revokes a previously-verified investor's KYC.
///
/// Topic: [`TOPIC_KYC_REVOKED`] (`"kyc_revoked"`)
///
/// # Fields
/// - `investor` – Address of the investor whose KYC was revoked.
/// - `admin` – Address of the admin who performed the revocation.
/// - `reason` – Auditable revocation reason.
/// - `timestamp` – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct KycRevoked {
    pub investor: Address,
    pub admin: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Emitted when an admin freezes an investor, blocking all investment actions.
///
/// Topic: [`TOPIC_INVESTOR_FROZEN`] (`"investor_frozen"`)
///
/// # Fields
/// - `investor` – Address of the frozen investor.
/// - `admin` – Address of the admin who performed the freeze.
/// - `reason` – The freeze reason (serialized as string).
/// - `timestamp` – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvestorFrozen {
    pub investor: Address,
    pub admin: Address,
    pub reason: String,
    pub timestamp: u64,
}

/// Emitted when an admin unfreezes an investor, restoring investment actions.
///
/// Topic: [`TOPIC_INVESTOR_UNFROZEN`] (`"investor_unfrozen"`)
///
/// # Fields
/// - `investor` – Address of the unfrozen investor.
/// - `admin` – Address of the admin who performed the unfreeze.
/// - `timestamp` – Ledger timestamp at emission time.
#[derive(Debug, PartialEq)]
#[contractevent]
pub struct InvestorUnfrozen {
    pub investor: Address,
    pub admin: Address,
    pub timestamp: u64,
}

pub fn emit_investor_verified(env: &Env, verification: &InvestorVerification) {
    let tier_num = match verification.tier {
        InvestorTier::Basic => 0u32,
        InvestorTier::Silver => 1,
        InvestorTier::Gold => 2,
        InvestorTier::Platinum => 3,
        InvestorTier::VIP => 4,
    };
    let risk_num = match verification.risk_level {
        InvestorRiskLevel::Low => 0u32,
        InvestorRiskLevel::Medium => 1,
        InvestorRiskLevel::High => 2,
        InvestorRiskLevel::VeryHigh => 3,
    };
    InvestorVerified {
        investor: verification.investor.clone(),
        investment_limit: verification.investment_limit,
        verified_at: verification.verified_at.unwrap_or(0),
        verified_by: verification
            .verified_by
            .clone()
            .unwrap_or(Address::from_str(
                env,
                "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            )),
        tier: tier_num,
        risk_level: risk_num,
        risk_score: verification.risk_score,
    }
    .publish(env);
}

/// Emitted when a business or investor submits a KYC application.
pub fn emit_kyc_submitted(env: &Env, subject: &Address, is_resubmit: bool) {
    KycSubmitted {
        subject: subject.clone(),
        is_resubmit,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emitted when an admin verifies (approves) a business or investor KYC record.
pub fn emit_kyc_verified(env: &Env, subject: &Address, admin: &Address) {
    KycVerified {
        subject: subject.clone(),
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emitted when an admin rejects a pending business or investor KYC record.
pub fn emit_kyc_rejected(env: &Env, subject: &Address, admin: &Address, reason: &String) {
    KycRejected {
        subject: subject.clone(),
        admin: admin.clone(),
        reason: reason.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emitted when an admin revokes a previously-verified investor's KYC.
pub fn emit_kyc_revoked(env: &Env, investor: &Address, admin: &Address, reason: &String) {
    KycRevoked {
        investor: investor.clone(),
        admin: admin.clone(),
        reason: reason.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emitted when an admin freezes an investor, blocking all investment actions.
pub fn emit_investor_frozen(
    env: &Env,
    investor: &Address,
    admin: &Address,
    reason: &InvestorFreezeReason,
) {
    let reason_str = match reason {
        crate::types::InvestorFreezeReason::AdminAction => String::from_str(env, "AdminAction"),
        crate::types::InvestorFreezeReason::KYCExpired => String::from_str(env, "KYCExpired"),
        crate::types::InvestorFreezeReason::ComplianceViolation => {
            String::from_str(env, "ComplianceViolation")
        }
        crate::types::InvestorFreezeReason::SuspiciousActivity => {
            String::from_str(env, "SuspiciousActivity")
        }
        crate::types::InvestorFreezeReason::LegalHold => String::from_str(env, "LegalHold"),
    };
    InvestorFrozen {
        investor: investor.clone(),
        admin: admin.clone(),
        reason: reason_str,
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}

/// Emitted when an admin unfreezes an investor, restoring investment actions.
pub fn emit_investor_unfrozen(env: &Env, investor: &Address, admin: &Address) {
    InvestorUnfrozen {
        investor: investor.clone(),
        admin: admin.clone(),
        timestamp: env.ledger().timestamp(),
    }
    .publish(env);
}
