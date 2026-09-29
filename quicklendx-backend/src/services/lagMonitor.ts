import { type Config, onReload } from '../config';

let warnThreshold = 10;
let criticalThreshold = 100;
let latestLagLedgers = 0;

/**
 * Updates the lag thresholds from configuration.
 * Called automatically on reload via onReload subscription.
 */
export function updateThresholds(config: Config): void {
  warnThreshold = config.LAG_WARN_THRESHOLD;
  criticalThreshold = config.LAG_CRITICAL_THRESHOLD;
}

/**
 * Returns the current warn threshold in ledgers.
 */
export function getWarnThreshold(): number {
  return warnThreshold;
}

/**
 * Returns the current critical threshold in ledgers.
 */
export function getCriticalThreshold(): number {
  return criticalThreshold;
}

/**
 * Returns the latest observed lag value in ledgers.
 */
export async function getLagLedgers(): Promise<number> {
  return latestLagLedgers;
}

/**
 * Sets the latest observed lag value (called by ingest pipeline).
 */
export function observeLag(ledgers: number): void {
  latestLagLedgers = ledgers;
}

/**
 * Determines the lag severity based on current thresholds.
 */
export function getLagSeverity(ledgers: number): 'ok' | 'warn' | 'critical' {
  if (ledgers >= criticalThreshold) return 'critical';
  if (ledgers >= warnThreshold) return 'warn';
  return 'ok';
}

/**
 * Subscribes to config reload events so that lag thresholds stay in sync
 * whenever the operator sends SIGHUP.
 */
export function setupLagMonitorReload(): () => void {
  return onReload((config) => {
    updateThresholds(config);
  });
}
