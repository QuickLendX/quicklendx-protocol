/** Stable machine-readable codes returned by the API key endpoints. */
export enum ApiKeyErrorCode {
  NOT_FOUND = 'API_KEY_NOT_FOUND',
  REVOKED = 'API_KEY_REVOKED',
  ROTATION_CONFLICT = 'API_KEY_ROTATION_CONFLICT',
  VALIDATION = 'API_KEY_VALIDATION_ERROR',
  INTERNAL = 'API_KEY_INTERNAL_ERROR',
}
