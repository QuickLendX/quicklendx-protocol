/**
 * Contract Testing Harness
 * 
 * Provides utilities for running contract tests against API responses.
 * Integrates with the ContractValidator to ensure API compliance.
 */

import { ContractValidator, type ValidationResult } from './contract-validator';
import * as path from 'path';

export interface ContractTestOptions {
  specPath?: string;
  failFast?: boolean;
  verbose?: boolean;
}

export interface ContractTestResult {
  passed: boolean;
  endpoint: string;
  method: string;
  statusCode: number;
  validation: ValidationResult;
  timestamp: string;
}

/**
 * Contract testing harness for validating API responses
 */
export class ContractTestHarness {
  private validator: ContractValidator;
  private options: Required<ContractTestOptions>;
  private results: ContractTestResult[] = [];

  constructor(options: ContractTestOptions = {}) {
    const defaultSpecPath = path.join(process.cwd(), 'openapi.yaml');
    
    this.options = {
      specPath: options.specPath || defaultSpecPath,
      failFast: options.failFast ?? true,
      verbose: options.verbose ?? false,
    };

    this.validator = new ContractValidator(this.options.specPath);
  }

  /**
   * Tests an API response against the contract
   */
  testResponse(
    method: string,
    endpoint: string,
    statusCode: number,
    responseBody: unknown,
    contentType: string = 'application/json'
  ): ContractTestResult {
    const validation = this.validator.validateResponse(
      method,
      endpoint,
      statusCode,
      responseBody,
      contentType
    );

    const result: ContractTestResult = {
      passed: validation.valid,
      endpoint,
      method: method.toUpperCase(),
      statusCode,
      validation,
      timestamp: new Date().toISOString(),
    };

    this.results.push(result);

    if (this.options.verbose) {
      this.logResult(result);
    }

    if (this.options.failFast && !result.passed) {
      this.throwContractError(result);
    }

    return result;
  }

  /**
   * Gets all test results
   */
  getResults(): ContractTestResult[] {
    return [...this.results];
  }

  /**
   * Gets summary of test results
   */
  getSummary(): {
    total: number;
    passed: number;
    failed: number;
    passRate: number;
  } {
    const total = this.results.length;
    const passed = this.results.filter((r) => r.passed).length;
    const failed = total - passed;
    const passRate = total > 0 ? (passed / total) * 100 : 0;

    return { total, passed, failed, passRate };
  }

  /**
   * Resets test results
   */
  reset(): void {
    this.results = [];
  }

  /**
   * Logs a test result
   */
  private logResult(result: ContractTestResult): void {
    const status = result.passed ? '✓' : '✗';
    const color = result.passed ? '\x1b[32m' : '\x1b[31m';
    const reset = '\x1b[0m';

    console.log(
      `${color}${status}${reset} ${result.method} ${result.endpoint} (${result.statusCode})`
    );

    if (!result.passed && result.validation.errors.length > 0) {
      console.log('  Validation errors:');
      for (const error of result.validation.errors) {
        console.log(`    - ${error.path}: ${error.message}`);
        if (error.expected !== undefined) {
          console.log(`      Expected: ${JSON.stringify(error.expected)}`);
        }
        if (error.actual !== undefined) {
          console.log(`      Actual: ${JSON.stringify(error.actual)}`);
        }
      }
    }
  }

  /**
   * Throws a contract violation error
   */
  private throwContractError(result: ContractTestResult): never {
    const errors = result.validation.errors
      .map((e) => {
        let msg = `  - ${e.path}: ${e.message}`;
        if (e.expected !== undefined) {
          msg += `\n    Expected: ${JSON.stringify(e.expected)}`;
        }
        if (e.actual !== undefined) {
          msg += `\n    Actual: ${JSON.stringify(e.actual)}`;
        }
        return msg;
      })
      .join('\n');

    const message = [
      '',
      '❌ CONTRACT VIOLATION DETECTED',
      '',
      `Endpoint: ${result.method} ${result.endpoint}`,
      `Status Code: ${result.statusCode}`,
      '',
      'Validation Errors:',
      errors,
      '',
      'The API response does not match the OpenAPI specification.',
      'This is a breaking change that must be fixed before deployment.',
      '',
    ].join('\n');

    throw new ContractViolationError(message, result);
  }
}

/**
 * Contract violation error
 */
export class ContractViolationError extends Error {
  constructor(
    message: string,
    public readonly result: ContractTestResult
  ) {
    super(message);
    this.name = 'ContractViolationError';
  }
}

/**
 * Creates a contract test harness instance
 */
export function createContractHarness(options?: ContractTestOptions): ContractTestHarness {
  return new ContractTestHarness(options);
}
