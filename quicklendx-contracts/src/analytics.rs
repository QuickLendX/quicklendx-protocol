use crate::errors::QuickLendXError;
use crate::invoice::{InvoiceCategory, InvoiceStatus};
use soroban_sdk::{contracttype, symbol_short, Address, Bytes, BytesN, Env, String, Vec};

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
    fn platform_metrics_key() -> (soroban_sdk::Symbol,) {
        (symbol_short!("plt_met"),)
    }

    fn performance_metrics_key() -> (soroban_sdk::Symbol,) {
        (symbol_short!("perf_met"),)
    }

    fn user_behavior_key(user: &Address) -> (soroban_sdk::Symbol, Address) {
        (symbol_short!("usr_beh"), user.clone())
    }

    fn business_report_key(report_id: &BytesN<32>) -> (soroban_sdk::Symbol, BytesN<32>) {
        (symbol_short!("biz_rpt"), report_id.clone())
    }

    fn investor_report_key(report_id: &BytesN<32>) -> (soroban_sdk::Symbol, BytesN<32>) {
        (symbol_short!("inv_rpt"), report_id.clone())
    }


    pub fn store_platform_metrics(env: &Env, metrics: &PlatformMetrics) {
        env.storage()
            .instance()
            .set(&Self::platform_metrics_key(), metrics);
    }

    pub fn get_platform_metrics(env: &Env) -> Option<PlatformMetrics> {
        env.storage().instance().get(&Self::platform_metrics_key())
    }

    pub fn store_performance_metrics(env: &Env, metrics: &PerformanceMetrics) {
        env.storage()
            .instance()
            .set(&Self::performance_metrics_key(), metrics);
    }

    pub fn get_performance_metrics(env: &Env) -> Option<PerformanceMetrics> {
        env.storage()
            .instance()
            .get(&Self::performance_metrics_key())
    }

    pub fn store_user_behavior(env: &Env, user: &Address, behavior: &UserBehaviorMetrics) {
        env.storage()
            .instance()
            .set(&Self::user_behavior_key(user), behavior);
    }


    pub fn store_business_report(env: &Env, report: &BusinessReport) {
        env.storage()
            .instance()
            .set(&Self::business_report_key(&report.report_id), report);
    }

    pub fn get_business_report(env: &Env, report_id: &BytesN<32>) -> Option<BusinessReport> {
        env.storage()
            .instance()
            .get(&Self::business_report_key(report_id))
    }

    pub fn store_investor_report(env: &Env, report: &InvestorReport) {
        env.storage()
            .instance()
            .set(&Self::investor_report_key(&report.report_id), report);
    }

    pub fn get_investor_report(env: &Env, report_id: &BytesN<32>) -> Option<InvestorReport> {
        env.storage()
            .instance()
            .get(&Self::investor_report_key(report_id))
    }


    pub fn generate_report_id(env: &Env) -> BytesN<32> {
        let timestamp = env.ledger().timestamp();
        let sequence = env.ledger().sequence();
        let _combined = timestamp.wrapping_add(sequence as u64);
        let bytes = Bytes::new(env);
        let hash = env.crypto().sha256(&bytes);
        BytesN::from_array(&env, &hash.to_array())
    }
}

/// Analytics calculation functions
pub struct AnalyticsCalculator;

impl AnalyticsCalculator {
    /// Calculate comprehensive platform metrics
    pub fn calculate_platform_metrics(env: &Env) -> Result<PlatformMetrics, QuickLendXError> {
        let current_timestamp = env.ledger().timestamp();

        // Get all invoices by status
        let pending_invoices =
            crate::invoice::InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Pending);
        let verified_invoices =
            crate::invoice::InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Verified);
        let funded_invoices =
            crate::invoice::InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Funded);
        let paid_invoices =
            crate::invoice::InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Paid);
        let defaulted_invoices =
            crate::invoice::InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Defaulted);

        let total_invoices = (pending_invoices.len()
            + verified_invoices.len()
            + funded_invoices.len()
            + paid_invoices.len()
            + defaulted_invoices.len()) as u32;

        // Calculate total volume
        let mut total_volume = 0i128;
        for invoice_id in [
            &pending_invoices,
            &verified_invoices,
            &funded_invoices,
            &paid_invoices,
            &defaulted_invoices,
        ]
        .iter()
        {
            for id in invoice_id.iter() {
                if let Some(invoice) = crate::invoice::InvoiceStorage::get_invoice(env, &id) {
                    total_volume = total_volume.saturating_add(invoice.amount);
                }
            }
        }

        // Calculate total investments by counting funded invoices
        let total_investments = funded_invoices.len() as u32;

        // Calculate total fees collected
        let mut total_fees = 0i128;
        for invoice_id in paid_invoices.iter() {
            if let Some(invoice) = crate::invoice::InvoiceStorage::get_invoice(env, &invoice_id) {
                if let Some(investment) =
                    crate::investment::InvestmentStorage::get_investment_by_invoice(
                        env,
                        &invoice_id,
                    )
                {
                    let (_, platform_fee) =
                        crate::profits::calculate_profit(env, investment.amount, invoice.amount);
                    total_fees = total_fees.saturating_add(platform_fee);
                }
            }
        }

        // Count active investors (simplified - would need proper tracking)
        let active_investors = 0u32; // Placeholder - would need investor tracking

        // Count verified businesses
        let verified_businesses =
            crate::verification::BusinessVerificationStorage::get_verified_businesses(env);
        let verified_businesses_count = verified_businesses.len() as u32;

        // Calculate averages
        let average_invoice_amount = if total_invoices > 0 {
            total_volume.saturating_div(total_invoices as i128)
        } else {
            0
        };

        let average_investment_amount = if total_investments > 0 {
            let mut total_invested = 0i128;
            for invoice_id in funded_invoices.iter() {
                if let Some(investment) =
                    crate::investment::InvestmentStorage::get_investment_by_invoice(
                        env,
                        &invoice_id,
                    )
                {
                    total_invested = total_invested.saturating_add(investment.amount);
                }
            }
            total_invested.saturating_div(total_investments as i128)
        } else {
            0
        };

        // Get platform fee rate
        let platform_fee_config = crate::profits::PlatformFee::get_config(env);
        let platform_fee_rate = platform_fee_config.fee_bps;

        // Calculate default rate
        let _current_timestamp = env.ledger().timestamp();
        let default_rate = if total_investments > 0 {
            let defaulted_count = defaulted_invoices.len() as u32;
            (defaulted_count.saturating_mul(10000)).saturating_div(total_investments) as i128
        } else {
            0
        };

        // Calculate success rate
        let success_rate = if total_investments > 0 {
            let successful_count = paid_invoices.len() as u32;
            (successful_count.saturating_mul(10000)).saturating_div(total_investments) as i128
        } else {
            0
        };

        Ok(PlatformMetrics {
            total_invoices,
            total_investments,
            total_volume,
            total_fees_collected: total_fees,
            active_investors,
            verified_businesses: verified_businesses_count,
            average_invoice_amount,
            average_investment_amount,
            platform_fee_rate,
            default_rate,
            success_rate,
            timestamp: current_timestamp,
        })
    }

    /// Calculate user behavior metrics
    pub fn calculate_user_behavior_metrics(
        env: &Env,
        user: &Address,
    ) -> Result<UserBehaviorMetrics, QuickLendXError> {
        let _current_timestamp = env.ledger().timestamp();

        // Get user's invoices
        let user_invoices = crate::invoice::InvoiceStorage::get_business_invoices(env, user);
        let total_invoices_uploaded = user_invoices.len() as u32;

        // Get user's investments (simplified - would need proper tracking)
        let total_investments_made = 0u32; // Placeholder - would need investor tracking

        // Get user's bids (simplified - would need proper tracking)
        let total_bids_placed = 0u32;
        let total_bid_amount = 0i128;

        let average_bid_amount = if total_bids_placed > 0 {
            total_bid_amount.saturating_div(total_bids_placed as i128)
        } else {
            0
        };

        let average_investment_amount = 0i128; // Placeholder

        // Calculate success and default rates (simplified)
        let preferred_categories = Vec::new(env);

        let success_rate = 0i128; // Placeholder

        let default_rate = 0i128; // Placeholder

        // Calculate risk score based on default rate and investment patterns
        let risk_score = if default_rate > 1000 {
            // > 10%
            100
        } else if default_rate > 500 {
            // > 5%
            75
        } else if default_rate > 200 {
            // > 2%
            50
        } else {
            25
        };

        // Find last activity
        let mut last_activity = 0u64;
        for invoice_id in user_invoices.iter() {
            if let Some(invoice) = crate::invoice::InvoiceStorage::get_invoice(env, &invoice_id) {
                if invoice.created_at > last_activity {
                    last_activity = invoice.created_at;
                }
            }
        }

        Ok(UserBehaviorMetrics {
            user_address: user.clone(),
            total_invoices_uploaded,
            total_investments_made,
            total_bids_placed,
            average_bid_amount,
            average_investment_amount,
            success_rate,
            default_rate,
            last_activity,
            preferred_categories,
            risk_score,
        })
    }

    /// Calculate financial metrics
    pub fn calculate_financial_metrics(
        env: &Env,
        period: TimePeriod,
    ) -> Result<FinancialMetrics, QuickLendXError> {
        let current_timestamp = env.ledger().timestamp();
        let (start_date, end_date) = Self::get_period_dates(current_timestamp, period.clone());

        let mut total_volume = 0i128;
        let mut total_fees = 0i128;
        let mut total_profits = 0i128;
        let mut volume_by_category = Vec::new(env);
        let mut currency_distribution = Vec::new(env);

        // Initialize category tracking
        let categories = [
            InvoiceCategory::Services,
            InvoiceCategory::Products,
            InvoiceCategory::Consulting,
            InvoiceCategory::Manufacturing,
            InvoiceCategory::Technology,
            InvoiceCategory::Healthcare,
            InvoiceCategory::Other,
        ];

        for category in categories.iter() {
            volume_by_category.push_back((category.clone(), 0i128));
        }

        // Get all invoices in the period by combining all statuses
        let mut all_invoices = Vec::new(env);
        for status in [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
        ]
        .iter()
        {
            let invoices = crate::invoice::InvoiceStorage::get_invoices_by_status(env, status);
            for invoice_id in invoices.iter() {
                all_invoices.push_back(invoice_id);
            }
        }
        for invoice_id in all_invoices.iter() {
            if let Some(invoice) = crate::invoice::InvoiceStorage::get_invoice(env, &invoice_id) {
                if invoice.created_at >= start_date && invoice.created_at <= end_date {
                    total_volume = total_volume.saturating_add(invoice.amount);

                    // Update category volume
                    for i in 0..volume_by_category.len() {
                        let (cat, vol) = volume_by_category.get(i).unwrap();
                        if cat == invoice.category {
                            volume_by_category.set(i, (cat, vol.saturating_add(invoice.amount)));
                            break;
                        }
                    }

                    // Track currency distribution
                    let mut found_currency = false;
                    for i in 0..currency_distribution.len() {
                        let (curr, amount): (Address, i128) = currency_distribution.get(i).unwrap();
                        if curr == invoice.currency {
                            currency_distribution
                                .set(i, (curr, amount.saturating_add(invoice.amount)));
                            found_currency = true;
                            break;
                        }
                    }
                    if !found_currency {
                        currency_distribution.push_back((invoice.currency.clone(), invoice.amount));
                    }

                    // Calculate fees and profits for paid invoices
                    if invoice.status == InvoiceStatus::Paid {
                        if let Some(investment) =
                            crate::investment::InvestmentStorage::get_investment_by_invoice(
                                env,
                                &invoice_id,
                            )
                        {
                            let (profit, platform_fee) = crate::profits::calculate_profit(
                                env,
                                investment.amount,
                                invoice.amount,
                            );
                            total_fees = total_fees.saturating_add(platform_fee);
                            total_profits = total_profits.saturating_add(profit);
                        }
                    }
                }
            }
        }

        // Calculate average return rate
        let average_return_rate = if total_volume > 0 {
            total_profits
                .saturating_mul(10000)
                .saturating_div(total_volume)
        } else {
            0
        };

        // Create fee breakdown
        let mut fee_breakdown = Vec::new(env);
        fee_breakdown.push_back((String::from_str(env, "platform_fees"), total_fees));

        // Create profit margins
        let mut profit_margins = Vec::new(env);
        profit_margins.push_back((String::from_str(env, "gross_profit"), total_profits));
        profit_margins.push_back((
            String::from_str(env, "net_profit"),
            total_profits.saturating_sub(total_fees),
        ));

        // Create volume by period (simplified for this implementation)
        let mut volume_by_period = Vec::new(env);
        volume_by_period.push_back((period, total_volume));

        Ok(FinancialMetrics {
            total_volume,
            total_fees,
            total_profits,
            average_return_rate,
            volume_by_category,
            volume_by_period,
            fee_breakdown,
            profit_margins,
            currency_distribution,
        })
    }

    /// Calculate performance metrics
    pub fn calculate_performance_metrics(env: &Env) -> Result<PerformanceMetrics, QuickLendXError> {
        let current_timestamp = env.ledger().timestamp();

        // Calculate average settlement time (simplified)
        let total_settlement_time = 0u64;
        let settlement_count = 0u32;

        let average_settlement_time = if settlement_count > 0 {
            total_settlement_time.saturating_div(settlement_count as u64)
        } else {
            0
        };

        // Calculate average verification time (simplified)
        let total_verification_time = 0u64;
        let verification_count = 0u32;

        let average_verification_time = if verification_count > 0 {
            total_verification_time.saturating_div(verification_count as u64)
        } else {
            0
        };

        // Calculate dispute resolution time
        let mut total_dispute_time = 0u64;
        let mut dispute_count = 0u32;
        let invoices_with_disputes = crate::defaults::get_invoices_with_disputes(env);

        for invoice_id in invoices_with_disputes.iter() {
            if let Some(dispute) =
                crate::defaults::get_dispute_details(env, &invoice_id).unwrap_or(None)
            {
                if dispute.resolved_at > 0 {
                    let resolution_time = dispute.resolved_at.saturating_sub(dispute.created_at);
                    total_dispute_time = total_dispute_time.saturating_add(resolution_time);
                    dispute_count += 1;
                }
            }
        }

        let dispute_resolution_time = if dispute_count > 0 {
            total_dispute_time.saturating_div(dispute_count as u64)
        } else {
            0
        };

        // Calculate transaction success rate
        let mut total_transactions = 0u32;
        let mut successful_transactions = 0u32;
        for status in [
            InvoiceStatus::Pending,
            InvoiceStatus::Verified,
            InvoiceStatus::Funded,
            InvoiceStatus::Paid,
            InvoiceStatus::Defaulted,
        ]
        .iter()
        {
            let count =
                crate::invoice::InvoiceStorage::get_invoices_by_status(env, status).len() as u32;
            total_transactions += count;
            if *status == InvoiceStatus::Paid {
                successful_transactions = count;
            }
        }
        let transaction_success_rate = if total_transactions > 0 {
            (successful_transactions.saturating_mul(10000)).saturating_div(total_transactions)
                as i128
        } else {
            0
        };

        // Calculate error rate (simplified)
        let defaulted_invoices =
            crate::invoice::InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Defaulted);
        let error_rate = if total_transactions > 0 {
            (defaulted_invoices.len() as u32)
                .saturating_mul(10000)
                .saturating_div(total_transactions) as i128
        } else {
            0
        };

        // Calculate user satisfaction score (based on ratings)
        let mut total_rating = 0u32;
        let mut rating_count = 0u32;
        let _invoices_with_ratings =
            crate::invoice::InvoiceStorage::get_invoices_with_ratings_count(env);

        // Get paid invoices for rating calculation
        let paid_invoices =
            crate::invoice::InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Paid);
        for invoice_id in paid_invoices.iter() {
            if let Some(invoice) = crate::invoice::InvoiceStorage::get_invoice(env, &invoice_id) {
                if let Some(avg_rating) = invoice.average_rating {
                    total_rating = total_rating.saturating_add(avg_rating);
                    rating_count += 1;
                }
            }
        }

        let user_satisfaction_score = if rating_count > 0 {
            total_rating.saturating_div(rating_count)
        } else {
            0
        };

        // Calculate platform efficiency
        let platform_efficiency = {
            let fee_config = crate::profits::PlatformFee::get_config(env);
            fee_config.fee_bps
        };

        Ok(PerformanceMetrics {
            platform_uptime: current_timestamp, // Simplified - would need more complex tracking
            average_settlement_time,
            average_verification_time,
            dispute_resolution_time,
            system_response_time: 0, // Would need system-level tracking
            transaction_success_rate,
            error_rate,
            user_satisfaction_score,
            platform_efficiency,
        })
    }

    /// Generate business report
    pub fn generate_business_report(
        env: &Env,
        business: &Address,
        period: TimePeriod,
    ) -> Result<BusinessReport, QuickLendXError> {
        let current_timestamp = env.ledger().timestamp();
        let (start_date, end_date) = Self::get_period_dates(current_timestamp, period.clone());
        let report_id = AnalyticsStorage::generate_report_id(env);

        // Get business invoices in the period
        let all_invoices = crate::invoice::InvoiceStorage::get_business_invoices(env, business);
        let mut invoices_uploaded = 0u32;
        let mut invoices_funded = 0u32;
        let mut total_volume = 0i128;
        let mut total_funding_time = 0u64;
        let mut successful_invoices = 0u32;
        let mut defaulted_invoices = 0u32;
        let mut category_breakdown = Vec::new(env);
        let mut total_rating = 0u32;
        let mut rating_count = 0u32;

        // Initialize category tracking
        let categories = [
            InvoiceCategory::Services,
            InvoiceCategory::Products,
            InvoiceCategory::Consulting,
            InvoiceCategory::Manufacturing,
            InvoiceCategory::Technology,
            InvoiceCategory::Healthcare,
            InvoiceCategory::Other,
        ];

        for category in categories.iter() {
            category_breakdown.push_back((category.clone(), 0u32));
        }

        for invoice_id in all_invoices.iter() {
            if let Some(invoice) = crate::invoice::InvoiceStorage::get_invoice(env, &invoice_id) {
                if invoice.created_at >= start_date && invoice.created_at <= end_date {
                    invoices_uploaded += 1;
                    total_volume = total_volume.saturating_add(invoice.amount);

                    // Update category breakdown
                    for i in 0..category_breakdown.len() {
                        let (cat, count) = category_breakdown.get(i).unwrap();
                        if cat == invoice.category {
                            category_breakdown.set(i, (cat, count.saturating_add(1)));
                            break;
                        }
                    }

                    // Track funding and success
                    if invoice.status == InvoiceStatus::Funded
                        || invoice.status == InvoiceStatus::Paid
                    {
                        invoices_funded += 1;
                        if let Some(investment) =
                            crate::investment::InvestmentStorage::get_investment_by_invoice(
                                env,
                                &invoice_id,
                            )
                        {
                            let funding_time =
                                investment.funded_at.saturating_sub(invoice.created_at);
                            total_funding_time = total_funding_time.saturating_add(funding_time);
                        }
                    }

                    match invoice.status {
                        InvoiceStatus::Paid => successful_invoices += 1,
                        InvoiceStatus::Defaulted => defaulted_invoices += 1,
                        _ => {}
                    }

                    // Track ratings
                    if let Some(avg_rating) = invoice.average_rating {
                        total_rating = total_rating.saturating_add(avg_rating);
                        rating_count += 1;
                    }
                }
            }
        }

        let average_funding_time = if invoices_funded > 0 {
            total_funding_time.saturating_div(invoices_funded as u64)
        } else {
            0
        };

        let success_rate = if invoices_uploaded > 0 {
            (successful_invoices.saturating_mul(10000)).saturating_div(invoices_uploaded) as i128
        } else {
            0
        };

        let default_rate = if invoices_uploaded > 0 {
            (defaulted_invoices.saturating_mul(10000)).saturating_div(invoices_uploaded) as i128
        } else {
            0
        };

        let rating_average = if rating_count > 0 {
            Some(total_rating.saturating_div(rating_count))
        } else {
            None
        };

        Ok(BusinessReport {
            report_id,
            business_address: business.clone(),
            period,
            start_date,
            end_date,
            invoices_uploaded,
            invoices_funded,
            total_volume,
            average_funding_time,
            success_rate,
            default_rate,
            category_breakdown,
            rating_average,
            total_ratings: rating_count,
            generated_at: current_timestamp,
        })
    }

    /// Generate investor report
    pub fn generate_investor_report(
        env: &Env,
        investor: &Address,
        period: TimePeriod,
    ) -> Result<InvestorReport, QuickLendXError> {
        let current_timestamp = env.ledger().timestamp();
        let (start_date, end_date) = Self::get_period_dates(current_timestamp, period.clone());
        let report_id = AnalyticsStorage::generate_report_id(env);

        // Get investor's investments in the period (simplified)
        let all_investments: Vec<crate::investment::Investment> = Vec::new(env); // Placeholder - would need proper tracking
        let mut investments_made = 0u32;
        let mut total_invested = 0i128;
        let mut total_returns = 0i128;
        let mut successful_investments = 0u32;
        let mut defaulted_investments = 0u32;
        let mut preferred_categories = Vec::new(env);

        // Initialize category tracking
        let categories = [
            InvoiceCategory::Services,
            InvoiceCategory::Products,
            InvoiceCategory::Consulting,
            InvoiceCategory::Manufacturing,
            InvoiceCategory::Technology,
            InvoiceCategory::Healthcare,
            InvoiceCategory::Other,
        ];

        for category in categories.iter() {
            preferred_categories.push_back((category.clone(), 0u32));
        }

        for investment in all_investments.iter() {
            if investment.funded_at >= start_date && investment.funded_at <= end_date {
                investments_made += 1;
                total_invested = total_invested.saturating_add(investment.amount);

                if let Some(invoice) =
                    crate::invoice::InvoiceStorage::get_invoice(env, &investment.invoice_id)
                {
                    // Update category preferences
                    for i in 0..preferred_categories.len() {
                        let (cat, count) = preferred_categories.get(i).unwrap();
                        if cat == invoice.category {
                            preferred_categories.set(i, (cat, count.saturating_add(1)));
                            break;
                        }
                    }

                    match invoice.status {
                        InvoiceStatus::Paid => {
                            successful_investments += 1;
                            let (profit, _) = crate::profits::calculate_profit(
                                env,
                                investment.amount,
                                invoice.amount,
                            );
                            total_returns = total_returns
                                .saturating_add(investment.amount.saturating_add(profit));
                        }
                        InvoiceStatus::Defaulted => defaulted_investments += 1,
                        _ => {}
                    }
                }
            }
        }

        let average_return_rate = if total_invested > 0 {
            let profit = total_returns.saturating_sub(total_invested);
            profit.saturating_mul(10000).saturating_div(total_invested)
        } else {
            0
        };

        let success_rate = if investments_made > 0 {
            (successful_investments.saturating_mul(10000)).saturating_div(investments_made) as i128
        } else {
            0
        };

        let default_rate = if investments_made > 0 {
            (defaulted_investments.saturating_mul(10000)).saturating_div(investments_made) as i128
        } else {
            0
        };

        // Calculate risk tolerance based on investment patterns
        let risk_tolerance = if default_rate > 1000 {
            // > 10%
            100
        } else if default_rate > 500 {
            // > 5%
            75
        } else if default_rate > 200 {
            // > 2%
            50
        } else {
            25
        };

        // Calculate portfolio diversity (simplified)
        let portfolio_diversity = if investments_made > 0 {
            let unique_categories = preferred_categories
                .iter()
                .filter(|(_, count)| *count > 0)
                .count() as u32;
            (unique_categories.saturating_mul(10000)).saturating_div(investments_made) as i128
        } else {
            0
        };

        Ok(InvestorReport {
            report_id,
            investor_address: investor.clone(),
            period,
            start_date,
            end_date,
            investments_made,
            total_invested,
            total_returns,
            average_return_rate,
            success_rate,
            default_rate,
            preferred_categories,
            risk_tolerance,
            portfolio_diversity,
            generated_at: current_timestamp,
        })
    }

    /// Get period dates based on time period
    pub fn get_period_dates(current_timestamp: u64, period: TimePeriod) -> (u64, u64) {
        match period {
            TimePeriod::Daily => {
                let day_start = current_timestamp.saturating_sub(24 * 60 * 60);
                (day_start, current_timestamp)
            }
            TimePeriod::Weekly => {
                let week_start = current_timestamp.saturating_sub(7 * 24 * 60 * 60);
                (week_start, current_timestamp)
            }
            TimePeriod::Monthly => {
                let month_start = current_timestamp.saturating_sub(30 * 24 * 60 * 60);
                (month_start, current_timestamp)
            }
            TimePeriod::Quarterly => {
                let quarter_start = current_timestamp.saturating_sub(90 * 24 * 60 * 60);
                (quarter_start, current_timestamp)
            }
            TimePeriod::Yearly => {
                let year_start = current_timestamp.saturating_sub(365 * 24 * 60 * 60);
                (year_start, current_timestamp)
            }
            TimePeriod::AllTime => (0, current_timestamp),
        }
    }
}
