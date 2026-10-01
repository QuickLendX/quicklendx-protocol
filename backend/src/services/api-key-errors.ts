import { ApiKeyErrorCode } from './api-key-error-codes';

/** Base class for domain errors raised by the API key service. */
export class ApiKeyError extends Error {
  constructor(
    message: string,
    public readonly code: string = ApiKeyErrorCode.INTERNAL,
    public readonly keyId?: string
  ) {
    super(message);
    this.name = new.target.name;
    Object.setPrototypeOf(this, new.target.prototype);
  }
}

export class ApiKeyNotFoundError extends ApiKeyError {
  constructor(keyId: string) {
    super(`API key not found: ${keyId}`, ApiKeyErrorCode.NOT_FOUND, keyId);
  }
}

export class ApiKeyRevokedError extends ApiKeyError {
  constructor(keyId: string) {
    super(`API key is revoked: ${keyId}`, ApiKeyErrorCode.REVOKED, keyId);
  }
}

export class ApiKeyRotationConflictError extends ApiKeyError {
  constructor(message: string, keyId?: string) {
    super(message, ApiKeyErrorCode.ROTATION_CONFLICT, keyId);
  }
}
