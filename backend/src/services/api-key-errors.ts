import { ApiKeyErrorCode } from './api-key-error-codes';

export class ApiKeyError extends Error {
  public readonly code: string;
  constructor(message: string, code: string = ApiKeyErrorCode.INTERNAL) {
    super(message);
    this.name = 'ApiKeyError';
    this.code = code;
  }
}

export class ApiKeyNotFoundError extends ApiKeyError {
  constructor(keyId: string) {
    super(`API key not found: ${keyId}`, ApiKeyErrorCode.NOT_FOUND);
    this.name = 'ApiKeyNotFoundError';
  }
}

export class ApiKeyRevokedError extends ApiKeyError {
  constructor(keyId: string) {
    super(`Cannot rotate a revoked key: ${keyId}`, ApiKeyErrorCode.REVOKED);
    this.name = 'ApiKeyRevokedError';
  }
}

export class ApiKeyRotationConflictError extends ApiKeyError {
  public readonly keyId?: string;
  constructor(message: string, keyId?: string) {
    super(message, ApiKeyErrorCode.ROTATION_CONFLICT);
    this.name = 'ApiKeyRotationConflictError';
    this.keyId = keyId;
  }
}
