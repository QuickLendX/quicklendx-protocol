use soroban_sdk::{
    contracttype, symbol_short, vec, Address, BytesN, Env, Map, String, Vec, symbol,
};
use crate::invoice::{Invoice, InvoiceStatus, InvoiceStorage};
use crate::bid::{Bid, BidStatus, BidStorage};
use crate::investment::{Investment, InvestmentStatus, InvestmentStorage};
use crate::payments::{Escrow, EscrowStorage, EscrowStatus};
use crate::verification::{BusinessVerificationStorage, BusinessVerificationStatus};
use crate::errors::QuickLendXError;

/// Analytics time period for reporting
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AnalyticsPeriod {
    Day,
    Week,
    Month,
    Quarter,
    Year,
    AllTime,
}

/// Platform metrics structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct PlatformMetrics {
    pub total_invoices: u32,
    pub total_volume: i128,
    pub total_funded_invoices: u32,
    pub total_funded_volume: i128,
    pub total_paid_invoices: u32,
    pub total_paid_volume: i128,
    pub total_defaulted_invoices: u32,
    pub total_defaulted_volume: i128,
    pub average_invoice_amount: i128,
    pub average_funding_time: u64,
    pub average_settlement_time: u64,
    pub platform_fee_total: i128,
    pub investor_returns_total: i128,
    pub active_businesses: u32,
    pub active_investors: u32,
    pub total_ratings: u32,
    pub average_rating: f64,
    pub timestamp: u64,
}

/// Business analytics structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct BusinessAnalytics {
    pub business: Address,
    pub total_invoices: u32,
    pub total_volume: i128,
    pub funded_invoices: u32,
    pub funded_volume: i128,
    pub paid_invoices: u32,
    pub paid_volume: i128,
    pub defaulted_invoices: u32,
    pub defaulted_volume: i128,
    pub average_invoice_amount: i128,
    pub average_rating: f64,
    pub total_ratings: u32,
    pub on_time_payment_rate: f64,
    pub average_funding_time: u64,
    pub last_activity: u64,
}

/// Investor analytics structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct InvestorAnalytics {
    pub investor: Address,
    pub total_investments: u32,
    pub total_invested_amount: i128,
    pub total_returns: i128,
    pub active_investments: u32,
    pub completed_investments: u32,
    pub defaulted_investments: u32,
    pub average_investment_amount: i128,
    pub average_return_rate: f64,
    pub total_ratings_given: u32,
    pub average_rating_given: f64,
    pub last_activity: u64,
}

/// Time-based analytics structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct TimeBasedAnalytics {
    pub period: AnalyticsPeriod,
    pub start_timestamp: u64,
    pub end_timestamp: u64,
    pub invoices_created: u32,
    pub invoices_funded: u32,
    pub invoices_paid: u32,
    pub invoices_defaulted: u32,
    pub total_volume: i128,
    pub funded_volume: i128,
    pub paid_volume: i128,
    pub defaulted_volume: i128,
    pub new_businesses: u32,
    pub new_investors: u32,
    pub platform_fees: i128,
    pub investor_returns: i128,
}

/// Analytics storage structure
pub struct AnalyticsStorage;

impl AnalyticsStorage {
    /// Store platform metrics
    pub fn store_platform_metrics(env: &Env, metrics: &PlatformMetrics) {
        let key = symbol_short!("plat_metrics");
        env.storage().instance().set(&key, metrics);
    }

    /// Get platform metrics
    pub fn get_platform_metrics(env: &Env) -> Option<PlatformMetrics> {
        let key = symbol_short!("plat_metrics");
        env.storage().instance().get(&key)
    }

    /// Store business analytics
    pub fn store_business_analytics(env: &Env, business: &Address, analytics: &BusinessAnalytics) {
        let key = (symbol_short!("bus_analytics"), business.clone());
        env.storage().instance().set(&key, analytics);
    }

    /// Get business analytics
    pub fn get_business_analytics(env: &Env, business: &Address) -> Option<BusinessAnalytics> {
        let key = (symbol_short!("bus_analytics"), business.clone());
        env.storage().instance().get(&key)
    }

    /// Store investor analytics
    pub fn store_investor_analytics(env: &Env, investor: &Address, analytics: &InvestorAnalytics) {
        let key = (symbol_short!("inv_analytics"), investor.clone());
        env.storage().instance().set(&key, analytics);
    }

    /// Get investor analytics
    pub fn get_investor_analytics(env: &Env, investor: &Address) -> Option<InvestorAnalytics> {
        let key = (symbol_short!("inv_analytics"), investor.clone());
        env.storage().instance().get(&key)
    }

    /// Store time-based analytics
    pub fn store_time_analytics(env: &Env, period: &AnalyticsPeriod, analytics: &TimeBasedAnalytics) {
        let key = (symbol_short!("time_analytics"), period.clone());
        env.storage().instance().set(&key, analytics);
    }

    /// Get time-based analytics
    pub fn get_time_analytics(env: &Env, period: &AnalyticsPeriod) -> Option<TimeBasedAnalytics> {
        let key = (symbol_short!("time_analytics"), period.clone());
        env.storage().instance().get(&key)
    }

    /// Get all business addresses
    pub fn get_all_businesses(env: &Env) -> Vec<Address> {
        let verified = BusinessVerificationStorage::get_verified_businesses(env);
        let pending = BusinessVerificationStorage::get_pending_businesses(env);
        let rejected = BusinessVerificationStorage::get_rejected_businesses(env);
        
        let mut all_businesses = vec![env];
        for business in verified.iter() {
            all_businesses.push_back(business);
        }
        for business in pending.iter() {
            all_businesses.push_back(business);
        }
        for business in rejected.iter() {
            all_businesses.push_back(business);
        }
        all_businesses
    }

    /// Get all investor addresses
    pub fn get_all_investors(env: &Env) -> Vec<Address> {
        let mut investors = vec![env];
        let mut seen_investors = Map::new(env);
        
        // Get investors from bids
        let all_bids = BidStorage::get_all_bids(env);
        for bid_id in all_bids.iter() {
            if let Some(bid) = BidStorage::get_bid(env, &bid_id) {
                if !seen_investors.contains_key(&bid.investor) {
                    investors.push_back(bid.investor.clone());
                    seen_investors.set(&bid.investor, &true);
                }
            }
        }
        
        // Get investors from investments
        let all_investments = InvestmentStorage::get_all_investments(env);
        for investment_id in all_investments.iter() {
            if let Some(investment) = InvestmentStorage::get_investment(env, &investment_id) {
                if !seen_investors.contains_key(&investment.investor) {
                    investors.push_back(investment.investor.clone());
                    seen_investors.set(&investment.investor, &true);
                }
            }
        }
        
        investors
    }
}

/// Analytics functions
pub struct Analytics;

impl Analytics {
    /// Calculate platform metrics
    pub fn calculate_platform_metrics(env: &Env) -> PlatformMetrics {
        let current_timestamp = env.ledger().timestamp();
        
        // Get all invoices by status
        let pending = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Pending);
        let verified = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Verified);
        let funded = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Funded);
        let paid = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Paid);
        let defaulted = InvoiceStorage::get_invoices_by_status(env, &InvoiceStatus::Defaulted);
        
        // Calculate totals
        let total_invoices = pending.len() + verified.len() + funded.len() + paid.len() + defaulted.len();
        
        // Calculate volumes
        let mut total_volume = 0i128;
        let mut funded_volume = 0i128;
        let mut paid_volume = 0i128;
        let mut defaulted_volume = 0i128;
        let mut platform_fee_total = 0i128;
        let mut investor_returns_total = 0i128;
        let mut total_ratings = 0u32;
        let mut rating_sum = 0u32;
        
        // Process all invoices
        for status_invoices in [pending, verified, funded, paid, defaulted].iter() {
            for invoice_id in status_invoices.iter() {
                if let Some(invoice) = InvoiceStorage::get_invoice(env, &invoice_id) {
                    total_volume += invoice.amount;
                    
                    match invoice.status {
                        InvoiceStatus::Funded => {
                            funded_volume += invoice.funded_amount;
                        },
                        InvoiceStatus::Paid => {
                            paid_volume += invoice.amount;
                            // Calculate platform fees and returns (simplified)
                            if let Some(settled_at) = invoice.settled_at {
                                let funding_time = invoice.funded_at.unwrap_or(0);
                                let settlement_time = settled_at - funding_time;
                                // Assume 2% platform fee and 8% investor return
                                let platform_fee = invoice.amount * 2 / 100;
                                let investor_return = invoice.amount * 8 / 100;
                                platform_fee_total += platform_fee;
                                investor_returns_total += investor_return;
                            }
                        },
                        InvoiceStatus::Defaulted => {
                            defaulted_volume += invoice.amount;
                        },
                        _ => {}
                    }
                    
                    // Calculate rating statistics
                    total_ratings += invoice.total_ratings;
                    if let Some(avg_rating) = invoice.average_rating {
                        rating_sum += avg_rating * invoice.total_ratings;
                    }
                }
            }
        }
        
        // Calculate averages
        let average_invoice_amount = if total_invoices > 0 {
            total_volume / total_invoices as i128
        } else {
            0
        };
        
        let average_rating = if total_ratings > 0 {
            rating_sum as f64 / total_ratings as f64
        } else {
            0.0
        };
        
        // Get active users
        let active_businesses = BusinessVerificationStorage::get_verified_businesses(env).len();
        let active_investors = AnalyticsStorage::get_all_investors(env).len();
        
        // Calculate average times (simplified)
        let average_funding_time = 86400u64; // 1 day default
        let average_settlement_time = 2592000u64; // 30 days default
        
        PlatformMetrics {
            total_invoices: total_invoices as u32,
            total_volume,
            total_funded_invoices: funded.len() as u32,
            total_funded_volume: funded_volume,
            total_paid_invoices: paid.len() as u32,
            total_paid_volume: paid_volume,
            total_defaulted_invoices: defaulted.len() as u32,
            total_defaulted_volume: defaulted_volume,
            average_invoice_amount,
            average_funding_time,
            average_settlement_time,
            platform_fee_total,
            investor_returns_total,
            active_businesses: active_businesses as u32,
            active_investors: active_investors as u32,
            total_ratings,
            average_rating,
            timestamp: current_timestamp,
        }
    }
    
    /// Calculate business analytics
    pub fn calculate_business_analytics(env: &Env, business: &Address) -> Option<BusinessAnalytics> {
        let business_invoices = InvoiceStorage::get_business_invoices(env, business);
        if business_invoices.is_empty() {
            return None;
        }
        
        let mut total_invoices = 0u32;
        let mut total_volume = 0i128;
        let mut funded_invoices = 0u32;
        let mut funded_volume = 0i128;
        let mut paid_invoices = 0u32;
        let mut paid_volume = 0i128;
        let mut defaulted_invoices = 0u32;
        let mut defaulted_volume = 0i128;
        let mut total_ratings = 0u32;
        let mut rating_sum = 0u32;
        let mut last_activity = 0u64;
        
        for invoice_id in business_invoices.iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(env, &invoice_id) {
                total_invoices += 1;
                total_volume += invoice.amount;
                last_activity = last_activity.max(invoice.created_at);
                
                match invoice.status {
                    InvoiceStatus::Funded => {
                        funded_invoices += 1;
                        funded_volume += invoice.funded_amount;
                    },
                    InvoiceStatus::Paid => {
                        paid_invoices += 1;
                        paid_volume += invoice.amount;
                    },
                    InvoiceStatus::Defaulted => {
                        defaulted_invoices += 1;
                        defaulted_volume += invoice.amount;
                    },
                    _ => {}
                }
                
                total_ratings += invoice.total_ratings;
                if let Some(avg_rating) = invoice.average_rating {
                    rating_sum += avg_rating * invoice.total_ratings;
                }
            }
        }
        
        let average_invoice_amount = if total_invoices > 0 {
            total_volume / total_invoices as i128
        } else {
            0
        };
        
        let average_rating = if total_ratings > 0 {
            rating_sum as f64 / total_ratings as f64
        } else {
            0.0
        };
        
        let on_time_payment_rate = if paid_invoices > 0 {
            (paid_invoices as f64 / (paid_invoices + defaulted_invoices) as f64) * 100.0
        } else {
            0.0
        };
        
        let average_funding_time = 86400u64; // Simplified calculation
        
        Some(BusinessAnalytics {
            business: business.clone(),
            total_invoices,
            total_volume,
            funded_invoices,
            funded_volume,
            paid_invoices,
            paid_volume,
            defaulted_invoices,
            defaulted_volume,
            average_invoice_amount,
            average_rating,
            total_ratings,
            on_time_payment_rate,
            average_funding_time,
            last_activity,
        })
    }
    
    /// Calculate investor analytics
    pub fn calculate_investor_analytics(env: &Env, investor: &Address) -> Option<InvestorAnalytics> {
        let mut total_investments = 0u32;
        let mut total_invested_amount = 0i128;
        let mut total_returns = 0i128;
        let mut active_investments = 0u32;
        let mut completed_investments = 0u32;
        let mut defaulted_investments = 0u32;
        let mut total_ratings_given = 0u32;
        let mut rating_sum = 0u32;
        let mut last_activity = 0u64;
        
        // Get investments by this investor
        let all_investments = InvestmentStorage::get_all_investments(env);
        for investment_id in all_investments.iter() {
            if let Some(investment) = InvestmentStorage::get_investment(env, &investment_id) {
                if investment.investor == *investor {
                    total_investments += 1;
                    total_invested_amount += investment.amount;
                    last_activity = last_activity.max(investment.funded_at);
                    
                    match investment.status {
                        InvestmentStatus::Active => {
                            active_investments += 1;
                        },
                        InvestmentStatus::Completed => {
                            completed_investments += 1;
                            // Simplified return calculation
                            total_returns += investment.amount * 108 / 100; // 8% return
                        },
                        InvestmentStatus::Defaulted => {
                            defaulted_investments += 1;
                        },
                    }
                }
            }
        }
        
        // Get ratings given by this investor
        let all_invoices = InvoiceStorage::get_all_invoices(env);
        for invoice_id in all_invoices.iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(env, &invoice_id) {
                for rating in invoice.ratings.iter() {
                    if rating.rated_by == *investor {
                        total_ratings_given += 1;
                        rating_sum += rating.rating;
                    }
                }
            }
        }
        
        if total_investments == 0 {
            return None;
        }
        
        let average_investment_amount = total_invested_amount / total_investments as i128;
        let average_return_rate = if total_invested_amount > 0 {
            (total_returns as f64 / total_invested_amount as f64) * 100.0
        } else {
            0.0
        };
        
        let average_rating_given = if total_ratings_given > 0 {
            rating_sum as f64 / total_ratings_given as f64
        } else {
            0.0
        };
        
        Some(InvestorAnalytics {
            investor: investor.clone(),
            total_investments,
            total_invested_amount,
            total_returns,
            active_investments,
            completed_investments,
            defaulted_investments,
            average_investment_amount,
            average_return_rate,
            total_ratings_given,
            average_rating_given,
            last_activity,
        })
    }
    
    /// Calculate time-based analytics
    pub fn calculate_time_analytics(env: &Env, period: &AnalyticsPeriod) -> TimeBasedAnalytics {
        let current_timestamp = env.ledger().timestamp();
        let (start_timestamp, end_timestamp) = Self::get_period_timestamps(env, period);
        
        let mut invoices_created = 0u32;
        let mut invoices_funded = 0u32;
        let mut invoices_paid = 0u32;
        let mut invoices_defaulted = 0u32;
        let mut total_volume = 0i128;
        let mut funded_volume = 0i128;
        let mut paid_volume = 0i128;
        let mut defaulted_volume = 0i128;
        let mut new_businesses = 0u32;
        let mut new_investors = 0u32;
        let mut platform_fees = 0i128;
        let mut investor_returns = 0i128;
        
        // Process all invoices
        let all_invoices = InvoiceStorage::get_all_invoices(env);
        for invoice_id in all_invoices.iter() {
            if let Some(invoice) = InvoiceStorage::get_invoice(env, &invoice_id) {
                if invoice.created_at >= start_timestamp && invoice.created_at <= end_timestamp {
                    invoices_created += 1;
                    total_volume += invoice.amount;
                }
                
                if let Some(funded_at) = invoice.funded_at {
                    if funded_at >= start_timestamp && funded_at <= end_timestamp {
                        invoices_funded += 1;
                        funded_volume += invoice.funded_amount;
                    }
                }
                
                if let Some(settled_at) = invoice.settled_at {
                    if settled_at >= start_timestamp && settled_at <= end_timestamp {
                        invoices_paid += 1;
                        paid_volume += invoice.amount;
                        // Simplified fee calculation
                        platform_fees += invoice.amount * 2 / 100;
                        investor_returns += invoice.amount * 8 / 100;
                    }
                }
                
                if invoice.status == InvoiceStatus::Defaulted {
                    // Check if defaulted during this period
                    if invoice.due_date >= start_timestamp && invoice.due_date <= end_timestamp {
                        invoices_defaulted += 1;
                        defaulted_volume += invoice.amount;
                    }
                }
            }
        }
        
        // Count new businesses and investors (simplified)
        new_businesses = BusinessVerificationStorage::get_verified_businesses(env).len() as u32;
        new_investors = AnalyticsStorage::get_all_investors(env).len() as u32;
        
        TimeBasedAnalytics {
            period: period.clone(),
            start_timestamp,
            end_timestamp,
            invoices_created,
            invoices_funded,
            invoices_paid,
            invoices_defaulted,
            total_volume,
            funded_volume,
            paid_volume,
            defaulted_volume,
            new_businesses,
            new_investors,
            platform_fees,
            investor_returns,
        }
    }
    
    /// Get period timestamps
    fn get_period_timestamps(env: &Env, period: &AnalyticsPeriod) -> (u64, u64) {
        let current_timestamp = env.ledger().timestamp();
        
        match period {
            AnalyticsPeriod::Day => {
                let start = current_timestamp - 86400; // 24 hours ago
                (start, current_timestamp)
            },
            AnalyticsPeriod::Week => {
                let start = current_timestamp - 604800; // 7 days ago
                (start, current_timestamp)
            },
            AnalyticsPeriod::Month => {
                let start = current_timestamp - 2592000; // 30 days ago
                (start, current_timestamp)
            },
            AnalyticsPeriod::Quarter => {
                let start = current_timestamp - 7776000; // 90 days ago
                (start, current_timestamp)
            },
            AnalyticsPeriod::Year => {
                let start = current_timestamp - 31536000; // 365 days ago
                (start, current_timestamp)
            },
            AnalyticsPeriod::AllTime => {
                (0, current_timestamp)
            },
        }
    }
    
    /// Generate business report
    pub fn generate_business_report(env: &Env, business: &Address) -> Option<String> {
        if let Some(analytics) = Self::calculate_business_analytics(env, business) {
            let report = format!(
                "Business Report for {}\n\
                Total Invoices: {}\n\
                Total Volume: {}\n\
                Funded Invoices: {}\n\
                Paid Invoices: {}\n\
                Defaulted Invoices: {}\n\
                Average Invoice Amount: {}\n\
                Average Rating: {:.2}\n\
                On-time Payment Rate: {:.2}%\n\
                Last Activity: {}",
                business.to_string(),
                analytics.total_invoices,
                analytics.total_volume,
                analytics.funded_invoices,
                analytics.paid_invoices,
                analytics.defaulted_invoices,
                analytics.average_invoice_amount,
                analytics.average_rating,
                analytics.on_time_payment_rate,
                analytics.last_activity
            );
            Some(report)
        } else {
            None
        }
    }
    
    /// Generate investor report
    pub fn generate_investor_report(env: &Env, investor: &Address) -> Option<String> {
        if let Some(analytics) = Self::calculate_investor_analytics(env, investor) {
            let report = format!(
                "Investor Report for {}\n\
                Total Investments: {}\n\
                Total Invested Amount: {}\n\
                Total Returns: {}\n\
                Active Investments: {}\n\
                Completed Investments: {}\n\
                Defaulted Investments: {}\n\
                Average Investment Amount: {}\n\
                Average Return Rate: {:.2}%\n\
                Total Ratings Given: {}\n\
                Average Rating Given: {:.2}\n\
                Last Activity: {}",
                investor.to_string(),
                analytics.total_investments,
                analytics.total_invested_amount,
                analytics.total_returns,
                analytics.active_investments,
                analytics.completed_investments,
                analytics.defaulted_investments,
                analytics.average_investment_amount,
                analytics.average_return_rate,
                analytics.total_ratings_given,
                analytics.average_rating_given,
                analytics.last_activity
            );
            Some(report)
        } else {
            None
        }
    }
    
    /// Generate platform report
    pub fn generate_platform_report(env: &Env) -> String {
        let metrics = Self::calculate_platform_metrics(env);
        
        format!(
            "Platform Report\n\
            Total Invoices: {}\n\
            Total Volume: {}\n\
            Funded Invoices: {}\n\
            Paid Invoices: {}\n\
            Defaulted Invoices: {}\n\
            Average Invoice Amount: {}\n\
            Platform Fee Total: {}\n\
            Investor Returns Total: {}\n\
            Active Businesses: {}\n\
            Active Investors: {}\n\
            Total Ratings: {}\n\
            Average Rating: {:.2}\n\
            Timestamp: {}",
            metrics.total_invoices,
            metrics.total_volume,
            metrics.total_funded_invoices,
            metrics.total_paid_invoices,
            metrics.total_defaulted_invoices,
            metrics.average_invoice_amount,
            metrics.platform_fee_total,
            metrics.investor_returns_total,
            metrics.active_businesses,
            metrics.active_investors,
            metrics.total_ratings,
            metrics.average_rating,
            metrics.timestamp
        )
    }
} 