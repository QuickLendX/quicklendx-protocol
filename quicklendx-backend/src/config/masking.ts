/**
 * Sensitive key patterns that should be redacted in logs
 */
const SENSITIVE_PATTERNS = [
  /password/i,
  /secret/i,
  /token/i,
  /key/i,
  /auth/i,
  /credential/i,
  /private/i,
  /api[_-]?key/i,
];

/**
 * Determines if a configuration key contains sensitive information
 */
export function isSensitiveKey(key: string): boolean {
  return SENSITIVE_PATTERNS.some((pattern) => pattern.test(key));
}

/**
 * Redacts sensitive values from configuration objects
 */
export function maskSensitiveValue(value: unknown): string {
  if (value === undefined || value === null) {
    return '[REDACTED]';
  }
  return '[REDACTED]';
}

/**
 * Creates a safe representation of configuration for logging
 */
export function getSafeConfig<T extends Record<string, unknown>>(config: T): Record<string, unknown> {
  const safeConfig: Record<string, unknown> = {};

  for (const [key, value] of Object.entries(config)) {
    if (isSensitiveKey(key)) {
      safeConfig[key] = maskSensitiveValue(value);
    } else {
      safeConfig[key] = value;
    }
  }

  return safeConfig;
}

/**
 * Redacts sensitive information from error messages
 */
export function sanitizeErrorMessage(error: Error, config: Record<string, unknown>): string {
  let message = error.message;

  // Replace any sensitive values that might appear in the error message
  for (const [key, value] of Object.entries(config)) {
    if (isSensitiveKey(key) && typeof value === 'string' && value.length > 0) {
      // Replace the actual value with [REDACTED] in error messages
      const regex = new RegExp(escapeRegExp(value), 'g');
      message = message.replace(regex, '[REDACTED]');
    }
  }

  return message;
}

/**
 * Escapes special regex characters in a string
 */
function escapeRegExp(str: string): string {
  return str.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Formats configuration for safe console output
 */
export function formatSafeConfig(config: Record<string, unknown>): string {
  const safeConfig = getSafeConfig(config);
  return JSON.stringify(safeConfig, null, 2);
}
