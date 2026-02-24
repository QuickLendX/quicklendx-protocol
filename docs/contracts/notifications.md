# Notifications Module

## Overview

The Notifications module provides a comprehensive notification system for the QuickLendX protocol, enabling real-time communication between businesses, investors, and the platform. It supports notification creation, delivery tracking, user preferences, and statistics.

## Features

- **Multi-type Notifications**: Support for various notification types (invoice events, bid events, payment events)
- **Priority Levels**: High, Medium, and Low priority notifications
- **Delivery Tracking**: Track notification status (Pending, Sent, Delivered, Read, Failed)
- **User Preferences**: Customizable notification preferences per user
- **Statistics**: Comprehensive notification statistics per user
- **Overdue Monitoring**: Automatic notifications for overdue invoices

## Data Structures

### NotificationType

Defines the type of notification:

```rust
pub enum NotificationType {
    InvoiceCreated,
    InvoiceVerified,
    InvoiceStatusChanged,
    BidReceived,
    BidAccepted,
    PaymentReceived,
    PaymentOverdue,
    InvoiceDefaulted,
}
```

### NotificationPriority

Defines the priority level:

```rust
pub enum NotificationPriority {
    High,    // Critical notifications (defaults, overdue payments)
    Medium,  // Important notifications (bid accepted, payment received)
    Low,     // Informational notifications (invoice created, bid received)
}
```

### NotificationDeliveryStatus

Tracks delivery status:

```rust
pub enum NotificationDeliveryStatus {
    Pending,    // Created but not sent
    Sent,       // Sent to delivery system
    Delivered,  // Delivered to user
    Read,       // Read by user
    Failed,     // Delivery failed
}
```

### Notification

Core notification structure:

```rust
pub struct Notification {
    pub id: BytesN<32>,
    pub notification_type: NotificationType,
    pub recipient: Address,
    pub title: String,
    pub message: String,
    pub priority: NotificationPriority,
    pub created_at: u64,
    pub delivery_status: NotificationDeliveryStatus,
    pub sent_at: Option<u64>,
    pub delivered_at: Option<u64>,
    pub read_at: Option<u64>,
    pub related_invoice_id: Option<BytesN<32>>,
    pub related_bid_id: Option<BytesN<32>>,
}
```

### NotificationPreferences

User notification preferences:

```rust
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
}
```

### NotificationStats

User notification statistics:

```rust
pub struct NotificationStats {
    pub total_sent: u32,
    pub total_delivered: u32,
    pub total_read: u32,
    pub total_failed: u32,
    pub unread_count: u32,
}
```

## Core Functions

### create_notification

Creates a new notification for a user.

```rust
pub fn create_notification(
    env: &Env,
    notification_type: NotificationType,
    recipient: Address,
    title: String,
    message: String,
    priority: NotificationPriority,
    related_invoice_id: Option<BytesN<32>>,
    related_bid_id: Option<BytesN<32>>,
) -> BytesN<32>
```

**Parameters:**

- `notification_type`: Type of notification
- `recipient`: Address of the notification recipient
- `title`: Notification title
- `message`: Notification message content
- `priority`: Priority level
- `related_invoice_id`: Optional related invoice ID
- `related_bid_id`: Optional related bid ID

**Returns:** Notification ID

**Example:**

```rust
let notification_id = NotificationSystem::create_notification(
    &env,
    NotificationType::InvoiceCreated,
    business_addr,
    String::from_str(&env, "Invoice Created"),
    String::from_str(&env, "Your invoice has been created successfully"),
    NotificationPriority::Low,
    Some(invoice_id),
    None,
);
```

### get_notification

Retrieves a notification by ID.

```rust
pub fn get_notification(env: &Env, notification_id: &BytesN<32>) -> Option<Notification>
```

### update_notification_status

Updates the delivery status of a notification.

```rust
pub fn update_notification_status(
    env: &Env,
    notification_id: &BytesN<32>,
    status: NotificationDeliveryStatus,
) -> Result<(), QuickLendXError>
```

**Example:**

```rust
// Mark as sent
NotificationSystem::update_notification_status(
    &env,
    &notification_id,
    NotificationDeliveryStatus::Sent,
)?;

// Mark as delivered
NotificationSystem::update_notification_status(
    &env,
    &notification_id,
    NotificationDeliveryStatus::Delivered,
)?;

// Mark as read
NotificationSystem::update_notification_status(
    &env,
    &notification_id,
    NotificationDeliveryStatus::Read,
)?;
```

### get_user_notifications

Retrieves all notification IDs for a user.

```rust
pub fn get_user_notifications(env: &Env, user: &Address) -> Vec<BytesN<32>>
```

**Example:**

```rust
let notification_ids = NotificationSystem::get_user_notifications(&env, &user_addr);

// Fetch full notification details
for id in notification_ids.iter() {
    if let Some(notification) = NotificationSystem::get_notification(&env, &id) {
        // Process notification
    }
}
```

### get_user_preferences

Retrieves notification preferences for a user.

```rust
pub fn get_user_preferences(env: &Env, user: &Address) -> NotificationPreferences
```

### update_user_preferences

Updates notification preferences for a user.

```rust
pub fn update_user_preferences(
    env: &Env,
    user: &Address,
    preferences: NotificationPreferences,
)
```

**Example:**

```rust
let mut prefs = NotificationSystem::get_user_preferences(&env, &user_addr);

// Disable invoice created notifications
prefs.invoice_created = false;

// Enable payment overdue notifications
prefs.payment_overdue = true;

NotificationSystem::update_user_preferences(&env, &user_addr, prefs);
```

### get_user_notification_stats

Retrieves notification statistics for a user.

```rust
pub fn get_user_notification_stats(env: &Env, user: &Address) -> NotificationStats
```

**Example:**

```rust
let stats = NotificationSystem::get_user_notification_stats(&env, &user_addr);

println!("Total sent: {}", stats.total_sent);
println!("Total delivered: {}", stats.total_delivered);
println!("Total read: {}", stats.total_read);
println!("Unread count: {}", stats.unread_count);
```

## Helper Functions

### notify_invoice_created

Creates a notification when an invoice is created.

```rust
pub fn notify_invoice_created(
    env: &Env,
    business: &Address,
    invoice_id: &BytesN<32>,
)
```

### notify_invoice_verified

Creates a notification when an invoice is verified.

```rust
pub fn notify_invoice_verified(
    env: &Env,
    business: &Address,
    invoice_id: &BytesN<32>,
)
```

### notify_invoice_status_changed

Creates a notification when an invoice status changes.

```rust
pub fn notify_invoice_status_changed(
    env: &Env,
    business: &Address,
    invoice_id: &BytesN<32>,
    old_status: &InvoiceStatus,
    new_status: &InvoiceStatus,
)
```

### notify_payment_overdue

Creates a notification when a payment is overdue.

```rust
pub fn notify_payment_overdue(
    env: &Env,
    business: &Address,
    investor: &Address,
    invoice_id: &BytesN<32>,
    days_overdue: u32,
)
```

### notify_bid_received

Creates a notification when a bid is received.

```rust
pub fn notify_bid_received(
    env: &Env,
    business: &Address,
    invoice_id: &BytesN<32>,
    bid_id: &BytesN<32>,
)
```

### notify_bid_accepted

Creates a notification when a bid is accepted.

```rust
pub fn notify_bid_accepted(
    env: &Env,
    investor: &Address,
    invoice_id: &BytesN<32>,
    bid_id: &BytesN<32>,
)
```

### notify_payment_received

Creates a notification when a payment is received.

```rust
pub fn notify_payment_received(
    env: &Env,
    investor: &Address,
    invoice_id: &BytesN<32>,
    amount: i128,
)
```

### notify_invoice_defaulted

Creates a notification when an invoice defaults.

```rust
pub fn notify_invoice_defaulted(
    env: &Env,
    business: &Address,
    investor: &Address,
    invoice_id: &BytesN<32>,
)
```

## Usage Examples

### Complete Notification Flow

```rust
use soroban_sdk::{Address, String, BytesN, Env};
use crate::notifications::{NotificationSystem, NotificationType, NotificationPriority};

// 1. Create invoice and notify business
let invoice_id = create_invoice(&env, business_addr, amount, currency, due_date);
NotificationSystem::notify_invoice_created(&env, &business_addr, &invoice_id);

// 2. Verify invoice and notify business
verify_invoice(&env, &invoice_id);
NotificationSystem::notify_invoice_verified(&env, &business_addr, &invoice_id);

// 3. Investor places bid, notify business
let bid_id = place_bid(&env, investor_addr, invoice_id, bid_amount);
NotificationSystem::notify_bid_received(&env, &business_addr, &invoice_id, &bid_id);

// 4. Business accepts bid, notify investor
accept_bid(&env, invoice_id, bid_id);
NotificationSystem::notify_bid_accepted(&env, &investor_addr, &invoice_id, &bid_id);

// 5. Payment received, notify investor
record_payment(&env, invoice_id, payment_amount);
NotificationSystem::notify_payment_received(&env, &investor_addr, &invoice_id, payment_amount);

// 6. Check for overdue invoices
if invoice.is_overdue(current_timestamp) {
    let days_overdue = calculate_days_overdue(invoice.due_date, current_timestamp);
    NotificationSystem::notify_payment_overdue(
        &env,
        &business_addr,
        &investor_addr,
        &invoice_id,
        days_overdue,
    );
}
```

### Managing User Preferences

```rust
// Get current preferences
let mut prefs = NotificationSystem::get_user_preferences(&env, &user_addr);

// Customize preferences
prefs.invoice_created = true;
prefs.invoice_verified = true;
prefs.bid_received = true;
prefs.bid_accepted = true;
prefs.payment_received = true;
prefs.payment_overdue = true;
prefs.invoice_defaulted = true;

// Save preferences
NotificationSystem::update_user_preferences(&env, &user_addr, prefs);
```

### Tracking Notification Delivery

```rust
// Create notification
let notification_id = NotificationSystem::create_notification(
    &env,
    NotificationType::BidAccepted,
    investor_addr,
    String::from_str(&env, "Bid Accepted"),
    String::from_str(&env, "Your bid has been accepted"),
    NotificationPriority::Medium,
    Some(invoice_id),
    Some(bid_id),
);

// Mark as sent
NotificationSystem::update_notification_status(
    &env,
    &notification_id,
    NotificationDeliveryStatus::Sent,
)?;

// Mark as delivered
NotificationSystem::update_notification_status(
    &env,
    &notification_id,
    NotificationDeliveryStatus::Delivered,
)?;

// User reads notification
NotificationSystem::update_notification_status(
    &env,
    &notification_id,
    NotificationDeliveryStatus::Read,
)?;
```

### Querying Notifications

```rust
// Get all user notifications
let notification_ids = NotificationSystem::get_user_notifications(&env, &user_addr);

// Filter unread notifications
let mut unread_notifications = Vec::new(&env);
for id in notification_ids.iter() {
    if let Some(notification) = NotificationSystem::get_notification(&env, &id) {
        if notification.delivery_status != NotificationDeliveryStatus::Read {
            unread_notifications.push_back(notification);
        }
    }
}

// Get notification statistics
let stats = NotificationSystem::get_user_notification_stats(&env, &user_addr);
println!("Unread notifications: {}", stats.unread_count);
```

## Security Considerations

### Access Control

- Only the notification recipient can update their preferences
- Notification creation is restricted to authorized contract functions
- Delivery status updates should be restricted to authorized systems

### Data Privacy

- Notifications contain only necessary information
- Sensitive data should not be included in notification messages
- User preferences are private to each user

### Performance

- Notifications are indexed by user for efficient retrieval
- Bulk notification operations should be batched
- Consider pagination for users with many notifications

## Best Practices

### Notification Creation

1. **Use appropriate priority levels**:
   - High: Defaults, critical issues
   - Medium: Bid accepted, payment received
   - Low: Invoice created, informational updates

2. **Include relevant context**:
   - Always link to related invoice/bid when applicable
   - Provide clear, actionable messages
   - Use descriptive titles

3. **Respect user preferences**:
   - Check preferences before creating notifications
   - Provide granular control over notification types
   - Allow users to opt-out of non-critical notifications

### Delivery Tracking

1. **Update status promptly**:
   - Mark as sent immediately after sending
   - Update to delivered when confirmed
   - Track read status for analytics

2. **Handle failures gracefully**:
   - Mark failed notifications appropriately
   - Implement retry logic for transient failures
   - Log failures for debugging

### Performance Optimization

1. **Batch operations**:
   - Create multiple notifications in a single transaction when possible
   - Batch status updates for efficiency

2. **Pagination**:
   - Implement pagination for users with many notifications
   - Limit query results to prevent performance issues

3. **Cleanup**:
   - Consider archiving old notifications
   - Implement retention policies for notification data

## Testing

The notifications module includes comprehensive tests covering:

- ✅ Notification creation for all event types
- ✅ Delivery status tracking
- ✅ User preferences management
- ✅ Notification statistics
- ✅ Overdue invoice notifications
- ✅ User preference filtering
- ✅ Edge cases and error handling

Run tests with:

```bash
cargo test notification
```

## Integration

### Invoice Module Integration

```rust
// In invoice.rs
use crate::notifications::NotificationSystem;

// After creating invoice
NotificationSystem::notify_invoice_created(&env, &business, &invoice_id);

// After verifying invoice
NotificationSystem::notify_invoice_verified(&env, &business, &invoice_id);

// After status change
NotificationSystem::notify_invoice_status_changed(
    &env,
    &business,
    &invoice_id,
    &old_status,
    &new_status,
);
```

### Bid Module Integration

```rust
// In bid.rs
use crate::notifications::NotificationSystem;

// After bid placement
NotificationSystem::notify_bid_received(&env, &business, &invoice_id, &bid_id);

// After bid acceptance
NotificationSystem::notify_bid_accepted(&env, &investor, &invoice_id, &bid_id);
```

### Payment Module Integration

```rust
// In payments.rs
use crate::notifications::NotificationSystem;

// After payment received
NotificationSystem::notify_payment_received(&env, &investor, &invoice_id, amount);

// Check for overdue payments
if invoice.is_overdue(current_timestamp) {
    NotificationSystem::notify_payment_overdue(
        &env,
        &business,
        &investor,
        &invoice_id,
        days_overdue,
    );
}
```

## Future Enhancements

- Email/SMS integration for off-chain notifications
- Push notification support for mobile apps
- Notification templates for consistent messaging
- Notification scheduling and batching
- Advanced filtering and search capabilities
- Notification channels (email, SMS, push, in-app)
- Notification grouping and threading
- Rich notification content (images, links, actions)

## References

- [Events Module](./events.md) - Event emission system
- [Invoice Lifecycle](./invoice-lifecycle.md) - Invoice state management
- [Bidding System](./bidding.md) - Bid placement and acceptance
- [Payment Processing](./escrow.md) - Payment and escrow management
