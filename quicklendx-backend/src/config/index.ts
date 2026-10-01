/**
 * Configuration Management System
 *
 * This module provides a production-grade configuration system with:
 * - Strict type validation using Zod
 * - Fail-fast behavior on invalid configuration
 * - Profile-based configuration (development, test, production)
 * - Automatic secret redaction in logs and error messages
 * - Comprehensive error handling
 * - SIGHUP-triggered hot reload for safe-to-change runtime configuration
 */

export {
  getConfig,
  loadConfig,
  resetConfig,
  reloadConfig,
  onReload,
  setupSignalHandlers,
  reloadListenerCount,
  resetSignalHandlers,
  clearReloadListeners,
  ConfigValidationError,
} from './loader';
export { getSafeConfig, isSensitiveKey, maskSensitiveValue, formatSafeConfig } from './masking';
export type { Config, Profile, LogLevel } from './schema';
