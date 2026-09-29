/**
 * Authentication endpoint fixtures
 */

export const validLoginRequest = {
  email: 'investor@example.com',
  password: 'SecurePassword123!',
};

export const validLoginResponse = {
  token: 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiI1NTBlODQwMC1lMjliLTQxZDQtYTcxNi00NDY2NTU0NDAwMDAiLCJlbWFpbCI6Imludmvzdg9yQGV4YW1wbGUuY29tIiwicm9sZSI6Imludmvzdg9yIiwiaWF0IjoxNjQwOTk1MjAwfQ.dummySignature',
  refreshToken: 'refresh_token_example_12345',
  user: {
    id: '550e8400-e29b-41d4-a716-446655440000',
    email: 'investor@example.com',
    role: 'investor',
    kycStatus: 'approved',
    createdAt: '2024-01-15T10:30:00Z',
    updatedAt: '2024-01-15T10:30:00Z',
  },
};

export const validRegisterRequest = {
  email: 'newuser@example.com',
  password: 'SecurePassword123!',
  role: 'investor',
};

export const validRegisterResponse = {
  token: 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiI2NjBmOTUwMC1mMzBjLTQyZTUtYjgyNy01NTc3NjY1NTExMTEiLCJlbWFpbCI6Im5ld3VzZXJAZXhhbXBsZS5jb20iLCJyb2xlIjoiaW52ZXN0b3IiLCJpYXQiOjE2NDA5OTUyMDB9.dummySignature',
  user: {
    id: '660f9500-f30c-42e5-b827-557766551111',
    email: 'newuser@example.com',
    role: 'investor',
    kycStatus: 'pending',
    createdAt: '2024-01-20T14:00:00Z',
    updatedAt: '2024-01-20T14:00:00Z',
  },
};

export const invalidCredentialsError = {
  error: 'AUTHENTICATION_FAILED',
  message: 'Invalid email or password',
};

export const validationError = {
  error: 'VALIDATION_ERROR',
  message: 'Invalid request parameters',
  details: {
    email: 'Invalid email format',
  },
};
