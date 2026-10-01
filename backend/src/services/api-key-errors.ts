export class ApiKeyError extends Error {
  public code: string;
  constructor(message: string, code: string = 'API_KEY_ERROR') {
    super(message);
    this.name = 'ApiKeyError';
    this.code = code;
  }
}

export class ApiKeyNotFoundError extends ApiKeyError {
  constructor(public keyId: string) {
    super(`API key ${keyId} not found`, 'NOT_FOUND');
    this.name = 'ApiKeyNotFoundError';
  }
}

export class ApiKeyRevokedError extends ApiKeyError {
  constructor(public keyId: string) {
    super(`API key ${keyId} has been revoked`, 'REVOKED');
    this.name = 'ApiKeyRevokedError';
  }
}

export class ApiKeyRotationConflictError extends ApiKeyError {
  constructor(message: string, public keyId: string) {
    super(message, 'ROTATION_CONFLICT');
    this.name = 'ApiKeyRotationConflictError';
  }
}
