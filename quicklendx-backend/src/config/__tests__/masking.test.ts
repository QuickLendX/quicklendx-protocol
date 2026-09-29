import { describe, it, expect } from 'vitest';
import {
  isSensitiveKey,
  maskSensitiveValue,
  getSafeConfig,
  formatSafeConfig,
  sanitizeErrorMessage,
} from '../masking';

describe('masking', () => {
  describe('isSensitiveKey', () => {
    it('should identify keys containing "password"', () => {
      expect(isSensitiveKey('PASSWORD')).toBe(true);
      expect(isSensitiveKey('password')).toBe(true);
      expect(isSensitiveKey('DB_PASSWORD')).toBe(true);
      expect(isSensitiveKey('user_password_hash')).toBe(true);
    });

    it('should identify keys containing "secret"', () => {
      expect(isSensitiveKey('SECRET')).toBe(true);
      expect(isSensitiveKey('secret')).toBe(true);
      expect(isSensitiveKey('JWT_SECRET')).toBe(true);
      expect(isSensitiveKey('app_secret_key')).toBe(true);
    });

    it('should identify keys containing "token"', () => {
      expect(isSensitiveKey('TOKEN')).toBe(true);
      expect(isSensitiveKey('token')).toBe(true);
      expect(isSensitiveKey('ACCESS_TOKEN')).toBe(true);
      expect(isSensitiveKey('refresh_token')).toBe(true);
    });

    it('should identify keys containing "key"', () => {
      expect(isSensitiveKey('KEY')).toBe(true);
      expect(isSensitiveKey('key')).toBe(true);
      expect(isSensitiveKey('API_KEY')).toBe(true);
      expect(isSensitiveKey('encryption_key')).toBe(true);
    });

    it('should identify keys containing "auth"', () => {
      expect(isSensitiveKey('AUTH')).toBe(true);
      expect(isSensitiveKey('auth')).toBe(true);
      expect(isSensitiveKey('AUTH_TOKEN')).toBe(true);
      expect(isSensitiveKey('oauth_secret')).toBe(true);
    });

    it('should identify keys containing "credential"', () => {
      expect(isSensitiveKey('CREDENTIAL')).toBe(true);
      expect(isSensitiveKey('credential')).toBe(true);
      expect(isSensitiveKey('AWS_CREDENTIALS')).toBe(true);
    });

    it('should identify keys containing "private"', () => {
      expect(isSensitiveKey('PRIVATE')).toBe(true);
      expect(isSensitiveKey('private')).toBe(true);
      expect(isSensitiveKey('PRIVATE_KEY')).toBe(true);
    });

    it('should identify api_key variations', () => {
      expect(isSensitiveKey('API_KEY')).toBe(true);
      expect(isSensitiveKey('api_key')).toBe(true);
      expect(isSensitiveKey('APIKEY')).toBe(true);
      expect(isSensitiveKey('api-key')).toBe(true);
    });

    it('should not identify non-sensitive keys', () => {
      expect(isSensitiveKey('PORT')).toBe(false);
      expect(isSensitiveKey('NODE_ENV')).toBe(false);
      expect(isSensitiveKey('DATABASE_URL')).toBe(false);
      expect(isSensitiveKey('LOG_LEVEL')).toBe(false);
      expect(isSensitiveKey('ENABLE_FEATURE')).toBe(false);
    });
  });

  describe('maskSensitiveValue', () => {
    it('should redact string values', () => {
      expect(maskSensitiveValue('my-secret-value')).toBe('[REDACTED]');
    });

    it('should redact number values', () => {
      expect(maskSensitiveValue(12345)).toBe('[REDACTED]');
    });

    it('should redact boolean values', () => {
      expect(maskSensitiveValue(true)).toBe('[REDACTED]');
    });

    it('should redact null values', () => {
      expect(maskSensitiveValue(null)).toBe('[REDACTED]');
    });

    it('should redact undefined values', () => {
      expect(maskSensitiveValue(undefined)).toBe('[REDACTED]');
    });

    it('should redact object values', () => {
      expect(maskSensitiveValue({ key: 'value' })).toBe('[REDACTED]');
    });

    it('should redact array values', () => {
      expect(maskSensitiveValue([1, 2, 3])).toBe('[REDACTED]');
    });
  });

  describe('getSafeConfig', () => {
    it('should redact sensitive keys', () => {
      const config = {
        PORT: 3000,
        JWT_SECRET: 'super-secret-jwt-key',
        API_KEY: 'my-api-key',
        DATABASE_PASSWORD: 'db-password',
        LOG_LEVEL: 'info',
      };

      const safe = getSafeConfig(config);

      expect(safe.PORT).toBe(3000);
      expect(safe.LOG_LEVEL).toBe('info');
      expect(safe.JWT_SECRET).toBe('[REDACTED]');
      expect(safe.API_KEY).toBe('[REDACTED]');
      expect(safe.DATABASE_PASSWORD).toBe('[REDACTED]');
    });

    it('should handle empty config', () => {
      const config = {};
      const safe = getSafeConfig(config);
      expect(safe).toEqual({});
    });

    it('should handle config with only non-sensitive keys', () => {
      const config = {
        PORT: 3000,
        NODE_ENV: 'production',
        LOG_LEVEL: 'warn',
      };

      const safe = getSafeConfig(config);
      expect(safe).toEqual(config);
    });

    it('should handle config with only sensitive keys', () => {
      const config = {
        JWT_SECRET: 'secret1',
        API_KEY: 'secret2',
        PASSWORD: 'secret3',
      };

      const safe = getSafeConfig(config);
      expect(safe).toEqual({
        JWT_SECRET: '[REDACTED]',
        API_KEY: '[REDACTED]',
        PASSWORD: '[REDACTED]',
      });
    });

    it('should handle mixed case sensitivity', () => {
      const config = {
        jwtSecret: 'secret',
        ApiKey: 'key',
        database_password: 'pass',
      };

      const safe = getSafeConfig(config);
      expect(safe.jwtSecret).toBe('[REDACTED]');
      expect(safe.ApiKey).toBe('[REDACTED]');
      expect(safe.database_password).toBe('[REDACTED]');
    });
  });

  describe('formatSafeConfig', () => {
    it('should format config as JSON with redacted values', () => {
      const config = {
        PORT: 3000,
        JWT_SECRET: 'super-secret',
        LOG_LEVEL: 'info',
      };

      const formatted = formatSafeConfig(config);
      const parsed = JSON.parse(formatted);

      expect(parsed.PORT).toBe(3000);
      expect(parsed.LOG_LEVEL).toBe('info');
      expect(parsed.JWT_SECRET).toBe('[REDACTED]');
    });

    it('should produce valid JSON', () => {
      const config = {
        API_KEY: 'secret',
        PORT: 8080,
      };

      const formatted = formatSafeConfig(config);
      expect(() => JSON.parse(formatted)).not.toThrow();
    });
  });

  describe('sanitizeErrorMessage', () => {
    it('should redact sensitive values from error messages', () => {
      const config = {
        JWT_SECRET: 'my-super-secret-jwt-key',
        API_KEY: 'api-key-12345',
        PORT: 3000,
      };

      const error = new Error(
        'Failed to connect with JWT: my-super-secret-jwt-key and API key: api-key-12345'
      );

      const sanitized = sanitizeErrorMessage(error, config);

      expect(sanitized).not.toContain('my-super-secret-jwt-key');
      expect(sanitized).not.toContain('api-key-12345');
      expect(sanitized).toContain('[REDACTED]');
    });

    it('should handle errors without sensitive data', () => {
      const config = {
        JWT_SECRET: 'secret',
        PORT: 3000,
      };

      const error = new Error('Connection failed on port 3000');
      const sanitized = sanitizeErrorMessage(error, config);

      expect(sanitized).toBe('Connection failed on port 3000');
    });

    it('should handle multiple occurrences of the same secret', () => {
      const config = {
        API_KEY: 'secret123',
      };

      const error = new Error('secret123 was used twice: secret123');
      const sanitized = sanitizeErrorMessage(error, config);

      expect(sanitized).toBe('[REDACTED] was used twice: [REDACTED]');
    });

    it('should not redact non-sensitive values', () => {
      const config = {
        PORT: 3000,
        JWT_SECRET: 'secret',
      };

      const error = new Error('Port 3000 is already in use');
      const sanitized = sanitizeErrorMessage(error, config);

      expect(sanitized).toContain('3000');
    });

    it('should handle empty config', () => {
      const config = {};
      const error = new Error('Some error message');
      const sanitized = sanitizeErrorMessage(error, config);

      expect(sanitized).toBe('Some error message');
    });

    it('should handle special regex characters in secrets', () => {
      const config = {
        API_KEY: 'key.with*special+chars?',
      };

      const error = new Error('Failed with key: key.with*special+chars?');
      const sanitized = sanitizeErrorMessage(error, config);

      expect(sanitized).not.toContain('key.with*special+chars?');
      expect(sanitized).toContain('[REDACTED]');
    });
  });
});
