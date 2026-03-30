use soroban_sdk::{symbol_short, Address, BytesN, Env, Symbol, Vec as SorobanVec};
pub use crate::types::{PlatformMetrics, PerformanceMetrics, UserBehaviorMetrics, BusinessReport};
use crate::types::{InvoiceStatus, TimePeriod};
use crate::errors::QuickLendXError;
use crate::storage::{InvoiceStorage};

pub struct AnalyticsCalculator;
pub struct AnalyticsStorage;

impl AnalyticsStorage {
    fn platform_metrics_key() -> Symbol { symbol_short!("plt_met") }
    // ... (other keys)

    pub fn generate_report_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let sequence = env.ledger().sequence();
        let mut id_bytes = [0u8; 32];
        id_bytes[0..8].copy_from_slice(&timestamp.to_be_bytes());
        id_bytes[8..12].copy_from_slice(&sequence.to_be_bytes());
        BytesN::from_array(env, &id_bytes)
    }

    pub fn get_platform_metrics(env: &Env) -> Option<PlatformMetrics> {
        env.storage().persistent().get(&Self::platform_metrics_key())
    }

    pub fn get_performance_metrics(env: &Env) -> Option<PerformanceMetrics> {
        env.storage().persistent().get(&symbol_short!("perf_met"))
    }

    pub fn store_business_report(env: &Env, report: &BusinessReport) {
        let key = (symbol_short!("bus_rep"), report.business_address.clone(), report.report_id.clone());
        env.storage().persistent().set(&key, report);
    }

    pub fn get_business_report(env: &Env, business: &Address, report_id: &BytesN<32>) -> Option<BusinessReport> {
        let key = (symbol_short!("bus_rep"), business.clone(), report_id.clone());
        env.storage().persistent().get(&key)
    }
}

impl AnalyticsCalculator {
    fn bps(numer: u32, denom: u32) -> i128 {
        if denom == 0 { return 0; }
        ((numer as i128).saturating_mul(10000)).saturating_div(denom as i128).min(10000)
    }

    pub fn calculate_platform_metrics(env: &Env) -> Result<PlatformMetrics, QuickLendXError> {
        let current_timestamp = env.ledger().timestamp();
        let pending = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Pending);
        let verified = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Verified);
        let funded = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Funded);
        let paid = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Paid);
        let defaulted = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Defaulted);

        let total_invoices = (pending.len() + verified.len() + funded.len() + paid.len() + defaulted.len()) as u32;
        let total_investments = (funded.len() + paid.len() + defaulted.len()) as u32;

        Ok(PlatformMetrics {
            total_invoices,
            total_investments,
            total_volume: 0,
            total_fees_collected: 0,
            active_investors: 0,
            verified_businesses: 0,
            average_invoice_amount: 0,
            average_investment_amount: 0,
            platform_fee_rate: 0,
            default_rate: Self::bps(defaulted.len(), total_investments),
            success_rate: Self::bps(paid.len(), total_investments),
            timestamp: current_timestamp,
        })
    }

    pub fn calculate_user_behavior_metrics(env: &Env, user: &Address) -> Result<UserBehaviorMetrics, QuickLendXError> {
        Ok(UserBehaviorMetrics {
            user_address: user.clone(),
            total_invoices_uploaded: 0,
            total_investments_made: 0,
            total_bids_placed: 0,
            average_bid_amount: 0,
            average_investment_amount: 0,
            success_rate: 0,
            default_rate: 0,
            last_activity: env.ledger().timestamp(),
            preferred_categories: SorobanVec::new(env),
            risk_score: 0,
        })
    }

    pub fn calculate_performance_metrics(env: &Env) -> Result<PerformanceMetrics, QuickLendXError> {
        Ok(PerformanceMetrics {
            platform_uptime: env.ledger().timestamp(),
            average_settlement_time: 0,
            average_verification_time: 0,
            dispute_resolution_time: 0,
            system_response_time: 0,
            transaction_success_rate: 0,
            error_rate: 0,
            user_satisfaction_score: 0,
            platform_efficiency: 0,
        })
    }

    pub fn generate_business_report(env: &Env, business: &Address, period: TimePeriod) -> Result<BusinessReport, QuickLendXError> {
        Ok(BusinessReport {
            report_id: AnalyticsStorage::generate_report_id(env),
            business_address: business.clone(),
            period,
            start_date: 0,
            end_date: env.ledger().timestamp(),
            invoices_uploaded: 0,
            invoices_funded: 0,
            total_volume: 0,
            average_funding_time: 0,
            success_rate: 0,
            default_rate: 0,
            category_breakdown: SorobanVec::new(env),
            rating_average: None,
            total_ratings: 0,
            generated_at: env.ledger().timestamp(),
        })
    }
}