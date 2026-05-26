/**
 * Bid endpoint fixtures
 */

export const validBid = {
  id: 'dd0e8400-e29b-41d4-a716-446655440020',
  invoiceId: 'aa0e8400-e29b-41d4-a716-446655440010',
  investorId: '550e8400-e29b-41d4-a716-446655440000',
  amount: '10000.00',
  interestRate: 8.5,
  status: 'pending',
  createdAt: '2024-01-20T10:00:00Z',
};

export const acceptedBid = {
  id: 'ee0e8400-e29b-41d4-a716-446655440021',
  invoiceId: 'aa0e8400-e29b-41d4-a716-446655440010',
  investorId: '550e8400-e29b-41d4-a716-446655440000',
  amount: '10000.00',
  interestRate: 7.5,
  status: 'accepted',
  createdAt: '2024-01-19T14:30:00Z',
};

export const rejectedBid = {
  id: 'ff0e8400-e29b-41d4-a716-446655440022',
  invoiceId: 'bb0e8400-e29b-41d4-a716-446655440011',
  investorId: '550e8400-e29b-41d4-a716-446655440000',
  amount: '25000.50',
  interestRate: 12.0,
  status: 'rejected',
  createdAt: '2024-01-18T09:15:00Z',
};

export const validCreateBidRequest = {
  invoiceId: 'aa0e8400-e29b-41d4-a716-446655440010',
  amount: '10000.00',
  interestRate: 8.5,
};

export const invalidBidRequest = {
  invoiceId: 'not-a-uuid',
  amount: '-1000',
  interestRate: 150, // Over 100%
};

export const invalidBidError = {
  error: 'VALIDATION_ERROR',
  message: 'Invalid request parameters',
  details: {
    invoiceId: 'Must be a valid UUID',
    amount: 'Must be a positive number',
    interestRate: 'Must be between 0 and 100',
  },
};
