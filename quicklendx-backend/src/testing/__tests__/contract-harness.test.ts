import { describe, it, expect, beforeEach, vi } from 'vitest';
import { ContractTestHarness, ContractViolationError, createContractHarness } from '../contract-harness';
import * as path from 'path';
import * as authFixtures from '../fixtures/auth.fixtures';
import * as invoiceFixtures from '../fixtures/invoice.fixtures';

describe('ContractTestHarness', () => {
  let harness: ContractTestHarness;

  beforeEach(() => {
    const specPath = path.join(process.cwd(), 'openapi.yaml');
    harness = new ContractTestHarness({ specPath, failFast: false, verbose: false });
  });

  describe('testResponse', () => {
    it('should record passing test', () => {
      const result = harness.testResponse(
        'POST',
        '/auth/login',
        200,
        authFixtures.validLoginResponse
      );

      expect(result.passed).toBe(true);
      expect(result.endpoint).toBe('/auth/login');
      expect(result.method).toBe('POST');
      expect(result.statusCode).toBe(200);
      expect(result.validation.valid).toBe(true);
      expect(result.timestamp).toBeDefined();
    });

    it('should record failing test', () => {
      const invalidResponse = {
        token: 'some-token',
        // Missing user field
      };

      const result = harness.testResponse(
        'POST',
        '/auth/login',
        200,
        invalidResponse
      );

      expect(result.passed).toBe(false);
      expect(result.validation.valid).toBe(false);
      expect(result.validation.errors.length).toBeGreaterThan(0);
    });

    it('should normalize method to uppercase', () => {
      const result = harness.testResponse(
        'post',
        '/auth/login',
        200,
        authFixtures.validLoginResponse
      );

      expect(result.method).toBe('POST');
    });

    it('should include timestamp in ISO format', () => {
      const result = harness.testResponse(
        'POST',
        '/auth/login',
        200,
        authFixtures.validLoginResponse
      );

      expect(result.timestamp).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/);
    });
  });

  describe('failFast mode', () => {
    it('should throw error immediately on contract violation when failFast is true', () => {
      const failFastHarness = new ContractTestHarness({
        specPath: path.join(process.cwd(), 'openapi.yaml'),
        failFast: true,
      });

      const invalidResponse = {
        token: 'some-token',
        // Missing user field
      };

      expect(() => {
        failFastHarness.testResponse(
          'POST',
          '/auth/login',
          200,
          invalidResponse
        );
      }).toThrow(ContractViolationError);
    });

    it('should not throw when failFast is false', () => {
      const invalidResponse = {
        token: 'some-token',
        // Missing user field
      };

      expect(() => {
        harness.testResponse(
          'POST',
          '/auth/login',
          200,
          invalidResponse
        );
      }).not.toThrow();
    });

    it('should include detailed error information in thrown error', () => {
      const failFastHarness = new ContractTestHarness({
        specPath: path.join(process.cwd(), 'openapi.yaml'),
        failFast: true,
      });

      const invalidResponse = {
        token: 'some-token',
      };

      try {
        failFastHarness.testResponse(
          'POST',
          '/auth/login',
          200,
          invalidResponse
        );
        expect.fail('Should have thrown ContractViolationError');
      } catch (error) {
        expect(error).toBeInstanceOf(ContractViolationError);
        const contractError = error as ContractViolationError;
        expect(contractError.message).toContain('CONTRACT VIOLATION');
        expect(contractError.message).toContain('POST /auth/login');
        expect(contractError.message).toContain('user');
        expect(contractError.result).toBeDefined();
        expect(contractError.result.passed).toBe(false);
      }
    });
  });

  describe('verbose mode', () => {
    it('should log results when verbose is true', () => {
      const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

      const verboseHarness = new ContractTestHarness({
        specPath: path.join(process.cwd(), 'openapi.yaml'),
        verbose: true,
      });

      verboseHarness.testResponse(
        'POST',
        '/auth/login',
        200,
        authFixtures.validLoginResponse
      );

      expect(consoleSpy).toHaveBeenCalled();
      consoleSpy.mockRestore();
    });

    it('should not log when verbose is false', () => {
      const consoleSpy = vi.spyOn(console, 'log').mockImplementation(() => {});

      harness.testResponse(
        'POST',
        '/auth/login',
        200,
        authFixtures.validLoginResponse
      );

      expect(consoleSpy).not.toHaveBeenCalled();
      consoleSpy.mockRestore();
    });
  });

  describe('getResults', () => {
    it('should return all test results', () => {
      harness.testResponse('POST', '/auth/login', 200, authFixtures.validLoginResponse);
      harness.testResponse('GET', '/users/profile', 200, { id: '123', email: 'test@example.com', role: 'investor', createdAt: '2024-01-01T00:00:00Z' });

      const results = harness.getResults();

      expect(results).toHaveLength(2);
      expect(results[0].endpoint).toBe('/auth/login');
      expect(results[1].endpoint).toBe('/users/profile');
    });

    it('should return empty array when no tests run', () => {
      const results = harness.getResults();
      expect(results).toHaveLength(0);
    });

    it('should return a copy of results array', () => {
      harness.testResponse('POST', '/auth/login', 200, authFixtures.validLoginResponse);

      const results1 = harness.getResults();
      const results2 = harness.getResults();

      expect(results1).not.toBe(results2);
      expect(results1).toEqual(results2);
    });
  });

  describe('getSummary', () => {
    it('should return correct summary for all passing tests', () => {
      harness.testResponse('POST', '/auth/login', 200, authFixtures.validLoginResponse);
      harness.testResponse('POST', '/auth/register', 201, authFixtures.validRegisterResponse);

      const summary = harness.getSummary();

      expect(summary.total).toBe(2);
      expect(summary.passed).toBe(2);
      expect(summary.failed).toBe(0);
      expect(summary.passRate).toBe(100);
    });

    it('should return correct summary for mixed results', () => {
      harness.testResponse('POST', '/auth/login', 200, authFixtures.validLoginResponse);
      harness.testResponse('POST', '/auth/login', 200, { token: 'invalid' }); // Missing user

      const summary = harness.getSummary();

      expect(summary.total).toBe(2);
      expect(summary.passed).toBe(1);
      expect(summary.failed).toBe(1);
      expect(summary.passRate).toBe(50);
    });

    it('should return correct summary for all failing tests', () => {
      harness.testResponse('POST', '/auth/login', 200, { token: 'invalid' });
      harness.testResponse('POST', '/auth/login', 200, { user: {} });

      const summary = harness.getSummary();

      expect(summary.total).toBe(2);
      expect(summary.passed).toBe(0);
      expect(summary.failed).toBe(2);
      expect(summary.passRate).toBe(0);
    });

    it('should return zero values when no tests run', () => {
      const summary = harness.getSummary();

      expect(summary.total).toBe(0);
      expect(summary.passed).toBe(0);
      expect(summary.failed).toBe(0);
      expect(summary.passRate).toBe(0);
    });
  });

  describe('reset', () => {
    it('should clear all test results', () => {
      harness.testResponse('POST', '/auth/login', 200, authFixtures.validLoginResponse);
      harness.testResponse('POST', '/auth/register', 201, authFixtures.validRegisterResponse);

      expect(harness.getResults()).toHaveLength(2);

      harness.reset();

      expect(harness.getResults()).toHaveLength(0);
    });

    it('should reset summary statistics', () => {
      harness.testResponse('POST', '/auth/login', 200, authFixtures.validLoginResponse);

      let summary = harness.getSummary();
      expect(summary.total).toBe(1);

      harness.reset();

      summary = harness.getSummary();
      expect(summary.total).toBe(0);
      expect(summary.passed).toBe(0);
      expect(summary.failed).toBe(0);
    });
  });

  describe('createContractHarness', () => {
    it('should create a new harness instance', () => {
      const newHarness = createContractHarness({
        specPath: path.join(process.cwd(), 'openapi.yaml'),
      });

      expect(newHarness).toBeInstanceOf(ContractTestHarness);
    });

    it('should use default options when not provided', () => {
      const newHarness = createContractHarness();

      expect(newHarness).toBeInstanceOf(ContractTestHarness);
    });
  });

  describe('Integration scenarios', () => {
    it('should handle multiple endpoints in sequence', () => {
      // Auth
      harness.testResponse('POST', '/auth/login', 200, authFixtures.validLoginResponse);
      harness.testResponse('POST', '/auth/register', 201, authFixtures.validRegisterResponse);

      // Invoices
      harness.testResponse('GET', '/invoices', 200, invoiceFixtures.validInvoiceListResponse);
      harness.testResponse('POST', '/invoices', 201, invoiceFixtures.validInvoice);

      const results = harness.getResults();
      expect(results).toHaveLength(4);
      expect(results.every((r) => r.passed)).toBe(true);

      const summary = harness.getSummary();
      expect(summary.passRate).toBe(100);
    });

    it('should track failures across multiple endpoints', () => {
      harness.testResponse('POST', '/auth/login', 200, authFixtures.validLoginResponse);
      harness.testResponse('POST', '/auth/login', 200, { invalid: 'response' });
      harness.testResponse('GET', '/invoices', 200, invoiceFixtures.validInvoiceListResponse);
      harness.testResponse('GET', '/invoices', 200, { data: 'not-an-array' });

      const summary = harness.getSummary();
      expect(summary.total).toBe(4);
      expect(summary.passed).toBe(2);
      expect(summary.failed).toBe(2);
      expect(summary.passRate).toBe(50);
    });
  });

  describe('Security - no secret leakage', () => {
    it('should not expose sensitive data in error messages', () => {
      const failFastHarness = new ContractTestHarness({
        specPath: path.join(process.cwd(), 'openapi.yaml'),
        failFast: true,
      });

      const responseWithSecrets = {
        token: 'super-secret-jwt-token-12345',
        user: {
          id: 'invalid-uuid',
          email: 'test@example.com',
          role: 'investor',
          createdAt: '2024-01-01T00:00:00Z',
        },
      };

      try {
        failFastHarness.testResponse(
          'POST',
          '/auth/login',
          200,
          responseWithSecrets
        );
        expect.fail('Should have thrown');
      } catch (error) {
        const errorMessage = (error as Error).message;
        
        // Error should mention the field but not the actual secret value
        expect(errorMessage).toContain('id');
        expect(errorMessage).toContain('Invalid format');
        
        // Should not contain the actual token value
        expect(errorMessage).not.toContain('super-secret-jwt-token-12345');
      }
    });
  });
});
