pub const MAX_NOTIFICATION_TITLE_LENGTH: u32 = 150;
pub const MAX_NOTIFICATION_MESSAGE_LENGTH: u32 = 1000;

pub fn check_string_length(s: &String, max_len: u32) -> Result<(), crate::errors::QuickLendXError> {
    if s.len() > max_len {
        return Err(crate::errors::QuickLendXError::InvalidDescription);
    }
    Ok(())
}

use crate::types::Bid;
use crate::types::{Invoice, InvoiceStatus};
use soroban_sdk::{
    contracttype, symbol_short, xdr::ToXdr, Address, Bytes, BytesN, Env, Map, String, Vec,
};

/// TTL threshold in ledgers for temporary storage idempotency markers (~500 ledgers).
pub const IDEMPOTENCY_TTL_THRESHOLD: u32 = 500;
/// Extension target in ledgers for temporary storage idempotency markers (~1,000 ledgers).
pub const IDEMPOTENCY_TTL_EXTEND_TO: u32 = 1_000;

/// Deprecated constant preserved for backward-compatibility.
/// Idempotency markers now expire automatically via temporary storage TTL.
#[deprecated(note = "Idempotency markers are now evicted automatically via temporary storage TTL")]
pub const MAX_IDEMPOTENCY_KEYS: u32 = 10_000;

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
    NotificationCounter,
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
    /// Idempotency key derived from (event_kind, target_id, related_invoice_id, ledger_seq, nonce).
    /// Ensures one-shot delivery under replay scenarios while isolating distinct invoices.
    pub idempotency_key: BytesN<32>,
}

impl Notification {
    /// Create a new notification with collision-resistant ID and idempotency key derivation.
    ///
    /// # ID Derivation
    /// The notification ID is derived via keccak256 hash over:
    /// - `recipient`: The recipient address (encoded as XDR bytes)
    /// - `notification_type`: The notification type (encoded as 1-byte discriminant)
    /// - `related_invoice_id`: Optional 32-byte invoice ID (tagged with 1u8 if present, 0u8 if None)
    /// - `ledger_seq`: The ledger sequence number (big-endian u32)
    /// - `timestamp`: The ledger timestamp (big-endian u64)
    /// - `counter`: The monotonic instance notification counter (big-endian u64)
    ///
    /// Preimage:
    /// `recipient.to_xdr() || type_byte || (has_invoice ? 1 || invoice_id : 0) || ledger_seq || timestamp || counter`
    ///
    /// This ensures:
    /// - **Per-recipient isolation**: Notifications for different users in the same ledger get distinct IDs.
    /// - **Multi-notification safety**: Multiple notifications in the same transaction/ledger have distinct IDs.
    /// - **Tamper-proof & deterministic**: Tied to protocol state, timestamp, and instance counter.
    ///
    /// # Idempotency Key Derivation
    /// The idempotency key is derived via keccak256 hash over:
    /// - `notification_type`: The notification type (encoded as 1-byte discriminant)
    /// - `recipient`: The recipient address (encoded as XDR bytes)
    /// - `related_invoice_id`: Optional 32-byte invoice ID (tagged with 1u8 if present, 0u8 if None)
    /// - `ledger_seq`: The ledger sequence number (big-endian u32)
    /// - `nonce`: A unique nonce (typically ledger timestamp, big-endian u64)
    ///
    /// Preimage:
    /// `type_byte || recipient.to_xdr() || (has_invoice ? 1 || invoice_id : 0) || ledger_seq || nonce`
    ///
    /// This ensures:
    /// - **Per-invoice isolation**: Two overdue invoices for the same user in the same ledger generate distinct keys.
    /// - **Replay rejection**: The same event for the same user and invoice in the same ledger is rejected as duplicate.
    pub fn new(
        env: &Env,
        recipient: Address,
        notification_type: NotificationType,
        priority: NotificationPriority,
        title: String,
        message: String,
        related_invoice_id: Option<BytesN<32>>,
    ) -> Self {
        let created_at = env.ledger().timestamp();
        let ledger_seq = env.ledger().sequence();
        let counter = NotificationSystem::next_notification_counter(env);

        let id = Self::derive_notification_id(
            env,
            &recipient,
            &notification_type,
            related_invoice_id.as_ref(),
            ledger_seq,
            created_at,
            counter,
        );

        let idempotency_key = Self::derive_idempotency_key(
            env,
            &notification_type,
            &recipient,
            related_invoice_id.as_ref(),
            ledger_seq,
            created_at,
        );

        Self {
            id,
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

    /// Derive a unique, collision-resistant notification ID.
    ///
    /// # ID Preimage
    /// Preimage: `recipient.to_xdr() || type_byte || (has_invoice ? 1 || invoice_id : 0) || ledger_seq || timestamp || counter`
    pub fn derive_notification_id(
        env: &Env,
        recipient: &Address,
        notification_type: &NotificationType,
        related_invoice_id: Option<&BytesN<32>>,
        ledger_seq: u32,
        timestamp: u64,
        counter: u64,
    ) -> BytesN<32> {
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

        let mut preimage = Bytes::new(env);
        preimage.append(&recipient.to_xdr(env));
        preimage.append(&Bytes::from_array(env, &[type_byte]));
        if let Some(inv_id) = related_invoice_id {
            preimage.append(&Bytes::from_array(env, &[1u8]));
            preimage.append(&inv_id.clone().into());
        } else {
            preimage.append(&Bytes::from_array(env, &[0u8]));
        }
        preimage.append(&Bytes::from_array(env, &ledger_seq.to_be_bytes()));
        preimage.append(&Bytes::from_array(env, &timestamp.to_be_bytes()));
        preimage.append(&Bytes::from_array(env, &counter.to_be_bytes()));

        env.crypto().keccak256(&preimage).into()
    }

    /// Derive an idempotency key from notification parameters.
    ///
    /// # Parameters
    /// - `notification_type`: The type of notification (event_kind)
    /// - `recipient`: The target address (target_id)
    /// - `related_invoice_id`: The optional invoice ID associated with the notification
    /// - `ledger_seq`: The ledger sequence number
    /// - `nonce`: A unique nonce (typically timestamp)
    ///
    /// # Returns
    /// A 32-byte idempotency key derived via keccak256 hash.
    ///
    /// # Key Preimage
    /// Preimage: `type_byte || recipient.to_xdr() || (has_invoice ? 1 || invoice_id : 0) || ledger_seq || nonce`
    pub fn derive_idempotency_key(
        env: &Env,
        notification_type: &NotificationType,
        recipient: &Address,
        related_invoice_id: Option<&BytesN<32>>,
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

        // Build the preimage: type_byte || recipient_bytes || invoice_flag_and_id || ledger_seq || nonce
        let mut preimage = Bytes::new(env);
        preimage.append(&Bytes::from_array(env, &[type_byte]));
        preimage.append(&recipient.to_xdr(env));
        if let Some(inv_id) = related_invoice_id {
            preimage.append(&Bytes::from_array(env, &[1u8]));
            preimage.append(&inv_id.clone().into());
        } else {
            preimage.append(&Bytes::from_array(env, &[0u8]));
        }
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
    /// Uses temporary storage to track previously-seen idempotency keys.
    /// Temporary storage automatically expires after `IDEMPOTENCY_TTL_EXTEND_TO`
    /// ledgers, preventing unbounded accumulation without requiring an instance-storage bloom set.
    fn has_seen_idempotency_key(env: &Env, key: &BytesN<32>) -> bool {
        let storage_key = DataKey::IdempotencyKey(key.clone());
        env.storage().temporary().has(&storage_key)
    }

    /// Record an idempotency key as seen in temporary storage.
    ///
    /// Sets the key with a temporary storage TTL to protect against replays
    /// during the active ledger window while guaranteeing bounded contract state.
    fn record_idempotency_key(env: &Env, key: &BytesN<32>) {
        let storage_key = DataKey::IdempotencyKey(key.clone());
        env.storage().temporary().set(&storage_key, &true);
        env.storage().temporary().extend_ttl(
            &storage_key,
            IDEMPOTENCY_TTL_THRESHOLD,
            IDEMPOTENCY_TTL_EXTEND_TO,
        );
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

    /// Store a notification in persistent storage with TTL extension.
    fn store_notification(env: &Env, notification: &Notification) {
        let key = Self::get_notification_key(&notification.id);
        env.storage().persistent().set(&key, notification);
        crate::storage::extend_persistent_ttl(env, &key);
    }

    /// Get a notification by ID from persistent storage with TTL refresh.
    pub fn get_notification(env: &Env, notification_id: &BytesN<32>) -> Option<Notification> {
        let key = Self::get_notification_key(notification_id);
        let result = env.storage().persistent().get(&key);
        if result.is_some() {
            crate::storage::extend_persistent_ttl(env, &key);
        }
        result
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

    /// Get user notifications from persistent storage with TTL refresh.
    pub fn get_user_notifications(env: &Env, user: &Address) -> Vec<BytesN<32>> {
        let key = Self::get_user_notifications_key(user);
        let result = env.storage().persistent().get(&key);
        if result.is_some() {
            crate::storage::extend_persistent_ttl(env, &key);
        }
        result.unwrap_or_else(|| Vec::new(env))
    }

    /// Return a paginated slice of user notification IDs.
    pub fn get_user_notifications_paged(
        env: &Env,
        user: &Address,
        offset: u32,
        limit: u32,
    ) -> Vec<BytesN<32>> {
        let list = Self::get_user_notifications(env, user);
        let total_count = list.len();
        let (start, end) = crate::pagination::calculate_safe_bounds(offset, limit, total_count);
        let mut page = Vec::new(env);
        let mut idx = start;
        while idx < end {
            if let Some(id) = list.get(idx) {
                page.push_back(id);
            }
            idx = idx.saturating_add(1);
        }
        page
    }

    /// Return a paginated response with metadata for user notifications.
    pub fn get_user_notifications_paginated(
        env: &Env,
        user: &Address,
        offset: u32,
        limit: u32,
    ) -> crate::types::PaginatedBytes32Vec {
        let items = Self::get_user_notifications_paged(env, user, offset, limit);
        let total_count = Self::get_user_notifications(env, user).len();
        let (_, has_more) = crate::pagination::pagination_metadata(offset, limit, total_count);
        crate::types::PaginatedBytes32Vec {
            items,
            total_count,
            has_more,
        }
    }

    /// Get user notification preferences from persistent storage with TTL refresh.
    pub fn get_user_preferences(env: &Env, user: &Address) -> NotificationPreferences {
        let key = DataKey::UserPreferences(user.clone());
        let result = env.storage().persistent().get(&key);
        if result.is_some() {
            crate::storage::extend_persistent_ttl(env, &key);
        }
        result.unwrap_or_else(|| NotificationPreferences::default_for_user(env, user.clone()))
    }

    /// Update user notification preferences in persistent storage with TTL refresh.
    pub fn update_user_preferences(
        env: &Env,
        user: &Address,
        preferences: NotificationPreferences,
    ) {
        let key = DataKey::UserPreferences(user.clone());
        env.storage().persistent().set(&key, &preferences);
        crate::storage::extend_persistent_ttl(env, &key);

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

    /// Return the number of unread notifications for `user` without loading full notification bodies.
    ///
    /// "Unread" is defined as delivery_status != Read. The scan is bounded by
    /// the user's notification-ID list length.
    pub fn get_notification_unread_count(env: &Env, user: &Address) -> u32 {
        let notification_ids = Self::get_user_notifications(env, user);
        let mut unread: u32 = 0;
        for id in notification_ids.iter() {
            if let Some(n) = Self::get_notification(env, &id) {
                if n.delivery_status != NotificationDeliveryStatus::Read {
                    unread += 1;
                }
            }
        }
        unread
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
        env.storage().persistent().set(&key, &notifications);
        crate::storage::extend_persistent_ttl(env, &key);
    }

    /// Retrieve and increment the instance-level monotonic notification counter.
    ///
    /// Guarantees that every notification created within the contract gets a strictly
    /// unique sequence number, preventing collisions even when multiple notifications
    /// are generated within the same ledger.
    pub fn next_notification_counter(env: &Env) -> u64 {
        let key = DataKey::NotificationCounter;
        let current: u64 = env.storage().instance().get(&key).unwrap_or(0);
        let next = current.saturating_add(1);
        env.storage().instance().set(&key, &next);
        next
    }

    /// Attempt delivery to a recipient, treating opt-out (`NotificationBlocked`) and
    /// replay (`NotificationDuplicate`) as non-fatal skips for multi-recipient fan-out.
    /// Returns `Ok(true)` if delivered, `Ok(false)` if skipped, or `Err(e)` on genuine failure.
    pub fn dispatch_recipient_notification(
        env: &Env,
        recipient: Address,
        notification_type: NotificationType,
        priority: NotificationPriority,
        title: String,
        message: String,
        related_invoice_id: Option<BytesN<32>>,
    ) -> Result<bool, crate::errors::QuickLendXError> {
        match Self::create_notification(
            env,
            recipient,
            notification_type,
            priority,
            title,
            message,
            related_invoice_id,
        ) {
            Ok(_) => Ok(true),
            Err(crate::errors::QuickLendXError::NotificationBlocked)
            | Err(crate::errors::QuickLendXError::NotificationDuplicate) => Ok(false),
            Err(e) => Err(e),
        }
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

        let mut first_err: Option<crate::errors::QuickLendXError> = None;

        if let Err(e) = Self::dispatch_recipient_notification(
            env,
            invoice.business.clone(),
            NotificationType::InvoiceStatusChanged,
            priority.clone(),
            title.clone(),
            message.clone(),
            Some(invoice.id.clone()),
        ) {
            first_err = Some(e);
        }

        // Notify investor if applicable
        if let Some(investor) = &invoice.investor {
            if let Err(e) = Self::dispatch_recipient_notification(
                env,
                investor.clone(),
                NotificationType::InvoiceStatusChanged,
                priority,
                title,
                message,
                Some(invoice.id.clone()),
            ) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }

        if let Some(err) = first_err {
            Err(err)
        } else {
            Ok(())
        }
    }

    /// Create payment overdue notification
    pub fn notify_payment_overdue(
        env: &Env,
        invoice: &Invoice,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Payment Overdue");
        let message = String::from_str(env, "Your invoice payment is overdue");

        let mut first_err: Option<crate::errors::QuickLendXError> = None;

        if let Err(e) = Self::dispatch_recipient_notification(
            env,
            invoice.business.clone(),
            NotificationType::PaymentOverdue,
            NotificationPriority::Critical,
            title,
            message,
            Some(invoice.id.clone()),
        ) {
            first_err = Some(e);
        }

        // Notify investor
        if let Some(investor) = &invoice.investor {
            let investor_title = String::from_str(env, "Invoice Payment Overdue");
            let investor_message =
                String::from_str(env, "An invoice you funded has an overdue payment");

            if let Err(e) = Self::dispatch_recipient_notification(
                env,
                investor.clone(),
                NotificationType::PaymentOverdue,
                NotificationPriority::Critical,
                investor_title,
                investor_message,
                Some(invoice.id.clone()),
            ) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }

        if let Some(err) = first_err {
            Err(err)
        } else {
            Ok(())
        }
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

        let mut first_err: Option<crate::errors::QuickLendXError> = None;

        // Notify business
        if let Err(e) = Self::dispatch_recipient_notification(
            env,
            invoice.business.clone(),
            NotificationType::PaymentReceived,
            NotificationPriority::High,
            title,
            message,
            Some(invoice.id.clone()),
        ) {
            first_err = Some(e);
        }

        // Notify investor if applicable
        if let Some(investor) = &invoice.investor {
            let investor_title = String::from_str(env, "Investment Payment Received");
            let investor_message =
                String::from_str(env, "Payment has been received for an invoice you funded");

            if let Err(e) = Self::dispatch_recipient_notification(
                env,
                investor.clone(),
                NotificationType::PaymentReceived,
                NotificationPriority::High,
                investor_title,
                investor_message,
                Some(invoice.id.clone()),
            ) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }

        if let Some(err) = first_err {
            Err(err)
        } else {
            Ok(())
        }
    }

    /// Create invoice defaulted notification
    pub fn notify_invoice_defaulted(
        env: &Env,
        invoice: &Invoice,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Invoice Defaulted");
        let message = String::from_str(env, "Your invoice has been marked as defaulted");

        let mut first_err: Option<crate::errors::QuickLendXError> = None;

        // Notify business
        if let Err(e) = Self::dispatch_recipient_notification(
            env,
            invoice.business.clone(),
            NotificationType::InvoiceDefaulted,
            NotificationPriority::Critical,
            title,
            message,
            Some(invoice.id.clone()),
        ) {
            first_err = Some(e);
        }

        // Notify investor if applicable
        if let Some(investor) = &invoice.investor {
            let investor_title = String::from_str(env, "Investment Defaulted");
            let investor_message = String::from_str(env, "An invoice you funded has defaulted");

            if let Err(e) = Self::dispatch_recipient_notification(
                env,
                investor.clone(),
                NotificationType::InvoiceDefaulted,
                NotificationPriority::Critical,
                investor_title,
                investor_message,
                Some(invoice.id.clone()),
            ) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }

        if let Some(err) = first_err {
            Err(err)
        } else {
            Ok(())
        }
    }

    /// Notify business and investor that a dispute was opened on an invoice.
    ///
    /// Uses `NotificationType::SystemAlert` so dispute lifecycle signals are delivered
    /// under default preferences (`general` is opt-in). Failures are isolated between
    /// recipients.
    pub fn notify_dispute_opened(
        env: &Env,
        invoice: &Invoice,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Dispute Opened");
        let message = String::from_str(env, "A dispute has been opened on your invoice");

        let mut first_err: Option<crate::errors::QuickLendXError> = None;

        if let Err(e) = Self::dispatch_recipient_notification(
            env,
            invoice.business.clone(),
            NotificationType::SystemAlert,
            NotificationPriority::High,
            title.clone(),
            message.clone(),
            Some(invoice.id.clone()),
        ) {
            first_err = Some(e);
        }

        if let Some(investor) = &invoice.investor {
            if let Err(e) = Self::dispatch_recipient_notification(
                env,
                investor.clone(),
                NotificationType::SystemAlert,
                NotificationPriority::High,
                title,
                message,
                Some(invoice.id.clone()),
            ) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }

        if let Some(err) = first_err {
            Err(err)
        } else {
            Ok(())
        }
    }

    /// Notify business and investor that a dispute was resolved.
    pub fn notify_dispute_resolved(
        env: &Env,
        invoice: &Invoice,
    ) -> Result<(), crate::errors::QuickLendXError> {
        let title = String::from_str(env, "Dispute Resolved");
        let message = String::from_str(env, "The dispute on your invoice has been resolved");

        let mut first_err: Option<crate::errors::QuickLendXError> = None;

        if let Err(e) = Self::dispatch_recipient_notification(
            env,
            invoice.business.clone(),
            NotificationType::SystemAlert,
            NotificationPriority::High,
            title.clone(),
            message.clone(),
            Some(invoice.id.clone()),
        ) {
            first_err = Some(e);
        }

        if let Some(investor) = &invoice.investor {
            if let Err(e) = Self::dispatch_recipient_notification(
                env,
                investor.clone(),
                NotificationType::SystemAlert,
                NotificationPriority::High,
                title,
                message,
                Some(invoice.id.clone()),
            ) {
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
        }

        if let Some(err) = first_err {
            Err(err)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::QuickLendXContract;
    use crate::types::{Invoice, InvoiceCategory, InvoiceStatus};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, BytesN, Env, String,
    };

    fn setup() -> (Env, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(QuickLendXContract, ());
        (env, contract_id)
    }

    fn sample_invoice(
        env: &Env,
        business: &Address,
        investor: Option<&Address>,
    ) -> Invoice {
        Invoice {
            id: BytesN::from_array(env, &[1u8; 32]),
            business: business.clone(),
            amount: 10_000,
            currency: Address::generate(env),
            due_date: env.ledger().timestamp() + 86_400,
            status: InvoiceStatus::Verified,
            created_at: env.ledger().timestamp(),
            description: String::from_str(env, "Test Invoice"),
            metadata_customer_name: None,
            metadata_customer_address: None,
            metadata_tax_id: None,
            metadata_notes: None,
            metadata_line_items: soroban_sdk::Vec::new(env),
            category: InvoiceCategory::Services,
            tags: soroban_sdk::Vec::new(env),
            funded_amount: 0,
            funded_at: None,
            investor: investor.cloned(),
            settled_at: None,
            average_rating: None,
            total_ratings: 0,
            ratings: soroban_sdk::Vec::new(env),
            dispute_status: crate::types::DisputeStatus::None,
            dispute: crate::types::Dispute {
                created_by: business.clone(),
                created_at: 0,
                reason: String::from_str(env, ""),
                evidence: String::from_str(env, ""),
                resolution: String::from_str(env, ""),
                resolved_by: business.clone(),
                resolved_at: 0,
                resolution_outcome: crate::types::DisputeResolution::None,
            },
            total_paid: 0,
            payment_history: soroban_sdk::Vec::new(env),
            origination_fee_bps: None,
            late_payment_penalty_bps: None,
            early_payment_discount_bps: None,
        }
    }

    /// Acceptance Criterion 1:
    /// reproduction step 2 gives distinct IDs, each record has the right `recipient`,
    /// and marking B's as read leaves I's unread.
    #[test]
    fn same_ledger_notifications_unique() {
        let (env, contract_id) = setup();
        env.as_contract(&contract_id, || {
            env.ledger().set_timestamp(1_000);
            env.ledger().set_sequence_number(100);

            let business = Address::generate(&env);
            let investor = Address::generate(&env);
            let invoice = sample_invoice(&env, &business, Some(&investor));

            let res = NotificationSystem::notify_dispute_opened(&env, &invoice);
            assert!(res.is_ok(), "notify_dispute_opened must succeed");

            let biz_notifs = NotificationSystem::get_user_notifications(&env, &business);
            let inv_notifs = NotificationSystem::get_user_notifications(&env, &investor);

            assert_eq!(biz_notifs.len(), 1, "business must have 1 notification");
            assert_eq!(inv_notifs.len(), 1, "investor must have 1 notification");

            let biz_id = biz_notifs.get(0).unwrap();
            let inv_id = inv_notifs.get(0).unwrap();

            // Distinct IDs in the same ledger
            assert_ne!(biz_id, inv_id, "notification IDs in same ledger must be distinct");

            // Correct recipient on each record
            let biz_record = NotificationSystem::get_notification(&env, &biz_id).unwrap();
            let inv_record = NotificationSystem::get_notification(&env, &inv_id).unwrap();

            assert_eq!(biz_record.recipient, business, "business record must belong to business");
            assert_eq!(inv_record.recipient, investor, "investor record must belong to investor");
            assert_eq!(biz_record.delivery_status, NotificationDeliveryStatus::Pending);
            assert_eq!(inv_record.delivery_status, NotificationDeliveryStatus::Pending);

            // Marking business's as read leaves investor's unread
            NotificationSystem::update_notification_status(&env, &biz_id, NotificationDeliveryStatus::Read)
                .expect("mark business notif as read");

            let biz_record_after = NotificationSystem::get_notification(&env, &biz_id).unwrap();
            let inv_record_after = NotificationSystem::get_notification(&env, &inv_id).unwrap();

            assert_eq!(biz_record_after.delivery_status, NotificationDeliveryStatus::Read);
            assert_eq!(inv_record_after.delivery_status, NotificationDeliveryStatus::Pending, "investor record must remain unread");

            assert_eq!(NotificationSystem::get_notification_unread_count(&env, &business), 0);
            assert_eq!(NotificationSystem::get_notification_unread_count(&env, &investor), 1);
        });
    }

    /// Acceptance Criterion 2:
    /// reproduction step 3. I receives exactly one notification even if B has opted out.
    #[test]
    fn blocked_business_does_not_block_investor() {
        let (env, contract_id) = setup();
        env.as_contract(&contract_id, || {
            env.ledger().set_timestamp(1_000);
            env.ledger().set_sequence_number(100);

            let business = Address::generate(&env);
            let investor = Address::generate(&env);
            let invoice = sample_invoice(&env, &business, Some(&investor));

            // Set business preferences to block SystemAlert
            let mut prefs = NotificationSystem::get_user_preferences(&env, &business);
            prefs.system_alerts = false;
            NotificationSystem::update_user_preferences(&env, &business, prefs);

            let res = NotificationSystem::notify_dispute_opened(&env, &invoice);
            assert!(res.is_ok(), "notify_dispute_opened must succeed even if business is blocked");

            let biz_notifs = NotificationSystem::get_user_notifications(&env, &business);
            let inv_notifs = NotificationSystem::get_user_notifications(&env, &investor);

            assert_eq!(biz_notifs.len(), 0, "blocked business must receive 0 notifications");
            assert_eq!(inv_notifs.len(), 1, "investor must receive exactly 1 notification");

            let inv_id = inv_notifs.get(0).unwrap();
            let inv_record = NotificationSystem::get_notification(&env, &inv_id).unwrap();
            assert_eq!(inv_record.recipient, investor);
            assert_eq!(inv_record.notification_type, NotificationType::SystemAlert);
        });
    }

    /// Acceptance Criterion 3:
    /// reproduction step 4. B has 2 PaymentOverdue notifications with different related_invoice_ids.
    #[test]
    fn two_invoices_same_ledger_both_notified() {
        let (env, contract_id) = setup();
        env.as_contract(&contract_id, || {
            env.ledger().set_timestamp(1_000);
            env.ledger().set_sequence_number(100);

            let business = Address::generate(&env);
            let investor = Address::generate(&env);

            let mut invoice1 = sample_invoice(&env, &business, Some(&investor));
            invoice1.id = BytesN::from_array(&env, &[1u8; 32]);

            let mut invoice2 = sample_invoice(&env, &business, Some(&investor));
            invoice2.id = BytesN::from_array(&env, &[2u8; 32]);

            let res1 = NotificationSystem::notify_payment_overdue(&env, &invoice1);
            let res2 = NotificationSystem::notify_payment_overdue(&env, &invoice2);

            assert!(res1.is_ok(), "first overdue notification must succeed");
            assert!(res2.is_ok(), "second overdue notification must succeed");

            let biz_notifs = NotificationSystem::get_user_notifications(&env, &business);
            let inv_notifs = NotificationSystem::get_user_notifications(&env, &investor);

            assert_eq!(biz_notifs.len(), 2, "business must have 2 notifications");
            assert_eq!(inv_notifs.len(), 2, "investor must have 2 notifications");

            let b_notif1 = NotificationSystem::get_notification(&env, &biz_notifs.get(0).unwrap()).unwrap();
            let b_notif2 = NotificationSystem::get_notification(&env, &biz_notifs.get(1).unwrap()).unwrap();

            assert_eq!(b_notif1.notification_type, NotificationType::PaymentOverdue);
            assert_eq!(b_notif2.notification_type, NotificationType::PaymentOverdue);

            assert_ne!(b_notif1.related_invoice_id, b_notif2.related_invoice_id, "notifications must reference different invoices");
            assert_eq!(b_notif1.related_invoice_id, Some(invoice1.id));
            assert_eq!(b_notif2.related_invoice_id, Some(invoice2.id));
        });
    }

    /// Acceptance Criterion 4:
    /// the same type, recipient and invoice twice in one ledger gives NotificationDuplicate
    /// on the second create_notification.
    #[test]
    fn true_duplicate_still_rejected() {
        let (env, contract_id) = setup();
        env.as_contract(&contract_id, || {
            env.ledger().set_timestamp(1_000);
            env.ledger().set_sequence_number(100);

            let recipient = Address::generate(&env);
            let invoice_id = BytesN::from_array(&env, &[42u8; 32]);

            let res1 = NotificationSystem::create_notification(
                &env,
                recipient.clone(),
                NotificationType::PaymentOverdue,
                NotificationPriority::Critical,
                String::from_str(&env, "Payment Overdue"),
                String::from_str(&env, "Your invoice payment is overdue"),
                Some(invoice_id.clone()),
            );
            assert!(res1.is_ok(), "first creation must succeed");

            let res2 = NotificationSystem::create_notification(
                &env,
                recipient.clone(),
                NotificationType::PaymentOverdue,
                NotificationPriority::Critical,
                String::from_str(&env, "Payment Overdue"),
                String::from_str(&env, "Your invoice payment is overdue"),
                Some(invoice_id.clone()),
            );

            assert!(
                matches!(res2, Err(crate::errors::QuickLendXError::NotificationDuplicate)),
                "exact duplicate in same ledger must return NotificationDuplicate"
            );
        });
    }
}
