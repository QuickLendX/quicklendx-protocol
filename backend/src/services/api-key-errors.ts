import { ApiKeyErrorCode } from './api-key-error-codes';

export class ApiKeyError extends Error {
  public readonly code: string;

  constructor(message: string, code: string = ApiKeyErrorCode.VALIDATION) {
    super(message);
    this.name = 'ApiKeyError';
    this.code = code;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export class ApiKeyNotFoundError extends ApiKeyError {
  public readonly keyId?: string;

  constructor(keyId?: string) {
    super(keyId ? `API key not found: ${keyId}` : 'API key not found', ApiKeyErrorCode.NOT_FOUND);
    this.name = 'ApiKeyNotFoundError';
    this.keyId = keyId;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export class ApiKeyRevokedError extends ApiKeyError {
  public readonly keyId?: string;

  constructor(keyId?: string) {
    super(keyId ? `API key is revoked: ${keyId}` : 'API key is revoked', ApiKeyErrorCode.REVOKED);
    this.name = 'ApiKeyRevokedError';
    this.keyId = keyId;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export class ApiKeyRotationConflictError extends ApiKeyError {
  public readonly keyId?: string;

  constructor(message: string, keyId?: string) {
    super(message, ApiKeyErrorCode.ROTATION_CONFLICT);
    this.name = 'ApiKeyRotationConflictError';
    this.keyId = keyId;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}
