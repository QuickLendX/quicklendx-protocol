import { describe, it, expect, afterEach } from 'vitest';
import { getConfig, resetConfig } from '../loader';

describe('config loader', () => {
  afterEach(() => {
    resetConfig();
  });

  it('should retrieve configuration', () => {
    // Set mock environment variables so validation passes
    process.env.NODE_ENV = 'test';
    process.env.PORT = '3000';
    process.env.JWT_SECRET = 'test-secret-key-must-be-long-enough-to-pass-validation';
    process.env.JWT_EXPIRES_IN = '1h';
    process.env.API_KEY = 'test-api-key-must-be-long-enough';
    process.env.ENCRYPTION_KEY = 'test-encryption-key-must-be-long-enough-to-pass-validation';
    process.env.DATABASE_URL = 'postgresql://localhost:5432/db';
    process.env.STELLAR_NETWORK_URL = 'https://horizon-testnet.stellar.org';
    process.env.STELLAR_NETWORK_PASSPHRASE = 'Test SDF Network ; September 2015';

    const config = getConfig();
    expect(config).toBeDefined();
    expect(config.PORT).toBe(3000);
  });

  it('should support resetting config cache', () => {
    process.env.NODE_ENV = 'test';
    process.env.PORT = '3000';
    process.env.JWT_SECRET = 'test-secret-key-must-be-long-enough-to-pass-validation';
    process.env.JWT_EXPIRES_IN = '1h';
    process.env.API_KEY = 'test-api-key-must-be-long-enough';
    process.env.ENCRYPTION_KEY = 'test-encryption-key-must-be-long-enough-to-pass-validation';
    process.env.DATABASE_URL = 'postgresql://localhost:5432/db';
    process.env.STELLAR_NETWORK_URL = 'https://horizon-testnet.stellar.org';
    process.env.STELLAR_NETWORK_PASSPHRASE = 'Test SDF Network ; September 2015';

    const config1 = getConfig();
    resetConfig();
    const config2 = getConfig();
    expect(config1).not.toBe(config2);
  });
});
