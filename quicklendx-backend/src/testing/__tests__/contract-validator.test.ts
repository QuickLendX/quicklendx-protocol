import { describe, it, expect, beforeAll } from 'vitest';
import { ContractValidator } from '../contract-validator';
import * as path from 'path';
import * as authFixtures from '../fixtures/auth.fixtures';
import * as userFixtures from '../fixtures/user.fixtures';
import * as invoiceFixtures from '../fixtures/invoice.fixtures';
import * as bidFixtures from '../fixtures/bid.fixtures';
import * as systemFixtures from '../fixtures/system.fixtures';

describe('ContractValidator', () => {
  let validator: ContractValidator;

  beforeAll(() => {
    const specPath = path.join(process.cwd(), 'openapi.yaml');
    validator = new ContractValidator(specPath);
  });

  describe('Authentication endpoints', () => {
    describe('POST /auth/login', () => {
      it('should validate successful login response', () => {
        const result = validator.validateResponse(
          'POST',
          '/auth/login',
          200,
          authFixtures.validLoginResponse
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should validate error response for invalid credentials', () => {
        const result = validator.validateResponse(
          'POST',
          '/auth/login',
          401,
          authFixtures.invalidCredentialsError
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should fail when required field is missing', () => {
        const invalidResponse = {
          token: 'some-token',
          // Missing user field
        };

        const result = validator.validateResponse(
          'POST',
          '/auth/login',
          200,
          invalidResponse
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'user',
            message: 'Required property missing',
          })
        );
      });

      it('should fail when field has wrong type', () => {
        const invalidResponse = {
          ...authFixtures.validLoginResponse,
          token: 12345, // Should be string
        };

        const result = validator.validateResponse(
          'POST',
          '/auth/login',
          200,
          invalidResponse
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'token',
            message: 'Type mismatch',
          })
        );
      });

      it('should fail when enum value is invalid', () => {
        const invalidResponse = {
          ...authFixtures.validLoginResponse,
          user: {
            ...authFixtures.validLoginResponse.user,
            role: 'superadmin', // Not in enum
          },
        };

        const result = validator.validateResponse(
          'POST',
          '/auth/login',
          200,
          invalidResponse
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'user.role',
            message: 'Value not in enum',
          })
        );
      });

      it('should fail when UUID format is invalid', () => {
        const invalidResponse = {
          ...authFixtures.validLoginResponse,
          user: {
            ...authFixtures.validLoginResponse.user,
            id: 'not-a-uuid',
          },
        };

        const result = validator.validateResponse(
          'POST',
          '/auth/login',
          200,
          invalidResponse
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'user.id',
            message: 'Invalid format',
          })
        );
      });

      it('should fail when email format is invalid', () => {
        const invalidResponse = {
          ...authFixtures.validLoginResponse,
          user: {
            ...authFixtures.validLoginResponse.user,
            email: 'not-an-email',
          },
        };

        const result = validator.validateResponse(
          'POST',
          '/auth/login',
          200,
          invalidResponse
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'user.email',
            message: 'Invalid format',
          })
        );
      });
    });

    describe('POST /auth/register', () => {
      it('should validate successful registration response', () => {
        const result = validator.validateResponse(
          'POST',
          '/auth/register',
          201,
          authFixtures.validRegisterResponse
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should validate validation error response', () => {
        const result = validator.validateResponse(
          'POST',
          '/auth/register',
          400,
          authFixtures.validationError
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });
    });
  });

  describe('User endpoints', () => {
    describe('GET /users/profile', () => {
      it('should validate user profile response', () => {
        const result = validator.validateResponse(
          'GET',
          '/users/profile',
          200,
          userFixtures.validUserProfile
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should validate business user profile', () => {
        const result = validator.validateResponse(
          'GET',
          '/users/profile',
          200,
          userFixtures.businessUserProfile
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should validate admin user profile', () => {
        const result = validator.validateResponse(
          'GET',
          '/users/profile',
          200,
          userFixtures.adminUserProfile
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should validate unauthorized error', () => {
        const result = validator.validateResponse(
          'GET',
          '/users/profile',
          401,
          userFixtures.unauthorizedError
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });
    });
  });

  describe('Invoice endpoints', () => {
    describe('GET /invoices', () => {
      it('should validate invoice list response', () => {
        const result = validator.validateResponse(
          'GET',
          '/invoices',
          200,
          invoiceFixtures.validInvoiceListResponse
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should validate empty invoice list', () => {
        const result = validator.validateResponse(
          'GET',
          '/invoices',
          200,
          invoiceFixtures.emptyInvoiceListResponse
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should fail when data is not an array', () => {
        const invalidResponse = {
          data: 'not-an-array',
          total: 0,
        };

        const result = validator.validateResponse(
          'GET',
          '/invoices',
          200,
          invalidResponse
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'data',
            message: 'Expected array',
          })
        );
      });
    });

    describe('POST /invoices', () => {
      it('should validate created invoice response', () => {
        const result = validator.validateResponse(
          'POST',
          '/invoices',
          201,
          invoiceFixtures.validInvoice
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should validate validation error', () => {
        const result = validator.validateResponse(
          'POST',
          '/invoices',
          400,
          invoiceFixtures.invalidInvoiceError
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should fail when amount pattern is invalid', () => {
        const invalidInvoice = {
          ...invoiceFixtures.validInvoice,
          amount: 'invalid-amount',
        };

        const result = validator.validateResponse(
          'POST',
          '/invoices',
          201,
          invalidInvoice
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'amount',
            message: 'Does not match pattern',
          })
        );
      });

      it('should fail when currency is not in enum', () => {
        const invalidInvoice = {
          ...invoiceFixtures.validInvoice,
          currency: 'BTC',
        };

        const result = validator.validateResponse(
          'POST',
          '/invoices',
          201,
          invalidInvoice
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'currency',
            message: 'Value not in enum',
          })
        );
      });

      it('should fail when status is not in enum', () => {
        const invalidInvoice = {
          ...invoiceFixtures.validInvoice,
          status: 'invalid-status',
        };

        const result = validator.validateResponse(
          'POST',
          '/invoices',
          201,
          invalidInvoice
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'status',
            message: 'Value not in enum',
          })
        );
      });
    });

    describe('GET /invoices/{invoiceId}', () => {
      it('should validate invoice detail response', () => {
        const result = validator.validateResponse(
          'GET',
          '/invoices/aa0e8400-e29b-41d4-a716-446655440010',
          200,
          invoiceFixtures.validInvoice
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should validate not found error', () => {
        const result = validator.validateResponse(
          'GET',
          '/invoices/00000000-0000-0000-0000-000000000000',
          404,
          invoiceFixtures.invoiceNotFoundError
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });
    });
  });

  describe('Bid endpoints', () => {
    describe('POST /bids', () => {
      it('should validate created bid response', () => {
        const result = validator.validateResponse(
          'POST',
          '/bids',
          201,
          bidFixtures.validBid
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should validate accepted bid', () => {
        const result = validator.validateResponse(
          'POST',
          '/bids',
          201,
          bidFixtures.acceptedBid
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should validate validation error', () => {
        const result = validator.validateResponse(
          'POST',
          '/bids',
          400,
          bidFixtures.invalidBidError
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should fail when interest rate is out of range', () => {
        const invalidBid = {
          ...bidFixtures.validBid,
          interestRate: 150,
        };

        const result = validator.validateResponse(
          'POST',
          '/bids',
          201,
          invalidBid
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'interestRate',
            message: 'Number too large',
          })
        );
      });

      it('should fail when interest rate is negative', () => {
        const invalidBid = {
          ...bidFixtures.validBid,
          interestRate: -5,
        };

        const result = validator.validateResponse(
          'POST',
          '/bids',
          201,
          invalidBid
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'interestRate',
            message: 'Number too small',
          })
        );
      });
    });
  });

  describe('System endpoints', () => {
    describe('GET /health', () => {
      it('should validate health check response', () => {
        const result = validator.validateResponse(
          'GET',
          '/health',
          200,
          systemFixtures.healthyResponse
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should validate minimal health response', () => {
        const result = validator.validateResponse(
          'GET',
          '/health',
          200,
          systemFixtures.healthyResponseMinimal
        );

        expect(result.valid).toBe(true);
        expect(result.errors).toHaveLength(0);
      });

      it('should fail when status is not in enum', () => {
        const invalidResponse = {
          status: 'unhealthy',
          timestamp: '2024-01-25T15:30:00Z',
        };

        const result = validator.validateResponse(
          'GET',
          '/health',
          200,
          invalidResponse
        );

        expect(result.valid).toBe(false);
        expect(result.errors).toContainEqual(
          expect.objectContaining({
            path: 'status',
            message: 'Value not in enum',
          })
        );
      });
    });
  });

  describe('Edge cases', () => {
    it('should handle non-existent path', () => {
      const result = validator.validateResponse(
        'GET',
        '/non-existent',
        200,
        {}
      );

      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          message: expect.stringContaining('Path not found'),
        })
      );
    });

    it('should handle non-existent method', () => {
      const result = validator.validateResponse(
        'DELETE',
        '/health',
        200,
        {}
      );

      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          message: expect.stringContaining('Method DELETE not defined'),
        })
      );
    });

    it('should handle non-existent status code', () => {
      const result = validator.validateResponse(
        'GET',
        '/health',
        500,
        {}
      );

      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          message: expect.stringContaining('Response 500 not defined'),
        })
      );
    });

    it('should handle date-time format validation', () => {
      const invalidInvoice = {
        ...invoiceFixtures.validInvoice,
        dueDate: 'not-a-date',
      };

      const result = validator.validateResponse(
        'POST',
        '/invoices',
        201,
        invalidInvoice
      );

      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          path: 'dueDate',
          message: 'Invalid format',
        })
      );
    });
  });
});
