/**
 * User endpoint fixtures
 */

export const validUserProfile = {
  id: '550e8400-e29b-41d4-a716-446655440000',
  email: 'investor@example.com',
  role: 'investor',
  kycStatus: 'approved',
  createdAt: '2024-01-15T10:30:00Z',
  updatedAt: '2024-01-15T10:30:00Z',
};

export const businessUserProfile = {
  id: '770e8400-e29b-41d4-a716-446655440001',
  email: 'business@example.com',
  role: 'business',
  kycStatus: 'approved',
  createdAt: '2024-01-10T08:00:00Z',
  updatedAt: '2024-01-10T08:00:00Z',
};

export const adminUserProfile = {
  id: '880e8400-e29b-41d4-a716-446655440002',
  email: 'admin@quicklendx.com',
  role: 'admin',
  kycStatus: 'approved',
  createdAt: '2024-01-01T00:00:00Z',
  updatedAt: '2024-01-01T00:00:00Z',
};

export const pendingKycUserProfile = {
  id: '990e8400-e29b-41d4-a716-446655440003',
  email: 'pending@example.com',
  role: 'investor',
  kycStatus: 'pending',
  createdAt: '2024-01-25T12:00:00Z',
  updatedAt: '2024-01-25T12:00:00Z',
};

export const unauthorizedError = {
  error: 'UNAUTHORIZED',
  message: 'Authentication required',
};
