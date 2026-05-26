/**
 * Testing Utilities
 * 
 * Exports contract testing tools and fixtures for API validation.
 */

export {
  ContractValidator,
  type ValidationResult,
  type ValidationError,
} from './contract-validator';

export {
  ContractTestHarness,
  ContractViolationError,
  createContractHarness,
  type ContractTestOptions,
  type ContractTestResult,
} from './contract-harness';

// Re-export fixtures for convenience
export * as authFixtures from './fixtures/auth.fixtures';
export * as userFixtures from './fixtures/user.fixtures';
export * as invoiceFixtures from './fixtures/invoice.fixtures';
export * as bidFixtures from './fixtures/bid.fixtures';
export * as systemFixtures from './fixtures/system.fixtures';
