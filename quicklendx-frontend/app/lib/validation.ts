import { z } from 'zod';
import { ValidationError, ErrorPrevention } from './errors';

// Common validation schemas
export const emailSchema = z
  .string()
  .email('Please enter a valid email address')
  .min(1, 'Email is required')
  .max(254, 'Email is too long');

export const passwordSchema = z
  .string()
  .min(8, 'Password must be at least 8 characters long')
  .regex(/^(?=.*[a-z])(?=.*[A-Z])(?=.*\d)/, 'Password must contain at least one uppercase letter, one lowercase letter, and one number')
  .max(128, 'Password is too long');

export const usernameSchema = z
  .string()
  .min(3, 'Username must be at least 3 characters long')
  .max(30, 'Username is too long')
  .regex(/^[a-zA-Z0-9_-]+$/, 'Username can only contain letters, numbers, underscores, and hyphens');

export const phoneSchema = z
  .string()
  .regex(/^\+?[\d\s\-\(\)]+$/, 'Please enter a valid phone number')
  .min(10, 'Phone number is too short')
  .max(20, 'Phone number is too long');

export const urlSchema = z
  .string()
  .url('Please enter a valid URL')
  .optional();

export const amountSchema = z
  .number()
  .positive('Amount must be positive')
  .min(0.01, 'Amount must be at least 0.01')
  .max(999999999.99, 'Amount is too large');

export const percentageSchema = z
  .number()
  .min(0, 'Percentage must be at least 0%')
  .max(100, 'Percentage cannot exceed 100%');

// Business-specific schemas
export const invoiceSchema = z.object({
  id: z.string().uuid('Invalid invoice ID'),
  amount: amountSchema,
  currency: z.enum(['USD', 'EUR', 'GBP'], {
    errorMap: () => ({ message: 'Please select a valid currency' })
  }),
  dueDate: z.date({
    required_error: 'Due date is required',
    invalid_type_error: 'Please enter a valid date'
  }).min(new Date(), 'Due date must be in the future'),
  description: z.string()
    .min(1, 'Description is required')
    .max(500, 'Description is too long'),
  status: z.enum(['pending', 'approved', 'rejected', 'paid'], {
    errorMap: () => ({ message: 'Invalid status' })
  }).optional(),
});

export const bidSchema = z.object({
  id: z.string().uuid('Invalid bid ID'),
  invoiceId: z.string().uuid('Invalid invoice ID'),
  amount: amountSchema,
  interestRate: percentageSchema,
  term: z.number()
    .int('Term must be a whole number')
    .min(1, 'Term must be at least 1 day')
    .max(365, 'Term cannot exceed 365 days'),
  description: z.string()
    .max(1000, 'Description is too long')
    .optional(),
});

export const userProfileSchema = z.object({
  firstName: z.string()
    .min(1, 'First name is required')
    .max(50, 'First name is too long')
    .regex(/^[a-zA-Z\s]+$/, 'First name can only contain letters and spaces'),
  lastName: z.string()
    .min(1, 'Last name is required')
    .max(50, 'Last name is too long')
    .regex(/^[a-zA-Z\s]+$/, 'Last name can only contain letters and spaces'),
  email: emailSchema,
  phone: phoneSchema.optional(),
  company: z.string()
    .max(100, 'Company name is too long')
    .optional(),
  website: urlSchema,
  address: z.object({
    street: z.string().max(100, 'Street address is too long').optional(),
    city: z.string().max(50, 'City name is too long').optional(),
    state: z.string().max(50, 'State name is too long').optional(),
    zipCode: z.string().max(10, 'ZIP code is too long').optional(),
    country: z.string().max(50, 'Country name is too long').optional(),
  }).optional(),
});

export const loginSchema = z.object({
  email: emailSchema,
  password: z.string().min(1, 'Password is required'),
  rememberMe: z.boolean().optional(),
});

export const registrationSchema = z.object({
  email: emailSchema,
  password: passwordSchema,
  confirmPassword: z.string().min(1, 'Please confirm your password'),
  firstName: z.string()
    .min(1, 'First name is required')
    .max(50, 'First name is too long'),
  lastName: z.string()
    .min(1, 'Last name is required')
    .max(50, 'Last name is too long'),
  acceptTerms: z.boolean().refine(val => val === true, {
    message: 'You must accept the terms and conditions'
  }),
}).refine((data) => data.password === data.confirmPassword, {
  message: "Passwords don't match",
  path: ["confirmPassword"],
});

export const fileUploadSchema = z.object({
  file: z.instanceof(File, { message: 'Please select a file' }),
  maxSize: z.number().optional().default(10 * 1024 * 1024), // 10MB default
  allowedTypes: z.array(z.string()).optional().default(['.pdf', '.jpg', '.jpeg', '.png', '.doc', '.docx']),
}).refine((data) => {
  if (data.file.size > data.maxSize) {
    return false;
  }
  const fileExtension = '.' + data.file.name.split('.').pop()?.toLowerCase();
  return data.allowedTypes.includes(fileExtension);
}, {
  message: 'File type not allowed or file too large',
  path: ['file'],
});

// Form validation helper
export class FormValidator {
  static validate<T>(schema: z.ZodSchema<T>, data: unknown): T {
    try {
      return schema.parse(data);
    } catch (error) {
      if (error instanceof z.ZodError) {
        const messages = error.errors.map(err => err.message).join(', ');
        throw new ValidationError(messages, {
          action: 'form_validation',
          additionalData: {
            fieldErrors: error.errors.reduce((acc, err) => {
              const path = err.path.join('.');
              acc[path] = err.message;
              return acc;
            }, {} as Record<string, string>),
            input: data,
          }
        });
      }
      throw error;
    }
  }

  static validatePartial<T>(schema: z.ZodObject<any>, data: unknown): any {
    try {
      return schema.partial().parse(data);
    } catch (error) {
      if (error instanceof z.ZodError) {
        const messages = error.errors.map(err => err.message).join(', ');
        throw new ValidationError(messages, {
          action: 'partial_form_validation',
          additionalData: {
            fieldErrors: error.errors.reduce((acc, err) => {
              const path = err.path.join('.');
              acc[path] = err.message;
              return acc;
            }, {} as Record<string, string>),
            input: data,
          }
        });
      }
      throw error;
    }
  }

  static async validateAsync<T>(schema: z.ZodSchema<T>, data: unknown): Promise<T> {
    try {
      return await schema.parseAsync(data);
    } catch (error) {
      if (error instanceof z.ZodError) {
        const messages = error.errors.map(err => err.message).join(', ');
        throw new ValidationError(messages, {
          action: 'async_form_validation',
          additionalData: {
            fieldErrors: error.errors.reduce((acc, err) => {
              const path = err.path.join('.');
              acc[path] = err.message;
              return acc;
            }, {} as Record<string, string>),
            input: data,
          }
        });
      }
      throw error;
    }
  }

  static sanitizeInput(input: string): string {
    return ErrorPrevention.sanitizeInput(input);
  }

  static validateAndSanitize<T>(schema: z.ZodSchema<T>, data: unknown): T {
    // First sanitize string inputs
    const sanitizedData = this.sanitizeObject(data);
    
    // Then validate
    return this.validate(schema, sanitizedData);
  }

  private static sanitizeObject(obj: any): any {
    if (typeof obj === 'string') {
      return this.sanitizeInput(obj);
    }
    
    if (Array.isArray(obj)) {
      return obj.map(item => this.sanitizeObject(item));
    }
    
    if (obj && typeof obj === 'object') {
      const sanitized: any = {};
      for (const [key, value] of Object.entries(obj)) {
        sanitized[key] = this.sanitizeObject(value);
      }
      return sanitized;
    }
    
    return obj;
  }
}

// Real-time validation hook
export const useValidation = <T>(schema: z.ZodSchema<T>) => {
  const validate = (data: unknown): { isValid: boolean; errors: Record<string, string>; data?: T } => {
    try {
      const validatedData = FormValidator.validate(schema, data);
      return { isValid: true, errors: {}, data: validatedData };
    } catch (error) {
      if (error instanceof ValidationError) {
        const fieldErrors = error.context?.additionalData?.fieldErrors || {};
        return { isValid: false, errors: fieldErrors };
      }
      return { isValid: false, errors: { general: 'Validation failed' } };
    }
  };

  const validateField = (fieldName: string, value: unknown): string | null => {
    try {
      if (schema instanceof z.ZodObject) {
        const fieldSchema = schema.shape[fieldName as keyof T];
        if (fieldSchema) {
          fieldSchema.parse(value);
          return null;
        }
      }
      return 'Field not found in schema';
    } catch (error) {
      if (error instanceof z.ZodError) {
        return error.errors[0]?.message || 'Invalid field';
      }
      return 'Validation error';
    }
  };

  return { validate, validateField };
};

// Export commonly used schemas
export const schemas = {
  email: emailSchema,
  password: passwordSchema,
  username: usernameSchema,
  phone: phoneSchema,
  url: urlSchema,
  amount: amountSchema,
  percentage: percentageSchema,
  invoice: invoiceSchema,
  bid: bidSchema,
  userProfile: userProfileSchema,
  login: loginSchema,
  registration: registrationSchema,
  fileUpload: fileUploadSchema,
}; 