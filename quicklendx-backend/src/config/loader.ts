import * as dotenv from 'dotenv';
import { EventEmitter } from 'events';
import { z } from 'zod';
import { ConfigSchema, ProductionConfigSchema, type Config, type Profile } from './schema';
import { getSafeConfig } from './masking';

const CONFIG_RELOAD_EVENT = 'reload';

const emitter = new EventEmitter();
emitter.setMaxListeners(64);

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
 * Returns the list of config keys that are safe for hot reload.
 * Determined by the `hotReloadable:true` tag in each field's `.describe()` metadata.
 */
function getHotReloadableKeys(): string[] {
  const shape = ConfigSchema.shape as Record<string, z.ZodTypeAny>;
  return Object.entries(shape)
    .filter(([_, fieldSchema]) => {
      const def = (fieldSchema as unknown as { _def?: { description?: string } })._def;
      return def?.description?.includes('hotReloadable:true') ?? false;
    })
    .map(([key]) => key);
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
      const safeErrors = error.errors.map((err) => {
        const path = err.path.join('.');
        const message = err.message;
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
    loadEnvFile(profile);
    const config = validateConfig(profile);

    if (profile !== 'test') {
      console.log(`\u2713 Configuration loaded successfully for profile: ${profile}`);
      console.log('Configuration (sensitive values redacted):');
      console.log(JSON.stringify(getSafeConfig(config as unknown as Record<string, unknown>), null, 2));
    }

    return config;
  } catch (error) {
    if (error instanceof ConfigValidationError) {
      console.error('\n\u274c CONFIGURATION ERROR\n');
      console.error(error.message);
      console.error('\nApplication cannot start with invalid configuration.');
      process.exit(1);
    }

    console.error('\n\u274c UNEXPECTED ERROR DURING CONFIGURATION LOAD\n');
    console.error(error);
    process.exit(1);
  }
}

let configInstance: Config | null = null;
let signalHandlerBound = false;

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

/**
 * Subscribes to configuration reload events.
 * Returns an unsubscribe function.
 */
export function onReload(cb: (config: Config) => void): () => void {
  emitter.on(CONFIG_RELOAD_EVENT, cb);
  return () => {
    emitter.off(CONFIG_RELOAD_EVENT, cb);
  };
}

/**
 * Reloads hot-reloadable configuration keys from the environment.
 * Non-reloadable keys (secrets, database URLs, etc.) are preserved from the
 * current config. If validation fails the current config is kept and an error
 * is logged; the error is not fatal.
 *
 * Never logs secret values.
 */
export function reloadConfig(): Config {
  const profile = (process.env.NODE_ENV as Profile) || 'development';

  let fresh: Config;
  try {
    loadEnvFile(profile);
    fresh = validateConfig(profile);
  } catch (error) {
    const message =
      error instanceof ConfigValidationError
        ? error.message
        : `Unexpected error during config reload: ${String(error)}`;
    console.error(`\nConfig reload failed — keeping current config.\n${message}\n`);
    return getConfig();
  }

  const current = getConfig();
  const hotKeys = getHotReloadableKeys();

  const merged = { ...current } as Record<string, unknown>;
  for (const key of hotKeys) {
    if (key in fresh) {
      merged[key] = (fresh as unknown as Record<string, unknown>)[key];
    }
  }

  const reloaded = merged as unknown as Config;

  if (profile !== 'test') {
    console.log(`\u2713 Configuration reloaded via SIGHUP`);
    console.log(JSON.stringify(getSafeConfig(merged), null, 2));
  }

  configInstance = reloaded;
  emitter.emit(CONFIG_RELOAD_EVENT, reloaded);
  return reloaded;
}

/**
 * Binds a SIGHUP handler that triggers a safe configuration reload.
 * Safe to call multiple times — only one listener is registered.
 */
export function setupSignalHandlers(): void {
  if (signalHandlerBound) {
    return;
  }
  signalHandlerBound = true;

  process.on('SIGHUP', () => {
    try {
      reloadConfig();
    } catch {
      // Defensive: reloadConfig already handles errors gracefully
    }
  });
}

/**
 * Returns the set of currently registered reload subscriber count.
 * Useful for assertions in tests.
 */
export function reloadListenerCount(): number {
  return emitter.listenerCount(CONFIG_RELOAD_EVENT);
}

/**
 * Resets signal handler state so that setupSignalHandlers() can re-bind.
 * Intended for test teardown.
 */
export function resetSignalHandlers(): void {
  signalHandlerBound = false;
}

/**
 * Removes all reload subscribers. Intended for test teardown.
 */
export function clearReloadListeners(): void {
  emitter.removeAllListeners(CONFIG_RELOAD_EVENT);
}
