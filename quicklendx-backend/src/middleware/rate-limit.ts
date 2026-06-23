import { type Config, onReload, getConfig } from '../config';

let points = 1000;
let requestsPerMinute = 100;

/**
 * Returns the current rate-limit point budget.
 */
export function getRateLimitPoints(): number {
  return points;
}

/**
 * Returns the current max requests per minute.
 */
export function getMaxRequestsPerMinute(): number {
  return requestsPerMinute;
}

/**
 * Synchronizes internal rate-limit state from the given config.
 */
export function updateRateLimitConfig(config: Config): void {
  points = config.RATE_LIMIT_POINTS;
  requestsPerMinute = config.MAX_REQUESTS_PER_MINUTE;
}

/**
 * Initialises rate-limit state from the current config and subscribes
 * to future reloads so limits stay in sync without a restart.
 */
export function setupRateLimitReload(): () => void {
  updateRateLimitConfig(getConfig());
  return onReload((config) => {
    updateRateLimitConfig(config);
  });
}
