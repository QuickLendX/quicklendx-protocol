import * as dotenv from 'dotenv';
import { z } from 'zod';
import { ConfigSchema, ProductionConfigSchema, type Config, type Profile } from './schema';
import { getSafeConfig, sanitizeErrorMessage } from './masking';

/**
 * Configuration validation error with safe error messages
 */
export class ConfigValidationError extends Error {
  constructor(
    message: string,
    public readonly errors: z.ZodError,
    public readonly profile: Profile
  ) {
    super(message);
    this.name = 'ConfigValidationError';
  }
}

/**
 * Loads environment variables from .env files
 */
function loadEnvFile(profile: Profile): void {
  const envFiles = ['.env', `.env.${profile}`, `.env.${profile}.local`];

  for (const file of envFiles) {
    dotenv.config({ path: file });
  }
}

/**
 * Validates configuration based on the current profile
 */
function validateConfig(profile: Profile): Config {
  const schema = profile === 'production' ? ProductionConfigSchema : ConfigSchema;

  try {
    return schema.parse(process.env);
  } catch (error) {
    if (error instanceof z.ZodError) {
      // Create safe error messages that don't leak sensitive values
      const safeErrors = error.errors.map((err) => {
        const path = err.path.join('.');
        const message = err.message;
        
        // Don't include the actual value in error messages
        return `  - ${path}: ${message}`;
      });

      const errorMessage = [
        `Configuration validation failed for profile "${profile}":`,
        ...safeErrors,
        '',
        'Please check your environment variables and try again.',
      ].join('\n');

      throw new ConfigValidationError(errorMessage, error, profile);
    }
    throw error;
  }
}

/**
 * Loads and validates configuration with fail-fast behavior
 */
export function loadConfig(): Config {
  const profile = (process.env.NODE_ENV as Profile) || 'development';

  try {
    // Load environment files
    loadEnvFile(profile);

    // Validate configuration
    const config = validateConfig(profile);

    // Log successful load (with sensitive data masked)
    if (profile !== 'test') {
      console.log(`✓ Configuration loaded successfully for profile: ${profile}`);
      console.log('Configuration (sensitive values redacted):');
      console.log(JSON.stringify(getSafeConfig(config), null, 2));
    }

    return config;
  } catch (error) {
    if (error instanceof ConfigValidationError) {
      console.error('\n❌ CONFIGURATION ERROR\n');
      console.error(error.message);
      console.error('\nApplication cannot start with invalid configuration.');
      process.exit(1);
    }

    // Handle unexpected errors
    console.error('\n❌ UNEXPECTED ERROR DURING CONFIGURATION LOAD\n');
    console.error(error);
    process.exit(1);
  }
}

/**
 * Singleton configuration instance
 */
let configInstance: Config | null = null;

/**
 * Gets the current configuration (loads if not already loaded)
 */
export function getConfig(): Config {
  if (!configInstance) {
    configInstance = loadConfig();
  }
  return configInstance;
}

/**
 * Resets the configuration instance (useful for testing)
 */
export function resetConfig(): void {
  configInstance = null;
}
