# Email Notification Service

## Overview

The QuickLendX backend includes an optional email notification service that sends notifications for key lifecycle events. The service is designed to be:

- **Opt-in**: Users must explicitly enable email notifications
- **Privacy-aware**: No sensitive information is included in emails
- **Idempotent**: Duplicate events are not sent multiple times
- **Efficient**: Minimal overhead and easy to review

## Architecture

### Components

1. **NotificationService**: Core service handling email sending, templates, and user preferences
2. **EventProcessor**: Processes blockchain events and triggers notifications
3. **Notification Routes**: REST API endpoints for managing preferences
4. **Email Templates**: HTML and text templates for different notification types

### Event Flow

```
Blockchain Event → Indexer → POST /api/v1/events → EventProcessor → NotificationService → Email
```

## Supported Events

The service monitors the following Soroban contract events:

- **InvoiceSettled**: Invoice has been funded
- **PaymentRecorded**: Payment received for invoice
- **DisputeCreated**: Dispute opened for invoice
- **DisputeResolved**: Dispute resolved

## User Preferences

Users can configure their notification preferences via the API:

```typescript
interface UserNotificationPreferences {
  email_enabled: boolean;
  email_address?: string;
  notifications: {
    invoice_funded: boolean;
    payment_received: boolean;
    dispute_opened: boolean;
    dispute_resolved: boolean;
  };
}
```

## API Endpoints

### Get Preferences
```
GET /api/v1/notifications/preferences/:userId
```

### Update Preferences
```
PUT /api/v1/notifications/preferences/:userId
Content-Type: application/json

{
  "email_enabled": true,
  "email_address": "user@example.com",
  "notifications": {
    "invoice_funded": true,
    "payment_received": true,
    "dispute_opened": false,
    "dispute_resolved": true
  }
}
```

### Unsubscribe
```
POST /api/v1/notifications/unsubscribe/:userId
```

### Process Events (Internal)
```
POST /api/v1/events
Content-Type: application/json

{
  "type": "InvoiceSettled",
  "id": "event123",
  "invoice_id": "inv123",
  "business": "GABC...",
  "investor": "GXYZ...",
  "amount": "1000",
  "timestamp": 1234567890
}
```

## Email Templates

All emails include:
- Clear subject lines
- HTML and plain text versions
- Links to relevant pages in the frontend
- Unsubscribe information

### Security Considerations

- No sensitive data (passwords, private keys, etc.) is included
- Emails contain only public invoice IDs and amounts
- Unsubscribe links are provided in all emails
- SMTP credentials are stored securely via environment variables

## Configuration

Required environment variables:

```bash
# SMTP Configuration
SMTP_HOST=smtp.gmail.com
SMTP_PORT=587
SMTP_USER=your-email@gmail.com
SMTP_PASS=your-app-password

# Email Settings
FROM_EMAIL=noreply@quicklendx.com
DEFAULT_EMAIL=user@example.com

# Frontend URL for links
FRONTEND_URL=https://quicklendx.com
```

## Testing

The service includes comprehensive tests covering:

- Idempotency (duplicate event handling)
- Template rendering
- User preference filtering
- Email sending error handling
- API endpoint validation

Run tests with:
```bash
npm test
```

## Deployment Notes

- The service is optional and can be disabled by not configuring SMTP
- In production, implement persistent storage for sent notifications (Redis/database)
- Consider rate limiting for email sending
- Monitor email delivery success/failure rates
- Implement email bounce handling if needed

## Future Enhancements

- SMS notifications
- Push notifications
- Webhook integrations
- Advanced templating with user customization
- Email analytics and delivery tracking