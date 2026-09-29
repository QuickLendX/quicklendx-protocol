/**
 * QuickLendX Backend — Application Entry Point
 *
 * Boots the scheduler and registers recurring jobs.
 */

import dotenv from 'dotenv';
import { Scheduler } from './lib/scheduler';
import { cleanupAll } from './services/retention';
import { runAll } from './services/invariantService';
import { run } from './services/reconciliationWorker';

dotenv.config();

const NODE_ENV = process.env.NODE_ENV ?? 'development';

/* ------------------------------------------------------------------ */
/*  Scheduler bootstrap                                                */
/* ------------------------------------------------------------------ */

const scheduler = new Scheduler({
  pollIntervalMs: 10_000,
  leaseDurationMs: 60_000,
});

scheduler
  .register('retention-cleanup', '0 0 * * *', cleanupAll)
  .register('invariant-check', '*/5 * * * *', runAll)
  .register('reconciliation', '*/5 * * * *', run);

/* Do not start the polling loop during automated tests unless the
   caller explicitly enables it via SCHEDULER_ENABLED=true. */
const schedulerEnabled =
  NODE_ENV !== 'test' || process.env.SCHEDULER_ENABLED === 'true';

if (schedulerEnabled) {
  scheduler.start();
}

/* ------------------------------------------------------------------ */
/*  Graceful shutdown                                                  */
/* ------------------------------------------------------------------ */

async function shutdown(signal: string): Promise<void> {
  console.log(`[app] received ${signal} — shutting down`);
  await scheduler.stop();
  scheduler.close();
  process.exit(0);
}

process.on('SIGTERM', () => void shutdown('SIGTERM'));
process.on('SIGINT', () => void shutdown('SIGINT'));

export { scheduler };
