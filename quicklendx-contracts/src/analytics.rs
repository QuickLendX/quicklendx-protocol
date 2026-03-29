use crate::errors::QuickLendXError;
use crate::storage::{InvestmentStorage, InvoiceStorage};
use crate::types::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{contracttype, symbol_short, Address, Bytes, BytesN, Env, String, Symbol, Vec};

/// Time period for analytics reports
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TimePeriod {
    Daily,
    Weekly,
    Monthly,
    Quarterly,
    Yearly,
    AllTime,
}

/// Platform metrics structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct PlatformMetrics {
    pub total_invoices: u32,
    pub total_investments: u32,
    pub total_volume: i128,
    pub total_fees_collected: i128,
    pub active_investors: u32,
    pub verified_businesses: u32,
    pub average_invoice_amount: i128,
    pub average_investment_amount: i128,
    pub platform_fee_rate: i128,
    pub default_rate: i128,
    pub success_rate: i128,
    pub timestamp: u64,
}

/// User behavior analytics
#[contracttype]
#[derive(Clone, Debug)]
pub struct UserBehaviorMetrics {
    pub user_address: Address,
    pub total_invoices_uploaded: u32,
    pub total_investments_made: u32,
    pub total_bids_placed: u32,
    pub average_bid_amount: i128,
    pub average_investment_amount: i128,
    pub success_rate: i128,
    pub default_rate: i128,
    pub last_activity: u64,
    pub preferred_categories: Vec<InvoiceCategory>,
    pub risk_score: u32,
}

/// Financial analytics structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct FinancialMetrics {
    pub total_volume: i128,
    pub total_fees: i128,
    pub total_profits: i128,
    pub average_return_rate: i128,
    pub volume_by_category: Vec<(InvoiceCategory, i128)>,
    pub volume_by_period: Vec<(TimePeriod, i128)>,
    pub fee_breakdown: Vec<(String, i128)>,
    pub profit_margins: Vec<(String, i128)>,
    pub currency_distribution: Vec<(Address, i128)>,
}

/// Performance tracking metrics
#[contracttype]
#[derive(Clone, Debug)]
pub struct PerformanceMetrics {
    pub platform_uptime: u64,
    pub average_settlement_time: u64,
    pub average_verification_time: u64,
    pub dispute_resolution_time: u64,
    pub system_response_time: u64,
    pub transaction_success_rate: i128,
    pub error_rate: i128,
    pub user_satisfaction_score: u32,
    pub platform_efficiency: i128,
}

/// Business report structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct BusinessReport {
    pub report_id: BytesN<32>,
    pub business_address: Address,
    pub period: TimePeriod,
    pub start_date: u64,
    pub end_date: u64,
    pub invoices_uploaded: u32,
    pub invoices_funded: u32,
    pub total_volume: i128,
    pub average_funding_time: u64,
    pub success_rate: i128,
    pub default_rate: i128,
    pub category_breakdown: Vec<(InvoiceCategory, u32)>,
    pub rating_average: Option<u32>,
    pub total_ratings: u32,
    pub generated_at: u64,
}

/// Investor report structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct InvestorReport {
    pub report_id: BytesN<32>,
    pub investor_address: Address,
    pub period: TimePeriod,
    pub start_date: u64,
    pub end_date: u64,
    pub investments_made: u32,
    pub total_invested: i128,
    pub total_returns: i128,
    pub average_return_rate: i128,
    pub success_rate: i128,
    pub default_rate: i128,
    pub preferred_categories: Vec<(InvoiceCategory, u32)>,
    pub risk_tolerance: u32,
    pub portfolio_diversity: i128,
    pub generated_at: u64,
}

/// Enhanced investor analytics structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct InvestorAnalytics {
    pub investor_address: Address,
    pub tier: crate::verification::InvestorTier,
    pub risk_level: crate::verification::InvestorRiskLevel,
    pub risk_score: u32,
    pub investment_limit: i128,
    pub total_invested: i128,
    pub total_returns: i128,
    pub successful_investments: u32,
    pub defaulted_investments: u32,
    pub success_rate: i128,
    pub average_investment_size: i128,
    pub portfolio_diversity_score: u32,
    pub preferred_categories: Vec<(InvoiceCategory, u32)>,
    pub last_activity: u64,
    pub account_age: u64,
    pub compliance_score: u32,
    pub generated_at: u64,
}

/// Investor performance metrics
#[contracttype]
#[derive(Clone, Debug)]
pub struct InvestorPerformanceMetrics {
    pub total_investors: u32,
    pub verified_investors: u32,
    pub pending_investors: u32,
    pub rejected_investors: u32,
    pub investors_by_tier: Vec<(crate::verification::InvestorTier, u32)>,
    pub investors_by_risk: Vec<(crate::verification::InvestorRiskLevel, u32)>,
    pub total_investment_volume: i128,
    pub average_investment_size: i128,
    pub platform_success_rate: i128,
    pub average_risk_score: u32,
    pub top_performing_investors: Vec<Address>,
    pub generated_at: u64,
}

/// Analytics storage structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct AnalyticsData {
    pub platform_metrics: PlatformMetrics,
    pub performance_metrics: PerformanceMetrics,
    pub last_updated: u64,
    pub data_points: Vec<(u64, PlatformMetrics)>,
}

pub struct AnalyticsStorage;

impl AnalyticsStorage {
    fn platform_metrics_key() -> Symbol { symbol_short!("plt_met") }
    fn performance_metrics_key() -> Symbol { symbol_short!("perf_met") }
    fn user_behavior_key(user: &Address) -> (Symbol, Address) { (symbol_short!("usr_beh"), user.clone()) }
    fn business_report_key(id: &BytesN<32>) -> (Symbol, BytesN<32>) { (symbol_short!("biz_rpt"), id.clone()) }
    fn investor_report_key(id: &BytesN<32>) -> (Symbol, BytesN<32>) { (symbol_short!("inv_rpt"), id.clone()) }
    fn investor_analytics_key(inv: &Address) -> (Symbol, Address) { (symbol_short!("inv_anal"), inv.clone()) }
    fn investor_performance_key() -> Symbol { symbol_short!("inv_perf") }

    pub fn store_platform_metrics(env: &Env, metrics: &PlatformMetrics) {
        env.storage().instance().set(&Self::platform_metrics_key(), metrics);
    }
    pub fn get_platform_metrics(env: &Env) -> Option<PlatformMetrics> {
        env.storage().instance().get(&Self::platform_metrics_key())
    }
    pub fn store_performance_metrics(env: &Env, metrics: &PerformanceMetrics) {
        env.storage().instance().set(&Self::performance_metrics_key(), metrics);
    }
    pub fn get_performance_metrics(env: &Env) -> Option<PerformanceMetrics> {
        env.storage().instance().get(&Self::performance_metrics_key())
    }
    pub fn store_user_behavior(env: &Env, user: &Address, behavior: &UserBehaviorMetrics) {
        env.storage().instance().set(&Self::user_behavior_key(user), behavior);
    }
    pub fn store_business_report(env: &Env, report: &BusinessReport) {
        env.storage().instance().set(&Self::business_report_key(&report.report_id), report);
    }
    pub fn get_business_report(env: &Env, report_id: &BytesN<32>) -> Option<BusinessReport> {
        env.storage().instance().get(&Self::business_report_key(report_id))
    }
    pub fn store_investor_report(env: &Env, report: &InvestorReport) {
        env.storage().instance().set(&Self::investor_report_key(&report.report_id), report);
    }
    pub fn get_investor_report(env: &Env, report_id: &BytesN<32>) -> Option<InvestorReport> {
        env.storage().instance().get(&Self::investor_report_key(report_id))
    }
    pub fn store_investor_analytics(env: &Env, investor: &Address, analytics: &InvestorAnalytics) {
        env.storage().instance().set(&Self::investor_analytics_key(investor), analytics);
    }
    pub fn get_investor_analytics(env: &Env, investor: &Address) -> Option<InvestorAnalytics> {
        env.storage().instance().get(&Self::investor_analytics_key(investor))
    }
    pub fn store_investor_performance(env: &Env, metrics: &InvestorPerformanceMetrics) {
        env.storage().instance().set(&Self::investor_performance_key(), metrics);
    }
    pub fn get_investor_performance(env: &Env) -> Option<InvestorPerformanceMetrics> {
        env.storage().instance().get(&Self::investor_performance_key())
    }

    pub fn generate_report_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let sequence = env.ledger().sequence();
        let mut id_bytes = [0u8; 32];
        id_bytes[0..8].copy_from_slice(&timestamp.to_be_bytes());
        id_bytes[8..12].copy_from_slice(&sequence.to_be_bytes());
        BytesN::from_array(env, &id_bytes)
    }
}

pub struct AnalyticsCalculator;

impl AnalyticsCalculator {
    fn bps(numer: u32, denom: u32) -> i128 {
        if denom == 0 { return 0; }
        ((numer as i128).saturating_mul(10000)).saturating_div(denom as i128).min(10000)
    }

    pub fn initialize_category_counters(env: &Env) {
        let categories = [
            InvoiceCategory::Services, InvoiceCategory::Products, InvoiceCategory::Consulting,
            InvoiceCategory::Manufacturing, InvoiceCategory::Technology, InvoiceCategory::Healthcare,
            InvoiceCategory::Other,
        ];
        for cat in categories.iter() {
            let key = (symbol_short!("biz_cat"), cat.clone());
            if !env.storage().instance().has(&key) {
                env.storage().instance().set(&key, &0u32);
            }
        }
    }

    pub fn increment_category_counter(env: &Env, cat: &InvoiceCategory) {
        let key = (symbol_short!("biz_cat"), cat.clone());
        let count: u32 = env.storage().instance().get(&key).unwrap_or(0);
        env.storage().instance().set(&key, &(count.saturating_add(1)));
    }

    pub fn calculate_platform_metrics(env: &Env) -> Result<PlatformMetrics, QuickLendXError> {
        let current_timestamp = env.ledger().timestamp();
        let pending = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Pending);
        let verified = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Verified);
        let funded = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Funded);
        let paid = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Paid);
        let defaulted = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Defaulted);

        let total_invoices = (pending.len() + verified.len() + funded.len() + paid.len() + defaulted.len()) as u32;

        let mut total_volume = 0i128;
        for ids in [&pending, &verified, &funded, &paid, &defaulted].iter() {
            for id in ids.iter() {
                if let Some(invoice) = InvoiceStorage::get_invoice(env, &id) {
                    total_volume = total_volume.saturating_add(invoice.amount);
                }
            }
        }

        let total_investments = (funded.len() + paid.len() + defaulted.len()) as u32;

        let mut total_fees = 0i128;
        for id in paid.iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(env, &id) {
                if let Some(investment) = InvestmentStorage::get_investment_by_invoice(env, &id) {
                    let (_, fee) = crate::profits::PlatformFee::calculate_with_fee_bps(
                        investment.amount, invoice.amount, crate::profits::DEFAULT_PLATFORM_FEE_BPS
                    );
                    total_fees = total_fees.saturating_add(fee);
                }
            }
        }

        let verified_businesses = crate::verification::BusinessVerificationStorage::get_verified_businesses(env).len() as u32;

        let avg_invoice = if total_invoices > 0 { total_volume / total_invoices as i128 } else { 0 };
        let avg_investment = if total_investments > 0 {
             let mut total_invested = 0i128;
             for ids in [&funded, &paid, &defaulted].iter() {
                 for id in ids.iter() {
                     if let Some(inv) = InvestmentStorage::get_investment_by_invoice(env, &id) {
                         total_invested = total_invested.saturating_add(inv.amount);
                     }
                 }
             }
             total_invested / total_investments as i128
        } else { 0 };

        let fee_rate = crate::profits::PlatformFee::get_config(env).fee_bps as i128;
        let default_rate = Self::bps(defaulted.len() as u32, total_investments);
        let success_rate = Self::bps(paid.len() as u32, total_investments);

        Ok(PlatformMetrics {
            total_invoices, total_investments, total_volume,
            total_fees_collected: total_fees, active_investors: 0,
            verified_businesses, average_invoice_amount: avg_invoice,
            average_investment_amount: avg_investment, platform_fee_rate: fee_rate,
            default_rate, success_rate, timestamp: current_timestamp,
        })
    }

    pub fn calculate_financial_metrics(env: &Env, period: TimePeriod) -> Result<FinancialMetrics, QuickLendXError> {
        let current_timestamp = env.ledger().timestamp();
        let (start, end) = Self::get_period_dates(current_timestamp, period.clone());

        let mut total_volume = 0i128;
        let mut total_fees = 0i128;
        let mut total_profits = 0i128;
        let mut volume_by_category = Vec::new(env);
        let mut currency_distribution = Vec::new(env);

        let categories = [
            InvoiceCategory::Services, InvoiceCategory::Products, InvoiceCategory::Consulting,
            InvoiceCategory::Manufacturing, InvoiceCategory::Technology, InvoiceCategory::Healthcare,
            InvoiceCategory::Other,
        ];

        for cat in categories.iter() {
            volume_by_category.push_back((cat.clone(), 0i128));
        }

        let mut all_ids = Vec::new(env);
        for status in [InvoiceStatus::Pending, InvoiceStatus::Verified, InvoiceStatus::Funded, InvoiceStatus::Paid, InvoiceStatus::Defaulted].iter() {
            let ids = InvoiceStorage::get_invoices_by_status(env, status);
            for id in ids.iter() { all_ids.push_back(id); }
        }

        for id in all_ids.iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(env, &id) {
                if invoice.created_at >= start && invoice.created_at <= end {
                    total_volume = total_volume.saturating_add(invoice.amount);
                    for i in 0..volume_by_category.len() {
                        let (cat, vol) = volume_by_category.get(i).unwrap();
                        if cat == invoice.category {
                            volume_by_category.set(i, (cat, vol.saturating_add(invoice.amount)));
                            break;
                        }
                    }
                    if invoice.status == InvoiceStatus::Paid {
                        if let Some(invt) = InvestmentStorage::get_investment_by_invoice(env, &id) {
                            let (profit, fee) = crate::profits::PlatformFee::calculate_with_fee_bps(
                                invt.amount, invoice.amount, crate::profits::DEFAULT_PLATFORM_FEE_BPS
                            );
                            total_fees = total_fees.saturating_add(fee);
                            total_profits = total_profits.saturating_add(profit.saturating_sub(invt.amount));
                        }
                    }
                }
            }
        }

        Ok(FinancialMetrics {
            total_volume, total_fees, total_profits,
            average_return_rate: if total_volume > 0 { total_profits * 10000 / total_volume } else { 0 },
            volume_by_category,
            volume_by_period: {
                let mut v = Vec::new(env);
                v.push_back((period, total_volume));
                v
            },
            fee_breakdown: {
                let mut v = Vec::new(env);
                v.push_back((String::from_str(env, "platform"), total_fees));
                v
            },
            profit_margins: {
                let mut v = Vec::new(env);
                v.push_back((String::from_str(env, "gross"), total_profits));
                v
            },
            currency_distribution,
        })
    }

    pub fn get_period_dates(now: u64, period: TimePeriod) -> (u64, u64) {
        let start = match period {
            TimePeriod::Daily => now.saturating_sub(86400),
            TimePeriod::Weekly => now.saturating_sub(604800),
            TimePeriod::Monthly => now.saturating_sub(2592000),
            TimePeriod::Quarterly => now.saturating_sub(7776000),
            TimePeriod::Yearly => now.saturating_sub(31536000),
            TimePeriod::AllTime => 0,
        };
        (start, now)
    }
}
