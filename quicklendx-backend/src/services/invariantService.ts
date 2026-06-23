/**
 * Invariant checking service.
 *
 * Runs a suite of domain-invariant checks against the current state
 * and exposes a running violation counter (used by MetricsService).
 */

let violationCount = 0;

/**
 * Run every registered invariant check.
 * Violations increment the internal counter.
 */
export async function runAll(): Promise<void> {
  // TODO: implement actual invariant checks against the data layer
}

/**
 * Return the total number of invariant violations detected since
 * the last service restart.
 */
export async function getViolationCount(): Promise<number> {
  return violationCount;
}

/** Exposed for tests / admin tooling. */
export function resetViolationCount(): void {
  violationCount = 0;
}
