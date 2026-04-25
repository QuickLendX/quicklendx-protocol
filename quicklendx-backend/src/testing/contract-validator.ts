/**
 * Contract Testing Validator
 * 
 * Validates API responses against OpenAPI specification to prevent breaking changes.
 * Ensures strict adherence to the API contract.
 */

import * as fs from 'fs';
import * as path from 'path';
import { parse as parseYaml } from 'yaml';
import type { OpenAPIV3 } from 'openapi-types';

export interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
}

export interface ValidationError {
  path: string;
  message: string;
  expected?: unknown;
  actual?: unknown;
}

export class ContractValidator {
  private spec: OpenAPIV3.Document;

  constructor(specPath: string) {
    const specContent = fs.readFileSync(specPath, 'utf-8');
    this.spec = parseYaml(specContent) as OpenAPIV3.Document;
  }

  /**
   * Validates a response against the OpenAPI specification
   */
  validateResponse(
    method: string,
    path: string,
    statusCode: number,
    responseBody: unknown,
    contentType: string = 'application/json'
  ): ValidationResult {
    const errors: ValidationError[] = [];

    try {
      // Find the path in the spec
      const pathItem = this.findPathItem(path);
      if (!pathItem) {
        errors.push({
          path: path,
          message: `Path not found in OpenAPI spec: ${path}`,
        });
        return { valid: false, errors };
      }

      // Get the operation (GET, POST, etc.)
      const operation = pathItem[method.toLowerCase() as keyof OpenAPIV3.PathItemObject] as OpenAPIV3.OperationObject;
      if (!operation) {
        errors.push({
          path: `${path}.${method}`,
          message: `Method ${method} not defined for path ${path}`,
        });
        return { valid: false, errors };
      }

      // Get the response definition
      const responseSpec = operation.responses?.[statusCode.toString()] as OpenAPIV3.ResponseObject;
      if (!responseSpec) {
        errors.push({
          path: `${path}.${method}.responses.${statusCode}`,
          message: `Response ${statusCode} not defined in spec`,
        });
        return { valid: false, errors };
      }

      // Validate response body against schema
      const mediaType = responseSpec.content?.[contentType] as OpenAPIV3.MediaTypeObject;
      if (!mediaType) {
        errors.push({
          path: `${path}.${method}.responses.${statusCode}.content`,
          message: `Content type ${contentType} not defined in spec`,
        });
        return { valid: false, errors };
      }

      const schema = mediaType.schema as OpenAPIV3.SchemaObject;
      if (schema) {
        this.validateSchema(responseBody, schema, '', errors);
      }

      return {
        valid: errors.length === 0,
        errors,
      };
    } catch (error) {
      errors.push({
        path: path,
        message: `Validation error: ${error instanceof Error ? error.message : String(error)}`,
      });
      return { valid: false, errors };
    }
  }

  /**
   * Finds a path item in the spec, handling path parameters
   */
  private findPathItem(requestPath: string): OpenAPIV3.PathItemObject | null {
    // Try exact match first
    if (this.spec.paths[requestPath]) {
      return this.spec.paths[requestPath] as OpenAPIV3.PathItemObject;
    }

    // Try matching with path parameters
    for (const [specPath, pathItem] of Object.entries(this.spec.paths)) {
      if (this.matchPath(requestPath, specPath)) {
        return pathItem as OpenAPIV3.PathItemObject;
      }
    }

    return null;
  }

  /**
   * Matches a request path against a spec path with parameters
   */
  private matchPath(requestPath: string, specPath: string): boolean {
    const requestParts = requestPath.split('/').filter(Boolean);
    const specParts = specPath.split('/').filter(Boolean);

    if (requestParts.length !== specParts.length) {
      return false;
    }

    return specParts.every((specPart, index) => {
      if (specPart.startsWith('{') && specPart.endsWith('}')) {
        return true; // Path parameter matches any value
      }
      return specPart === requestParts[index];
    });
  }

  /**
   * Validates data against a JSON schema
   */
  private validateSchema(
    data: unknown,
    schema: OpenAPIV3.SchemaObject | OpenAPIV3.ReferenceObject,
    path: string,
    errors: ValidationError[]
  ): void {
    // Resolve $ref if present
    if ('$ref' in schema) {
      const resolvedSchema = this.resolveRef(schema.$ref);
      if (resolvedSchema) {
        this.validateSchema(data, resolvedSchema, path, errors);
      }
      return;
    }

    const schemaObj = schema as OpenAPIV3.SchemaObject;

    // Check type
    if (schemaObj.type) {
      const actualType = this.getType(data);
      const expectedType = schemaObj.type;

      if (actualType !== expectedType) {
        errors.push({
          path: path || 'root',
          message: `Type mismatch`,
          expected: expectedType,
          actual: actualType,
        });
        return;
      }
    }

    // Validate based on type
    if (schemaObj.type === 'object') {
      this.validateObject(data, schemaObj, path, errors);
    } else if (schemaObj.type === 'array') {
      this.validateArray(data, schemaObj, path, errors);
    } else if (schemaObj.type === 'string') {
      this.validateString(data, schemaObj, path, errors);
    } else if (schemaObj.type === 'number' || schemaObj.type === 'integer') {
      this.validateNumber(data, schemaObj, path, errors);
    }

    // Validate enum
    if (schemaObj.enum && !schemaObj.enum.includes(data as never)) {
      errors.push({
        path: path || 'root',
        message: `Value not in enum`,
        expected: schemaObj.enum,
        actual: data,
      });
    }
  }

  /**
   * Validates an object against a schema
   */
  private validateObject(
    data: unknown,
    schema: OpenAPIV3.SchemaObject,
    path: string,
    errors: ValidationError[]
  ): void {
    if (typeof data !== 'object' || data === null || Array.isArray(data)) {
      errors.push({
        path: path || 'root',
        message: 'Expected object',
        expected: 'object',
        actual: this.getType(data),
      });
      return;
    }

    const obj = data as Record<string, unknown>;

    // Check required properties
    if (schema.required) {
      for (const requiredProp of schema.required) {
        if (!(requiredProp in obj)) {
          errors.push({
            path: path ? `${path}.${requiredProp}` : requiredProp,
            message: `Required property missing`,
            expected: requiredProp,
            actual: undefined,
          });
        }
      }
    }

    // Validate properties
    if (schema.properties) {
      for (const [propName, propSchema] of Object.entries(schema.properties)) {
        if (propName in obj) {
          const propPath = path ? `${path}.${propName}` : propName;
          this.validateSchema(obj[propName], propSchema, propPath, errors);
        }
      }
    }

    // Check for additional properties if not allowed
    if (schema.additionalProperties === false && schema.properties) {
      const allowedProps = Object.keys(schema.properties);
      for (const propName of Object.keys(obj)) {
        if (!allowedProps.includes(propName)) {
          errors.push({
            path: path ? `${path}.${propName}` : propName,
            message: `Additional property not allowed`,
            actual: propName,
          });
        }
      }
    }
  }

  /**
   * Validates an array against a schema
   */
  private validateArray(
    data: unknown,
    schema: OpenAPIV3.SchemaObject,
    path: string,
    errors: ValidationError[]
  ): void {
    if (!Array.isArray(data)) {
      errors.push({
        path: path || 'root',
        message: 'Expected array',
        expected: 'array',
        actual: this.getType(data),
      });
      return;
    }

    // Validate array items
    if (schema.items) {
      data.forEach((item, index) => {
        const itemPath = `${path}[${index}]`;
        this.validateSchema(item, schema.items as OpenAPIV3.SchemaObject, itemPath, errors);
      });
    }

    // Validate min/max items
    if (schema.minItems !== undefined && data.length < schema.minItems) {
      errors.push({
        path: path || 'root',
        message: `Array too short`,
        expected: `minItems: ${schema.minItems}`,
        actual: data.length,
      });
    }

    if (schema.maxItems !== undefined && data.length > schema.maxItems) {
      errors.push({
        path: path || 'root',
        message: `Array too long`,
        expected: `maxItems: ${schema.maxItems}`,
        actual: data.length,
      });
    }
  }

  /**
   * Validates a string against a schema
   */
  private validateString(
    data: unknown,
    schema: OpenAPIV3.SchemaObject,
    path: string,
    errors: ValidationError[]
  ): void {
    if (typeof data !== 'string') {
      return; // Type error already caught
    }

    // Validate format
    if (schema.format) {
      if (!this.validateFormat(data, schema.format)) {
        errors.push({
          path: path || 'root',
          message: `Invalid format`,
          expected: schema.format,
          actual: data,
        });
      }
    }

    // Validate pattern
    if (schema.pattern) {
      const regex = new RegExp(schema.pattern);
      if (!regex.test(data)) {
        errors.push({
          path: path || 'root',
          message: `Does not match pattern`,
          expected: schema.pattern,
          actual: data,
        });
      }
    }

    // Validate length
    if (schema.minLength !== undefined && data.length < schema.minLength) {
      errors.push({
        path: path || 'root',
        message: `String too short`,
        expected: `minLength: ${schema.minLength}`,
        actual: data.length,
      });
    }

    if (schema.maxLength !== undefined && data.length > schema.maxLength) {
      errors.push({
        path: path || 'root',
        message: `String too long`,
        expected: `maxLength: ${schema.maxLength}`,
        actual: data.length,
      });
    }
  }

  /**
   * Validates a number against a schema
   */
  private validateNumber(
    data: unknown,
    schema: OpenAPIV3.SchemaObject,
    path: string,
    errors: ValidationError[]
  ): void {
    if (typeof data !== 'number') {
      return; // Type error already caught
    }

    // Validate integer
    if (schema.type === 'integer' && !Number.isInteger(data)) {
      errors.push({
        path: path || 'root',
        message: `Expected integer`,
        expected: 'integer',
        actual: data,
      });
    }

    // Validate min/max
    if (schema.minimum !== undefined && data < schema.minimum) {
      errors.push({
        path: path || 'root',
        message: `Number too small`,
        expected: `minimum: ${schema.minimum}`,
        actual: data,
      });
    }

    if (schema.maximum !== undefined && data > schema.maximum) {
      errors.push({
        path: path || 'root',
        message: `Number too large`,
        expected: `maximum: ${schema.maximum}`,
        actual: data,
      });
    }
  }

  /**
   * Validates string format
   */
  private validateFormat(value: string, format: string): boolean {
    switch (format) {
      case 'email':
        return /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(value);
      case 'uuid':
        return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value);
      case 'date-time':
        return !isNaN(Date.parse(value));
      case 'uri':
      case 'url':
        try {
          new URL(value);
          return true;
        } catch {
          return false;
        }
      default:
        return true; // Unknown formats pass
    }
  }

  /**
   * Gets the JSON schema type of a value
   */
  private getType(value: unknown): string {
    if (value === null) return 'null';
    if (Array.isArray(value)) return 'array';
    return typeof value;
  }

  /**
   * Resolves a $ref reference
   */
  private resolveRef(ref: string): OpenAPIV3.SchemaObject | null {
    // Only handle #/components/schemas/ refs for now
    if (!ref.startsWith('#/components/schemas/')) {
      return null;
    }

    const schemaName = ref.replace('#/components/schemas/', '');
    const schemas = this.spec.components?.schemas;
    
    if (!schemas || !schemas[schemaName]) {
      return null;
    }

    return schemas[schemaName] as OpenAPIV3.SchemaObject;
  }
}
