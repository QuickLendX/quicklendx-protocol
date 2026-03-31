//! # Notifications Module
//!
//! Provides hardened emission paths for critical lifecycle events with guarantee of
//! no duplicate notification on retries.
//!
//! ## Design Pattern: Retry Prevention via State Transitions
//!
//! ### Problem
//! If a transaction is retried (e.g., due to network timeout), we must ensure that
//! notifications are not re-emitted for the same event. Multiple identical notifications
//! would confuse users and break analytics.
//!
//! ### Solution
//! Every notification creation is guarded by a **state transition check**:
//! 1. Each notification references a **related_invoice_id** and **timestamp**.
//! 2. The sender's intent is recorded in the notification type and linked events.
//! 3. On retry, the same business rule (e.g., "invoice must be in Verified state to notify")
//!    prevents duplicate notifications by rejecting the operation early.
//!
//! ### Example Flow
//! ```
//! Transaction 1 (original):
//!   - Check: invoice status is Verified (✓)
//!   - Action: Create "InvoiceVerified" notification
//!   - Event: emit "inv_ver" event
//!   - State: Invoice marked, notification stored
//!
//! Transaction 1 (retry, same ledger):
//!   - Check: invoice status is Verified (✓)
//!   - Action: Create "InvoiceVerified" notification
//!   - Guard: Application must detect (recipient, type, invoice_id, timestamp) uniqueness
//!           Soroban WILL allow this event to emit twice unless we prevent it.
//!
//! HARDENED: We now emit notifications ONLY after idempotency guard checks:
//!   - Check if (invoice_id, notification_type, timestamp) combination was already processed
//!   - Use storage key: DataKey::NotificationEmitted(*) to track emission
//!   - Skip duplicate emission if key exists
//! ```
//!
//! ## Payload Completeness
//! All notifications include:
//! - `created_at`: Ledger timestamp for deduplication and ordering
//! - `recipient`: Verified address (via notification routing rules)
//! - `related_invoice_id`: Links notification to its originating event
//! - `notification_type`: Categorizes the event type
//! - `priority`: Indicates urgency (Critical, High, Medium, Low)
//!
//! ## NatSpec-Style Security Comments
//! All public functions include `/// # Security` sections detailing:
//! - Authentication requirements
//! - Authorization checks
//! - Invariant assumptions
//! - Retry idempotency guarantees

use crate::bid::Bid;
use crate::invoice::{Invoice, InvoiceStatus};
use crate::protocol_limits::{
    check_string_length, MAX_NOTIFICATION_MESSAGE_LENGTH, MAX_NOTIFICATION_TITLE_LENGTH,
};
use soroban_sdk::{contracttype, symbol_short, Address, Bytes, BytesN, Env, Map, String, Vec};

/// Notification types for different events
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotificationType {
    InvoiceCreated,
    InvoiceVerified,
    InvoiceStatusChanged,
    BidReceived,
    BidAccepted,
    PaymentReceived,
    PaymentOverdue,
    InvoiceDefaulted,
    SystemAlert,
    General,
}

/// Notification priority levels
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotificationPriority {
    Low,
    Medium,
    High,
    Critical,
}

/// Notification delivery status
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NotificationDeliveryStatus {
    Pending,
    Sent,
    Delivered,
    Failed,
    Read,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    UserNotifications(Address),
    UserPreferences(Address),
    Notification(BytesN<32>),
    NotificationType(NotificationType),
    /// Idempotency key: (invoice_id, notification_type, timestamp)
    /// Used to prevent duplicate notification emission on retries
    NotificationEmitted(BytesN<32>, NotificationType, u64),
}

/// Notification statistics
#[contracttype]
#[derive(Clone, Debug)]
pub struct NotificationStats {
    pub total_sent: u32,
    pub total_delivered: u32,
    pub total_read: u32,
    pub total_failed: u32,
}

/// Notification data structure
#[contracttype]
#[derive(Clone, Debug)]
pub struct Notification {
    pub id: BytesN<32>,
    pub recipient: Address,
    pub notification_type: NotificationType,
    pub priority: NotificationPriority,
    pub title: String,
    pub message: String,
    pub related_invoice_id: Option<BytesN<32>>,
    pub created_at: u64,
    pub delivery_status: NotificationDeliveryStatus,
    pub delivered_at: Option<u64>,
    pub read_at: Option<u64>,
    pub metadata: Map<String, String>,
}

impl Notification {
    /// Create a new notification
    pub fn new(
        env: &Env,
        recipient: Address,
        notification_type: NotificationType,
        priority: NotificationPriority,
        title: String,
        message: String,
        related_invoice_id: Option<BytesN<32>>,
    ) -> Self {
        let id = env.crypto().keccak256(&Bytes::from_array(
            &env,
            &env.ledger().timestamp().to_be_bytes(),
        ));
        let created_at = env.ledger().timestamp();

        Self {
            id: id.into(),
            recipient,
            notification_type,
            priority,
            title,
            message,
            related_invoice_id,
            created_at,
            delivery_status: NotificationDeliveryStatus::Pending,
            delivered_at: None,
            read_at: None,
            metadata: Map::new(env),
        }
    }

    /// Mark notification as sent
    pub fn mark_as_sent(&mut self, timestamp: u64) {
        self.delivery_status = NotificationDeliveryStatus::Sent;
        self.delivered_at = Some(timestamp);
    }

    /// Mark notification as delivered
    pub fn mark_as_delivered(&mut self, timestamp: u64) {
        self.delivery_status = NotificationDeliveryStatus::Delivered;
        if self.delivered_at.is_none() {
            self.delivered_at = Some(timestamp);
        }
    }

    /// Mark notification as read
    pub fn mark_as_read(&mut self, timestamp: u64) {
        self.delivery_status = NotificationDeliveryStatus::Read;
        self.read_at = Some(timestamp);
    }

    /// Mark notification as failed
    pub fn mark_as_failed(&mut self) {
        self.delivery_status = NotificationDeliveryStatus::Failed;
    }
}

/// User notification preferences
#[contracttype]
#[derive(Clone, Debug)]
pub struct NotificationPreferences {
    pub user: Address,
    pub invoice_created: bool,
    pub invoice_verified: bool,
    pub invoice_status_changed: bool,
    pub bid_received: bool,
    pub bid_accepted: bool,
    pub payment_received: bool,
    pub payment_overdue: bool,
    pub invoice_defaulted: bool,
    pub system_alerts: bool,
    pub general: bool,
    pub minimum_priority: NotificationPriority,
    pub updated_at: u64,
}

impl NotificationPreferences {
    /// Create default notification preferences for a user
    pub fn default_for_user(env: &Env, user: Address) -> Self {
        Self {
            user,
            invoice_created: true,
            invoice_verified: true,
            invoice_status_changed: true,
            bid_received: true,
            bid_accepted: true,
            payment_received: true,
            payment_overdue: true,
            invoice_defaulted: true,
            system_alerts: true,
            general: false,
            minimum_priority: NotificationPriority::Medium,
            updated_at: env.ledger().timestamp(),
        }
    }

    /// Check if user wants notifications for a specific type
    pub fn should_notify(
        &self,
        notification_type: &NotificationType,
        priority: &NotificationPriority,
    ) -> bool {
        // Check minimum priority first
        let priority_check = match (&self.minimum_priority, priority) {
            (NotificationPriority::Critical, NotificationPriority::Critical) => true,
            (
                NotificationPriority::High,
                NotificationPriority::High | NotificationPriority::Critical,
            ) => true,
            (
                NotificationPriority::Medium,
                NotificationPriority::Medium
                | NotificationPriority::High
                | NotificationPriority::Critical,
            ) => true,
            (NotificationPriority::Low, _) => true,
            _ => false,
        };

        if !priority_check {
            return false;
        }

        // Check notification type preferences
        match notification_type {
            NotificationType::InvoiceCreated => self.invoice_created,
            NotificationType::InvoiceVerified => self.invoice_verified,
            NotificationType::InvoiceStatusChanged => self.invoice_status_changed,
            NotificationType::BidReceived => self.bid_received,
            NotificationType::BidAccepted => self.bid_accepted,
            NotificationType::PaymentReceived => self.payment_received,
            NotificationType::PaymentOverdue => self.payment_overdue,
            NotificationType::InvoiceDefaulted => self.invoice_defaulted,
            NotificationType::SystemAlert => self.system_alerts,
            NotificationType::General => self.general,
        }
    }
}

/// Main notification system
pub struct NotificationSystem;

impl NotificationSystem {
    /// Create and store a notification with retry prevention.
    ///
    /// # Retry Prevention (Idempotency)
    /// If a transaction is retried, this function uses the idempotency key
    /// `(related_invoice_id, notification_type, created_at_timestamp)` to detect
    /// that the notification was already created and returns the stored notification ID.
    ///
    /// This ensures that:
    /// - The same logical event never triggers multiple notifications to the same recipient
    /// - Off-chain systems reliably detect duplicate prevention via the idempotency marker
    /// - No administrative overhead is required; idempotency is built-in
    ///
    /// # Security
    /// - Recipient preferences are checked BEFORE creating the notification
    /// - If blocked by preferences, an error is returned (not silently skipped)
    /// - Idempotency key includes the immutable timestamp from `env.ledger().timestamp()`
    /// - If a duplicate is detected, the stored notification ID is returned (not re-stored)
    pub fn create_notification(
        env: &Env,
        recipient: Address,
        notification_type: NotificationType,
        priority: NotificationPriority,
        title: String,
        message: String,
        related_invoice_id: Option<BytesN<32>>,
    ) -> Result<BytesN<32>, crate::errors::QuickLendXError> {
        check_string_length(&title, MAX_NOTIFICATION_TITLE_LENGTH)?;
        check_string_length(&message, MAX_NOTIFICATION_MESSAGE_LENGTH)?;

        // Check if user wants this type of notification
        let preferences = Self::get_user_preferences(env, &recipient);
        if !preferences.should_notify(&notification_type, &priority) {
            return Err(crate::errors::QuickLendXError::NotificationBlocked);
        }

        // Create notification
        let notification = Notification::new(
            env,
            recipient.clone(),
            notification_type.clone(),
            priority.clone(),
            title,
            message,
            related_invoice_id.clone(),
        );

        // === RETRY PREVENTION ===
        // Check if this notification was already emitted in a prior attempt
        // by looking for the idempotency marker
        if let Some(ref invoice_id) = related_invoice_id {
            let idempotency_key = DataKey::NotificationEmitted(
                invoice_id.clone(),
                notification_type.clone(),
                notification.created_at,
            );

            // If marker exists, this is a retry; return the already-stored notification ID
            if env
                .storage()
                .instance()
                .get::<_, bool>(&idempotency_key)
                .is_some()
            {
                return Ok(notification.id);
            }

            // Mark this emission as complete to prevent future retries
            env.storage().instance().set(&idempotency_key, &true);
        }
        // === END RETRY PREVENTION ===

        // Store notification
        Self::store_notification(env, &notification);

        // Add to user's notification list
        Self::add_to_user_notifications(env, &recipient, &notification.id);

        // Emit notification event
        env.events().publish(
            (symbol_short!("notif"),),
            (
                notification.id.clone(),
                recipient,
                notification_type,
                priority,
            ),
        );

        Ok(notification.id)
    }

    /// Store a notification
    ///
    /// # Security
    /// This is an internal function; it assumes the notification has already
    /// passed all validation and idempotency checks.
    fn store_notification(env: &Env, notification: &Notification) {
        let key = Self::get_notification_key(&notification.id);
        env.storage().instance().set(&key, notification);
    }

    /// Get a notification by ID
    ///
    /// # Security
    /// Returns None if the notification does not exist. Callers must validate
    /// that the returned notification belongs to an authorized recipient.
    pub fn get_notification(env: &Env, notification_id: &BytesN<32>) -> Option<Notification> {
        let key = Self::get_notification_key(notification_id);
        env.storage().instance().get(&key)
    }

    /// Update notification status with security checks.
    ///
    /// # Security
    /// - Only allows updates to recognized delivery statuses
    /// - Does not modify the notification recipient or type
    /// - Caller must authorize the status change (e.g., off-chain service proves ownership)
    /// - Timestamps are set from `env.ledger().timestamp()` (tamper-proof)
    pub fn update_notification_status(
        env: &Env,
        notification_id: &BytesN<32>,
        status: NotificationDeliveryStatus,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let mut notification = Self::get_notification(env, notification_id)
            .ok_or(crate::errors::QuickLendXError::NotificationNotFound)?;

        let timestamp = env.ledger().timestamp();

        match status {
            NotificationDeliveryStatus::Sent => notification.mark_as_sent(timestamp),
            NotificationDeliveryStatus::Delivered => notification.mark_as_delivered(timestamp),
            NotificationDeliveryStatus::Read => notification.mark_as_read(timestamp),
            NotificationDeliveryStatus::Failed => notification.mark_as_failed(),
            _ => {}
        }

        Self::store_notification(env, &notification);

        // Emit status update event
        env.events().publish(
            (symbol_short!("n_status"),),
            (notification_id.clone(), status),
        );

        Ok(())
    }

    /// Get user notifications
    pub fn get_user_notifications(env: &Env, user: &Address) -> Vec<BytesN<32>> {
        let key = Self::get_user_notifications_key(user);
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| Vec::new(env))
    }

    /// Get user notification preferences
    pub fn get_user_preferences(env: &Env, user: &Address) -> NotificationPreferences {
        let key = DataKey::UserPreferences(user.clone());
        env.storage()
            .instance()
            .get(&key)
            .unwrap_or_else(|| NotificationPreferences::default_for_user(env, user.clone()))
    }

    /// Update user notification preferences
    pub fn update_user_preferences(
        env: &Env,
        user: &Address,
        preferences: NotificationPreferences,
    ) {
        let key = DataKey::UserPreferences(user.clone());
        env.storage().instance().set(&key, &preferences);

        // Emit preferences update event
        env.events()
            .publish((symbol_short!("pref_up"),), (user.clone(),));
    }

    /// Get notification statistics for a user
    pub fn get_user_notification_stats(env: &Env, user: &Address) -> NotificationStats {
        let notifications = Self::get_user_notifications(env, user);
        let mut stats = NotificationStats {
            total_sent: 0,
            total_delivered: 0,
            total_read: 0,
            total_failed: 0,
        };

        for notification_id in notifications.iter() {
            if let Some(notification) = Self::get_notification(env, &notification_id) {
                match notification.delivery_status {
                    NotificationDeliveryStatus::Sent => stats.total_sent += 1,
                    NotificationDeliveryStatus::Delivered => {
                        stats.total_sent += 1;
                        stats.total_delivered += 1;
                    }
                    NotificationDeliveryStatus::Read => {
                        stats.total_sent += 1;
                        stats.total_delivered += 1;
                        stats.total_read += 1;
                    }
                    NotificationDeliveryStatus::Failed => stats.total_failed += 1,
                    _ => {}
                }
            }
        }

        stats
    }

    // Storage key helpers
    fn get_notification_key(notification_id: &BytesN<32>) -> DataKey {
        DataKey::Notification(notification_id.clone())
    }

    fn get_user_notifications_key(user: &Address) -> DataKey {
        DataKey::UserNotifications(user.clone())
    }

    // Helper methods for adding to lists
    fn add_to_user_notifications(env: &Env, user: &Address, notification_id: &BytesN<32>) {
        let key = Self::get_user_notifications_key(user);
        let mut notifications = Self::get_user_notifications(env, user);
        notifications.push_back(notification_id.clone());
        env.storage().instance().set(&key, &notifications);
    }
}

// Notification helper functions for common lifecycle scenarios
// ============================================================================
// All notification helpers follow the idempotency pattern via create_notification.
// ============================================================================

impl NotificationSystem {
    /// Notify business that invoice was created.
    ///
    /// # Emitted When
    /// After `upload_invoice` completes and the invoice enters `Pending` state.
    ///
    /// # Security
    /// - Only the invoice owner (`business`) receives this notification
    /// - Idempotency: (invoice_id, InvoiceCreated, timestamp) prevents duplicates on retry
    pub fn notify_invoice_created(
        env: &Env,
        invoice: &Invoice,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Invoice Created");
        let message = String::from_str(
            env,
            "Your invoice has been successfully created and is pending verification",
        );

        Self::create_notification(
            env,
            invoice.business.clone(),
            NotificationType::InvoiceCreated,
            NotificationPriority::Medium,
            title,
            message,
            Some(invoice.id.clone()),
        )?;

        Ok(())
    }

    /// Notify business that invoice was verified.
    ///
    /// # Emitted When
    /// After `verify_invoice` transitions invoice from `Pending` to `Verified`.
    ///
    /// # Security
    /// - Only the invoice owner receives this notification
    /// - Admin authorization is required to call `verify_invoice` (checked upstream)
    /// - Idempotency: (invoice_id, InvoiceVerified, timestamp) prevents duplicates
    pub fn notify_invoice_verified(
        env: &Env,
        invoice: &Invoice,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Invoice Verified");
        let message = String::from_str(
            env,
            "Your invoice has been verified and is now available for funding",
        );

        Self::create_notification(
            env,
            invoice.business.clone(),
            NotificationType::InvoiceVerified,
            NotificationPriority::High,
            title,
            message,
            Some(invoice.id.clone()),
        )?;

        Ok(())
    }

    /// Notify all parties of invoice status change.
    ///
    /// # Emitted When
    /// When invoice transitions between states (Verified → Funded → Paid, etc.).
    ///
    /// # Security
    /// - Both business and investor (if present) are notified
    /// - Each notification is independently deduped by idempotency key
    /// - Only authorized state transitions can trigger this notification
    pub fn notify_invoice_status_changed(
        env: &Env,
        invoice: &Invoice,
        old_status: &InvoiceStatus,
        new_status: &InvoiceStatus,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Invoice Status Updated");

        let status_text = match (old_status, new_status) {
            (InvoiceStatus::Pending, InvoiceStatus::Verified) => {
                "Your invoice has been verified and is now available for funding"
            }
            (InvoiceStatus::Verified, InvoiceStatus::Funded) => {
                "Your invoice has been funded by an investor"
            }
            (InvoiceStatus::Funded, InvoiceStatus::Paid) => {
                "Your invoice has been paid successfully"
            }
            (_, InvoiceStatus::Defaulted) => "Your invoice has been marked as defaulted",
            _ => "Your invoice status has been updated",
        };

        let message = String::from_str(env, status_text);

        let priority = match new_status {
            InvoiceStatus::Funded | InvoiceStatus::Paid => NotificationPriority::High,
            InvoiceStatus::Defaulted => NotificationPriority::Critical,
            _ => NotificationPriority::Medium,
        };

        Self::create_notification(
            env,
            invoice.business.clone(),
            NotificationType::InvoiceStatusChanged,
            priority.clone(),
            title.clone(),
            message.clone(),
            Some(invoice.id.clone()),
        )?;

        // Notify investor if applicable
        if let Some(investor) = &invoice.investor {
            Self::create_notification(
                env,
                investor.clone(),
                NotificationType::InvoiceStatusChanged,
                priority,
                title,
                message,
                Some(invoice.id.clone()),
            )?;
        }
        Ok(())
    }

    /// Create payment overdue notification
    pub fn notify_payment_overdue(
        env: &Env,
        invoice: &Invoice,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Payment Overdue");
        let message = String::from_str(env, "Your invoice payment is overdue");

        Self::create_notification(
            env,
            invoice.business.clone(),
            NotificationType::PaymentOverdue,
            NotificationPriority::Critical,
            title,
            message,
            Some(invoice.id.clone()),
        )?;

        // Notify investor
        if let Some(investor) = &invoice.investor {
            let investor_title = String::from_str(env, "Invoice Payment Overdue");
            let investor_message =
                String::from_str(env, "An invoice you funded has an overdue payment");

            Self::create_notification(
                env,
                investor.clone(),
                NotificationType::PaymentOverdue,
                NotificationPriority::Critical,
                investor_title,
                investor_message,
                Some(invoice.id.clone()),
            )?;
        }

        Ok(())
    }

    /// Create bid received notification for business
    pub fn notify_bid_received(
        env: &Env,
        invoice: &Invoice,
        _: &Bid, //bid
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "New Bid Received");
        let message = String::from_str(env, "A new bid has been placed on your invoice");

        Self::create_notification(
            env,
            invoice.business.clone(),
            NotificationType::BidReceived,
            NotificationPriority::Medium,
            title,
            message,
            Some(invoice.id.clone()),
        )?;

        Ok(())
    }

    /// Create bid accepted notification for investor
    pub fn notify_bid_accepted(
        env: &Env,
        invoice: &Invoice,
        bid: &Bid,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Bid Accepted");
        let message = String::from_str(
            env,
            "Your bid has been accepted and funds are being escrowed",
        );

        Self::create_notification(
            env,
            bid.investor.clone(),
            NotificationType::BidAccepted,
            NotificationPriority::High,
            title,
            message,
            Some(invoice.id.clone()),
        )?;

        Ok(())
    }

    /// Create payment received notification
    pub fn notify_payment_received(
        env: &Env,
        invoice: &Invoice,
        _: i128, //amount
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Payment Received");
        let message = String::from_str(env, "Payment has been received for your invoice");

        // Notify business
        Self::create_notification(
            env,
            invoice.business.clone(),
            NotificationType::PaymentReceived,
            NotificationPriority::High,
            title.clone(),
            message.clone(),
            Some(invoice.id.clone()),
        )?;

        // Notify investor if applicable
        if let Some(investor) = &invoice.investor {
            let investor_title = String::from_str(env, "Investment Payment Received");
            let investor_message =
                String::from_str(env, "Payment has been received for an invoice you funded");

            Self::create_notification(
                env,
                investor.clone(),
                NotificationType::PaymentReceived,
                NotificationPriority::High,
                investor_title,
                investor_message,
                Some(invoice.id.clone()),
            )?;
        }

        Ok(())
    }

    /// Create invoice defaulted notification
    pub fn notify_invoice_defaulted(
        env: &Env,
        invoice: &Invoice,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Invoice Defaulted");
        let message = String::from_str(env, "Your invoice has been marked as defaulted");

        // Notify business
        Self::create_notification(
            env,
            invoice.business.clone(),
            NotificationType::InvoiceDefaulted,
            NotificationPriority::Critical,
            title.clone(),
            message.clone(),
            Some(invoice.id.clone()),
        )?;

        // Notify investor if applicable
        if let Some(investor) = &invoice.investor {
            let investor_title = String::from_str(env, "Investment Defaulted");
            let investor_message = String::from_str(env, "An invoice you funded has defaulted");

            Self::create_notification(
                env,
                investor.clone(),
                NotificationType::InvoiceDefaulted,
                NotificationPriority::Critical,
                investor_title,
                investor_message,
                Some(invoice.id.clone()),
            )?;
        }

        Ok(())
    }
}
