import path from 'path';
import crypto from 'crypto';
import { getDatabase, closeDatabase } from '../lib/database';
import { NotificationService } from '../services/notificationService';
import { NotificationEvent, NotificationType } from '../types/contract';
import { auditService } from '../services/auditService';
import { alertRouter } from '../services/alertRouter';
import { Severity, AlertStatus } from '../types/reconciliation';
import { CircuitBreaker, CircuitState } from '../lib/circuitBreaker';

// Mock nodemailer transporter
let mockSendMail: jest.Mock;
jest.mock('nodemailer', () => ({
  createTransport: () => ({
    sendMail: (...args: any[]) => mockSendMail(...args),
  }),
}));

const TEST_DB_DIR = path.resolve(__dirname, '../../.data');
const TEST_DB_PATH = path.join(TEST_DB_DIR, `test-notifications-retry-${crypto.randomUUID()}.db`);

function setupSchema(db: ReturnType<typeof getDatabase>) {
  db.exec(`
    CREATE TABLE IF NOT EXISTS notifications (
      id TEXT PRIMARY KEY,
      event_id TEXT NOT NULL,
      user_id TEXT NOT NULL,
      notification_type TEXT NOT NULL,
      status TEXT NOT NULL CHECK(status IN ('pending','sent','failed')),
      smtp_error TEXT,
      created_at TEXT NOT NULL,
      updated_at TEXT NOT NULL,
      UNIQUE(event_id, user_id)
    )
  `);
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
    )
  `);
}

function seedPrefs(userId: string) {
  const db = getDatabase();
  db.prepare(`
    INSERT OR REPLACE INTO user_notification_preferences
      (user_id, email_enabled, email_address,
       notify_invoice_funded, notify_payment_received,
       notify_dispute_opened, notify_dispute_resolved, updated_at)
    VALUES (?, 1, 'test@example.com', 1, 1, 1, 1, ?)
  `).run(userId, new Date().toISOString());
}

beforeAll(() => {
  process.env.DATABASE_PATH = TEST_DB_PATH;
  closeDatabase();
  const db = getDatabase();
  setupSchema(db);
  auditService.setAuditDir(path.join(TEST_DB_DIR, `audit-retry-${crypto.randomUUID()}`));
});

afterAll(() => {
  closeDatabase();
  try {
    const fs = require('fs');
    if (fs.existsSync(TEST_DB_PATH)) fs.unlinkSync(TEST_DB_PATH);
  } catch {}
});

describe('CircuitBreaker backoff calculation', () => {
  it('jittered backoff is in [base, base*1.5] window', () => {
    const cb = new CircuitBreaker({ initialDelayMs: 1000 });
    
    // Attempt 0: base = 1000. Window = [1000, 1500]
    let delays: number[] = [];
    for (let i = 0; i < 100; i++) {
      // @ts-ignore (access private method for testing)
      delays.push(cb.calculateBackoff(0));
    }
    expect(Math.min(...delays)).toBeGreaterThanOrEqual(1000);
    expect(Math.max(...delays)).toBeLessThanOrEqual(1500);

    // Attempt 1: base = 2000. Window = [2000, 3000]
    delays = [];
    for (let i = 0; i < 100; i++) {
      // @ts-ignore
      delays.push(cb.calculateBackoff(1));
    }
    expect(Math.min(...delays)).toBeGreaterThanOrEqual(2000);
    expect(Math.max(...delays)).toBeLessThanOrEqual(3000);
  });
});

describe('Notification Retry & Circuit Breaker', () => {
  let svc: NotificationService;

  beforeEach(() => {
    getDatabase().exec('DELETE FROM notifications');
    getDatabase().exec('DELETE FROM user_notification_preferences');
    auditService.clearAll();
    alertRouter.clearAlerts();
    
    mockSendMail = jest.fn().mockResolvedValue({ messageId: 'ok' });
    (NotificationService as any).instance = undefined;
    svc = NotificationService.getInstance();
    
    // Override CircuitBreaker delays to make tests fast without fake timers
    const cb = (svc as any).circuitBreaker as CircuitBreaker;
    (cb as any).options.initialDelayMs = 1;
    (cb as any).options.maxDelayMs = 5;
  });

  afterEach(() => {
    jest.restoreAllMocks();
  });

  it('transient failure recovers', async () => {
    const event: NotificationEvent = {
      id: `evt_transient_${crypto.randomUUID()}`,
      type: NotificationType.InvoiceFunded,
      user_id: 'USER_A',
      invoice_id: 'INV_1',
      amount: '1000',
      timestamp: Date.now(),
    };
    seedPrefs(event.user_id);

    // Fail first 2 attempts, succeed on 3rd
    mockSendMail
      .mockRejectedValueOnce(new Error('Transient Error 1'))
      .mockRejectedValueOnce(new Error('Transient Error 2'))
      .mockResolvedValueOnce({ messageId: 'ok' });

    await svc.processNotification(event);

    expect(mockSendMail).toHaveBeenCalledTimes(3);

    // Verify sent
    const row = getDatabase()
      .prepare('SELECT status FROM notifications WHERE event_id = ?')
      .get(event.id) as { status: string };
    expect(row.status).toBe('sent');

    // Audit and alerts should NOT be triggered on transient failure
    expect(auditService.getAllEntries()).toHaveLength(0);
    expect(alertRouter.getAllAlerts()).toHaveLength(0);
  });

  it('permanent failure exhausts budget, circuit opens, audit entry persisted, alert sent', async () => {
    const event: NotificationEvent = {
      id: `evt_perm_${crypto.randomUUID()}`,
      type: NotificationType.InvoiceFunded,
      user_id: 'USER_B',
      invoice_id: 'INV_2',
      amount: '2000',
      timestamp: Date.now(),
    };
    seedPrefs(event.user_id);

    // Fail all attempts
    mockSendMail.mockRejectedValue(new Error('Permanent SMTP Error'));

    await expect(svc.processNotification(event)).rejects.toThrow('Permanent SMTP Error');

    // Should have tried 4 times (1 initial + 3 retries based on config of 3 retries)
    expect(mockSendMail).toHaveBeenCalledTimes(4);

    const row = getDatabase()
      .prepare('SELECT status, smtp_error FROM notifications WHERE event_id = ?')
      .get(event.id) as { status: string; smtp_error: string };
    expect(row.status).toBe('failed');
    expect(row.smtp_error).toBe('Permanent SMTP Error');

    // Verify Audit Log
    const audits = auditService.getAllEntries();
    expect(audits).toHaveLength(1);
    expect(audits[0].operation).toBe('NOTIFICATION_DELIVERY_FAILED');
    expect(audits[0].params).toMatchObject({
      eventId: event.id,
      userId: event.user_id,
      error: 'Permanent SMTP Error',
    });

    // Verify Alert Router
    const alerts = alertRouter.getAllAlerts();
    expect(alerts).toHaveLength(1);
    expect(alerts[0].severity).toBe(Severity.HIGH);
    expect(alerts[0].message).toContain('Permanent notification drop');
    
    // Check circuit breaker state on the NotificationService using private reflection
    const cb = (svc as any).circuitBreaker as CircuitBreaker;
    // Note: It might take failureThreshold (5) to open the circuit. 
    // This single execution only fails 1 time from the circuit breaker's perspective.
    // The inner loop retries, but executeWithRetries only calls onFailure() ONCE if it exhausts retries.
    // So failureCount is 1. We need 5 total top-level failures to open the circuit.
    
    for(let i=0; i<4; i++) {
      const ev: NotificationEvent = { ...event, id: `evt_perm_${i}` };
      seedPrefs(ev.user_id);
      await expect(svc.processNotification(ev)).rejects.toThrow();
    }
    
    expect(cb.getState()).toBe(CircuitState.OPEN);
  });
});
