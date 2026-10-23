import request from 'supertest';
import express from 'express';
import notificationRoutes from '../routes/v1/notifications';
import { notificationService } from '../services/notificationService';
import {
  computeUnsubscribeToken,
  verifyUnsubscribeToken,
  sign,
  verify,
  resetTokenCache,
} from '../services/notificationTokens';
import { FrozenClock } from '../services/clock';
import { load } from '../config';
import { NotificationEvent, NotificationType } from '../types/contract';
import { getDatabase } from '../lib/database';

const app = express();
app.use(express.json());
app.use('/api/v1/notifications', notificationRoutes);

describe('Notifications Security & IDOR Protections (Issue #2907)', () => {
  const GA = 'GA1111111111111111111111111111111111111111111111111111111111';
  const GB = 'GB2222222222222222222222222222222222222222222222222222222222';

  beforeEach(() => {
    resetTokenCache();
    const db = getDatabase();
    db.exec(`
      CREATE TABLE IF NOT EXISTS user_notification_preferences (
        user_id TEXT PRIMARY KEY,
        email_enabled INTEGER NOT NULL DEFAULT 1,
        email_address TEXT,
        notify_invoice_funded INTEGER NOT NULL DEFAULT 1,
        notify_payment_received INTEGER NOT NULL DEFAULT 1,
        notify_dispute_opened INTEGER NOT NULL DEFAULT 1,
        notify_dispute_resolved INTEGER NOT NULL DEFAULT 1,
        updated_at TEXT NOT NULL
      );
    `);
    db.exec('DELETE FROM user_notification_preferences');

    // Seed GA preferences
    notificationService.updateUserPreferences(GA, {
      email_enabled: true,
      email_address: 'ga_original@example.com',
      notifications: {
        [NotificationType.InvoiceFunded]: true,
        [NotificationType.PaymentReceived]: true,
        [NotificationType.DisputeOpened]: true,
        [NotificationType.DisputeResolved]: true,
      },
    });
  });

  describe('GET /api/v1/notifications/preferences/:userId (Authentication & Ownership)', () => {
    it('returns 401 when unauthenticated', async () => {
      const res = await request(app).get(`/api/v1/notifications/preferences/${GA}`);
      expect(res.status).toBe(401);
    });

    it('returns 403 when authenticated as a different principal (GB accessing GA)', async () => {
      const res = await request(app)
        .get(`/api/v1/notifications/preferences/${GA}`)
        .set('Authorization', `Bearer ${GB}`);
      expect(res.status).toBe(403);
      expect(res.body.error).toBe('FORBIDDEN');
    });

    it('returns 200 with the full email address when authenticated as the owner (GA)', async () => {
      const res = await request(app)
        .get(`/api/v1/notifications/preferences/${GA}`)
        .set('Authorization', `Bearer ${GA}`);
      expect(res.status).toBe(200);
      expect(res.body.email_address).toBe('ga_original@example.com');
      expect(res.body.email_enabled).toBe(true);
    });
  });

  describe('PUT /api/v1/notifications/preferences/:userId (Email Verification Flow)', () => {
    it('returns 401 when unauthenticated', async () => {
      const res = await request(app)
        .put(`/api/v1/notifications/preferences/${GA}`)
        .send({ email_address: 'attacker@evil.test' });
      expect(res.status).toBe(401);
    });

    it('returns 403 when principal GB attempts to modify GA', async () => {
      const res = await request(app)
        .put(`/api/v1/notifications/preferences/${GA}`)
        .set('Authorization', `Bearer ${GB}`)
        .send({ email_address: 'attacker@evil.test' });
      expect(res.status).toBe(403);
    });

    it('returns 202 verification_required and leaves email_address unchanged until verified', async () => {
      const res = await request(app)
        .put(`/api/v1/notifications/preferences/${GA}`)
        .set('Authorization', `Bearer ${GA}`)
        .send({ email_address: 'ga_new@example.com' });

      expect(res.status).toBe(202);
      expect(res.body.verification_required).toBe(true);

      // Verify persisted email is still the original address
      const prefs = notificationService.getUserPreferencesPublic(GA);
      expect(prefs?.email_address).toBe('ga_original@example.com');
    });
  });

  describe('Email Verification Token Lifecycle', () => {
    it('rejects verify token for GA when submitted for GB (403), after 24h (400 TOKEN_EXPIRED), and on second use (400 TOKEN_USED)', async () => {
      const clock = new FrozenClock(1_000_000);
      const { verificationToken } = notificationService.requestEmailChange(
        GA,
        'ga_verified@example.com',
        clock
      );

      // 1. Rejected for GB (403)
      const resGB = notificationService.verifyAndActivateEmail(GB, verificationToken, clock);
      expect(resGB.status).toBe(403);
      expect(resGB.success).toBe(false);

      // 2. Rejected after 24 hours (400 TOKEN_EXPIRED)
      const expiredClock = new FrozenClock(1_000_000 + 25 * 60 * 60 * 1000); // 25h later
      const resExpired = notificationService.verifyAndActivateEmail(
        GA,
        verificationToken,
        expiredClock
      );
      expect(resExpired.status).toBe(400);
      expect(resExpired.error).toBe('TOKEN_EXPIRED');

      // 3. First valid use switches the address
      const resValid = notificationService.verifyAndActivateEmail(GA, verificationToken, clock);
      expect(resValid.status).toBe(200);
      expect(resValid.success).toBe(true);

      const updatedPrefs = notificationService.getUserPreferencesPublic(GA);
      expect(updatedPrefs?.email_address).toBe('ga_verified@example.com');

      // 4. Rejected on second use (400 TOKEN_USED)
      const resSecond = notificationService.verifyAndActivateEmail(GA, verificationToken, clock);
      expect(resSecond.status).toBe(400);
      expect(resSecond.error).toBe('TOKEN_USED');
    });
  });

  describe('POST /api/v1/notifications/unsubscribe/:userId', () => {
    it('returns 403 when no token is provided', async () => {
      const res = await request(app).post(`/api/v1/notifications/unsubscribe/${GA}`);
      expect(res.status).toBe(403);
      expect(res.body.error).toBe('INVALID_UNSUBSCRIBE_TOKEN');
    });

    it('returns 403 with a token computed for a different user (GB)', async () => {
      const tokenGB = computeUnsubscribeToken(GB);
      const res = await request(app)
        .post(`/api/v1/notifications/unsubscribe/${GA}`)
        .query({ token: tokenGB });
      expect(res.status).toBe(403);
      expect(res.body.error).toBe('INVALID_UNSUBSCRIBE_TOKEN');
    });

    it('returns 200 and disables email when correct token is provided, and remains idempotent on repeated calls', async () => {
      const tokenGA = computeUnsubscribeToken(GA);

      // First call
      const res1 = await request(app)
        .post(`/api/v1/notifications/unsubscribe/${GA}`)
        .query({ token: tokenGA });
      expect(res1.status).toBe(200);
      expect(res1.body.email_enabled).toBe(false);

      const prefs1 = notificationService.getUserPreferencesPublic(GA);
      expect(prefs1?.email_enabled).toBe(false);

      // Repeated call (idempotent)
      const res2 = await request(app)
        .post(`/api/v1/notifications/unsubscribe/${GA}`)
        .query({ token: tokenGA });
      expect(res2.status).toBe(200);
      expect(res2.body.email_enabled).toBe(false);
    });
  });

  describe('Email Template Verification', () => {
    it('renders InvoiceFunded template with an unsubscribe URL whose token validates for that user', () => {
      const event: NotificationEvent = {
        id: 'evt-101',
        type: NotificationType.InvoiceFunded,
        user_id: GA,
        invoice_id: 'inv-101',
        amount: '2500',
        timestamp: Date.now(),
      };

      const template = notificationService.getEmailTemplate(event);

      // Verify unsubscribe link in HTML
      const matchHtml = template.html.match(/token=([A-Za-z0-9_-]+)/);
      expect(matchHtml).not.toBeNull();
      const tokenInHtml = matchHtml![1];
      expect(verifyUnsubscribeToken(GA, tokenInHtml)).toBe(true);

      // Verify unsubscribe link in text
      const matchText = template.text.match(/token=([A-Za-z0-9_-]+)/);
      expect(matchText).not.toBeNull();
      const tokenInText = matchText![1];
      expect(verifyUnsubscribeToken(GA, tokenInText)).toBe(true);
    });
  });

  describe('Configuration Security Invariant', () => {
    it('throws in production when NOTIFICATION_LINK_SECRET is missing', () => {
      const prodEnv: NodeJS.ProcessEnv = {
        NODE_ENV: 'production',
        ADMIN_API_KEY: 'a'.repeat(32),
        EXPORT_SECRET: 'b'.repeat(32),
        // NOTIFICATION_LINK_SECRET deliberately omitted
      };

      expect(() => load(prodEnv)).toThrow(/NOTIFICATION_LINK_SECRET/);
    });
  });
});
