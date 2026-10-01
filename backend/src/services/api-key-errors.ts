export class ApiKeyError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'ApiKeyError';
  }
}

export class ApiKeyNotFoundError extends ApiKeyError {
  constructor(keyId?: string) {
    super(`API key not found${keyId ? ` with id: ${keyId}` : ''}`);
    this.name = 'ApiKeyNotFoundError';
  }
}

export class ApiKeyExpiredError extends ApiKeyError {
  constructor() {
    super('API key has expired');
    this.name = 'ApiKeyExpiredError';
  }
}

export class ApiKeyInvalidError extends ApiKeyError {
  constructor() {
    super('Invalid API key format');
    this.name = 'ApiKeyInvalidError';
  }
}

export class ApiKeyRevokedError extends ApiKeyError {
  constructor(keyId?: string) {
    super(`API key has been revoked${keyId ? ` (key ID: ${keyId})` : ''}`);
    this.name = 'ApiKeyRevokedError';
  }
}

export class ApiKeyRotationConflictError extends ApiKeyError {
  constructor(message?: string, keyId?: string) {
    super(message || `API key rotation conflict${keyId ? ` (key ID: ${keyId})` : ''}`);
    this.name = 'ApiKeyRotationConflictError';
  }
}