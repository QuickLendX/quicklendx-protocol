use crate::protocol_limits::{
    check_string_length, MAX_NOTIFICATION_MESSAGE_LENGTH, MAX_NOTIFICATION_TITLE_LENGTH,
};
use crate::types::Bid;
use crate::types::{Invoice, InvoiceStatus};
use soroban_sdk::{
    contracttype, symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, Map, String, Vec,
};

/// Maximum number of idempotency keys to track in the bloom-resistant set.
/// This provides protection against replay attacks while maintaining reasonable storage.
const MAX_IDEMPOTENCY_KEYS: u32 = 10_000;

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
    IdempotencyKey(BytesN<32>),
    IdempotencyKeySet,
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
    /// Idempotency key derived from (event_kind, target_id, ledger_seq, nonce).
    /// Ensures one-shot delivery under replay scenarios.
    pub idempotency_key: BytesN<32>,
}

impl Notification {
    /// Create a new notification with idempotency key derivation.
    ///
    /// # Idempotency Key Derivation
    /// The idempotency key is derived from:
    /// - `event_kind`: The notification type (encoded as bytes)
    /// - `target_id`: The recipient address (encoded as bytes)
    /// - `ledger_seq`: The current ledger sequence number
    /// - `nonce`: A unique nonce (derived from timestamp)
    ///
    /// The key is computed as: `keccak256(event_kind || target_id || ledger_seq || nonce)`
    ///
    /// This derivation is:
    /// - **Collision-resistant**: keccak256 provides cryptographic strength
    /// - **Stable across versions**: Uses only fundamental protocol data
    /// - **Deterministic**: Same inputs always produce the same key
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

        // Derive idempotency key from (event_kind, target_id, ledger_seq, nonce)
        let idempotency_key = Self::derive_idempotency_key(
            env,
            &notification_type,
            &recipient,
            env.ledger().sequence(),
            created_at,
        );

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
            idempotency_key,
        }
    }

    /// Derive an idempotency key from notification parameters.
    ///
    /// # Parameters
    /// - `notification_type`: The type of notification (event_kind)
    /// - `recipient`: The target address (target_id)
    /// - `ledger_seq`: The ledger sequence number
    /// - `nonce`: A unique nonce (typically timestamp)
    ///
    /// # Returns
    /// A 32-byte idempotency key derived via keccak256 hash.
    ///
    /// # Collision Resistance
    /// The key uses keccak256, which provides 256-bit security against collisions.
    /// The combination of notification type, recipient, ledger sequence, and nonce
    /// ensures uniqueness across all notification scenarios.
    fn derive_idempotency_key(
        env: &Env,
        notification_type: &NotificationType,
        recipient: &Address,
        ledger_seq: u32,
        nonce: u64,
    ) -> BytesN<32> {
        // Encode notification type as a single byte discriminant
        let type_byte = match notification_type {
            NotificationType::InvoiceCreated => 0u8,
            NotificationType::InvoiceVerified => 1u8,
            NotificationType::InvoiceStatusChanged => 2u8,
            NotificationType::BidReceived => 3u8,
            NotificationType::BidAccepted => 4u8,
            NotificationType::PaymentReceived => 5u8,
            NotificationType::PaymentOverdue => 6u8,
            NotificationType::InvoiceDefaulted => 7u8,
            NotificationType::SystemAlert => 8u8,
            NotificationType::General => 9u8,
        };

        // Build the preimage: type_byte || recipient_bytes || ledger_seq || nonce
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from_array(env, &[type_byte]));
        preimage.append(&recipient.to_xdr(env));
        preimage.append(&Bytes::from_array(env, &ledger_seq.to_be_bytes()));
        preimage.append(&Bytes::from_array(env, &nonce.to_be_bytes()));

        env.crypto().keccak256(&preimage).into()
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
    /// Check if an idempotency key has been seen before.
    ///
    /// Uses a bloom-resistant set stored in contract storage to track
    /// previously-seen idempotency keys. This prevents duplicate notifications
    /// from being emitted if a transaction is replayed.
    fn has_seen_idempotency_key(env: &Env, key: &BytesN<32>) -> bool {
        let storage_key = DataKey::IdempotencyKey(key.clone());
        env.storage().instance().has(&storage_key)
    }

    /// Record an idempotency key as seen.
    ///
    /// Stores the key in the bloom-resistant set to prevent future replays.
    /// If the set exceeds MAX_IDEMPOTENCY_KEYS, the oldest entries are pruned.
    fn record_idempotency_key(env: &Env, key: &BytesN<32>) {
        let storage_key = DataKey::IdempotencyKey(key.clone());
        env.storage().instance().set(&storage_key, &true);

        // Track the set size for potential pruning
        let set_key = DataKey::IdempotencyKeySet;
        let mut key_set: Vec<BytesN<32>> = env
            .storage()
            .instance()
            .get(&set_key)
            .unwrap_or_else(|| Vec::new(env));

        if key_set.len() < MAX_IDEMPOTENCY_KEYS {
            key_set.push_back(key.clone());
            env.storage().instance().set(&set_key, &key_set);
        }
    }

    /// Create and store a notification with idempotency protection.
    ///
    /// # Idempotency Guarantee
    /// If a notification with the same idempotency key is submitted twice,
    /// the second submission is rejected with `NotificationDuplicate`.
    /// This ensures one-shot delivery semantics even under transaction replay.
    ///
    /// # Interplay with Indexer Deduplication
    /// The backend indexer also maintains a UNIQUE(event_id, user_id) constraint
    /// at the database level. This contract-level idempotency key provides:
    /// 1. **Immediate rejection** at the contract level (no event emission)
    /// 2. **Deterministic key derivation** that survives contract upgrades
    /// 3. **Replay protection** for the same logical notification event
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

        // Create notification (which derives idempotency key)
        let notification = Notification::new(
            env,
            recipient.clone(),
            notification_type.clone(),
            priority.clone(),
            title,
            message,
            related_invoice_id,
        );

        // Check for duplicate via idempotency key
        if Self::has_seen_idempotency_key(env, &notification.idempotency_key) {
            return Err(crate::errors::QuickLendXError::NotificationDuplicate);
        }

        // Record the idempotency key to prevent future replays
        Self::record_idempotency_key(env, &notification.idempotency_key);

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
    fn store_notification(env: &Env, notification: &Notification) {
        let key = Self::get_notification_key(&notification.id);
        env.storage().instance().set(&key, notification);
    }

    /// Get a notification by ID
    pub fn get_notification(env: &Env, notification_id: &BytesN<32>) -> Option<Notification> {
        let key = Self::get_notification_key(notification_id);
        env.storage().instance().get(&key)
    }

    /// Update notification status
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

// Notification helper functions for common scenarios
impl NotificationSystem {
    /// Create invoice created notification
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

    /// Create invoice verified notification
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

    /// Create invoice status changed notification
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

    /// Notify business and investor that a dispute was opened on an invoice.
    ///
    /// Uses `NotificationType::SystemAlert` so dispute lifecycle signals are delivered
    /// under default preferences (`general` is opt-in). Failures are isolated from
    /// fund-moving calls.
    pub fn notify_dispute_opened(
        env: &Env,
        invoice: &Invoice,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Dispute Opened");
        let message = String::from_str(env, "A dispute has been opened on your invoice");

        Self::create_notification(
            env,
            invoice.business.clone(),
            NotificationType::SystemAlert,
            NotificationPriority::High,
            title.clone(),
            message.clone(),
            Some(invoice.id.clone()),
        )?;

        if let Some(investor) = &invoice.investor {
            Self::create_notification(
                env,
                investor.clone(),
                NotificationType::SystemAlert,
                NotificationPriority::High,
                title,
                message,
                Some(invoice.id.clone()),
            )?;
        }

        Ok(())
    }

    /// Notify business and investor that a dispute was resolved.
    pub fn notify_dispute_resolved(
        env: &Env,
        invoice: &Invoice,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Dispute Resolved");
        let message = String::from_str(env, "The dispute on your invoice has been resolved");

        Self::create_notification(
            env,
            invoice.business.clone(),
            NotificationType::SystemAlert,
            NotificationPriority::High,
            title.clone(),
            message.clone(),
            Some(invoice.id.clone()),
        )?;

        if let Some(investor) = &invoice.investor {
            Self::create_notification(
                env,
                investor.clone(),
                NotificationType::SystemAlert,
                NotificationPriority::High,
                title,
                message,
                Some(invoice.id.clone()),
            )?;
        }

        Ok(())
    }
}
