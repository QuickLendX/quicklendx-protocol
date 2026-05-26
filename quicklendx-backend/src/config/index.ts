/**
 * Configuration Management System
 * 
 * This module provides a production-grade configuration system with:
 * - Strict type validation using Zod
 * - Fail-fast behavior on invalid configuration
 * - Profile-based configuration (development, test, production)
 * - Automatic secret redaction in logs and error messages
 * - Comprehensive error handling
 */

export { getConfig, loadConfig, resetConfig, ConfigValidationError } from './loader';
export { getSafeConfig, isSensitiveKey, maskSensitiveValue, formatSafeConfig } from './masking';
export type { Config, Profile, LogLevel } from './schema';
