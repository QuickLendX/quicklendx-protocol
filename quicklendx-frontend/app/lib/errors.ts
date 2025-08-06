// Error categorization and custom error types
export enum ErrorCategory {
  NETWORK = 'network',
  VALIDATION = 'validation',
  AUTHENTICATION = 'authentication',
  AUTHORIZATION = 'authorization',
  BUSINESS_LOGIC = 'business_logic',
  SYSTEM = 'system',
  USER_INPUT = 'user_input',
  EXTERNAL_SERVICE = 'external_service'
}

export enum ErrorSeverity {
  LOW = 'low',
  MEDIUM = 'medium',
  HIGH = 'high',
  CRITICAL = 'critical'
}

export interface ErrorContext {
  userId?: string;
  sessionId?: string;
  timestamp: number;
  userAgent: string;
  url: string;
  component?: string;
  action?: string;
  additionalData?: Record<string, any>;
}

export interface ErrorAnalyticsData {
  category: ErrorCategory;
  severity: ErrorSeverity;
  message: string;
  code?: string;
  context: ErrorContext;
  stack?: string;
  retryCount?: number;
}

// Custom error types
export class AppError extends Error {
  public readonly category: ErrorCategory;
  public readonly severity: ErrorSeverity;
  public readonly code?: string;
  public readonly context: ErrorContext;
  public readonly retryable: boolean;
  public readonly retryCount: number;

  constructor(
    message: string,
    category: ErrorCategory,
    severity: ErrorSeverity = ErrorSeverity.MEDIUM,
    code?: string,
    context?: Partial<ErrorContext>,
    retryable: boolean = false,
    retryCount: number = 0
  ) {
    super(message);
    this.name = 'AppError';
    this.category = category;
    this.severity = severity;
    this.code = code;
    this.context = {
      timestamp: typeof window !== 'undefined' ? Date.now() : 0,
      userAgent: typeof window !== 'undefined' ? window.navigator.userAgent : 'unknown',
      url: typeof window !== 'undefined' ? window.location.href : 'unknown',
      ...context
    };
    this.retryable = retryable;
    this.retryCount = retryCount;
  }
}

export class NetworkError extends AppError {
  constructor(message: string, context?: Partial<ErrorContext>, retryCount: number = 0) {
    super(message, ErrorCategory.NETWORK, ErrorSeverity.HIGH, 'NETWORK_ERROR', context, true, retryCount);
    this.name = 'NetworkError';
  }
}

export class ValidationError extends AppError {
  constructor(message: string, context?: Partial<ErrorContext>) {
    super(message, ErrorCategory.VALIDATION, ErrorSeverity.MEDIUM, 'VALIDATION_ERROR', context, false);
    this.name = 'ValidationError';
  }
}

export class AuthenticationError extends AppError {
  constructor(message: string, context?: Partial<ErrorContext>) {
    super(message, ErrorCategory.AUTHENTICATION, ErrorSeverity.HIGH, 'AUTH_ERROR', context, true);
    this.name = 'AuthenticationError';
  }
}

export class AuthorizationError extends AppError {
  constructor(message: string, context?: Partial<ErrorContext>) {
    super(message, ErrorCategory.AUTHORIZATION, ErrorSeverity.HIGH, 'FORBIDDEN', context, false);
    this.name = 'AuthorizationError';
  }
}

export class BusinessLogicError extends AppError {
  constructor(message: string, context?: Partial<ErrorContext>) {
    super(message, ErrorCategory.BUSINESS_LOGIC, ErrorSeverity.MEDIUM, 'BUSINESS_ERROR', context, false);
    this.name = 'BusinessLogicError';
  }
}

export class SystemError extends AppError {
  constructor(message: string, context?: Partial<ErrorContext>) {
    super(message, ErrorCategory.SYSTEM, ErrorSeverity.CRITICAL, 'SYSTEM_ERROR', context, true);
    this.name = 'SystemError';
  }
}

// Error rate limiting
class ErrorRateLimiter {
  private errorCounts: Map<string, { count: number; resetTime: number }> = new Map();
  private readonly maxErrorsPerMinute = 10;
  private readonly resetInterval = 60000; // 1 minute

  isRateLimited(errorKey: string): boolean {
    const now = Date.now();
    const errorData = this.errorCounts.get(errorKey);

    if (!errorData || now > errorData.resetTime) {
      this.errorCounts.set(errorKey, { count: 1, resetTime: now + this.resetInterval });
      return false;
    }

    if (errorData.count >= this.maxErrorsPerMinute) {
      return true;
    }

    errorData.count++;
    return false;
  }

  getErrorKey(error: AppError): string {
    return `${error.category}_${error.code || 'unknown'}`;
  }
}

export const errorRateLimiter = new ErrorRateLimiter();

// Error analytics tracking
export class ErrorAnalytics {
  private static instance: ErrorAnalytics;
  private errors: ErrorAnalyticsData[] = [];
  private readonly maxErrorsToStore = 1000;

  static getInstance(): ErrorAnalytics {
    if (!ErrorAnalytics.instance) {
      ErrorAnalytics.instance = new ErrorAnalytics();
    }
    return ErrorAnalytics.instance;
  }

  trackError(error: AppError): void {
    if (errorRateLimiter.isRateLimited(errorRateLimiter.getErrorKey(error))) {
      return; // Rate limited, don't track
    }

    const analytics: ErrorAnalyticsData = {
      category: error.category,
      severity: error.severity,
      message: error.message,
      code: error.code,
      context: error.context,
      stack: error.stack,
      retryCount: error.retryCount
    };

    this.errors.push(analytics);

    // Keep only the latest errors
    if (this.errors.length > this.maxErrorsToStore) {
      this.errors = this.errors.slice(-this.maxErrorsToStore);
    }

    // Send to external analytics service (e.g., Sentry)
    this.sendToAnalyticsService(analytics);
  }

  private sendToAnalyticsService(analytics: ErrorAnalyticsData): void {
    // Console logging for development and debugging
    if (process.env.NODE_ENV === 'development') {
      console.group(`Error: ${analytics.category.toUpperCase()}`);
      console.error('Message:', analytics.message);
      console.error('Code:', analytics.code);
      console.error('Severity:', analytics.severity);
      console.error('Context:', analytics.context);
      console.error('Stack:', analytics.stack);
      console.groupEnd();
    }

    // Send to Google Analytics if available (client-side only)
    if (typeof window !== 'undefined' && window.gtag) {
      window.gtag('event', 'error', {
        error_category: analytics.category,
        error_severity: analytics.severity,
        error_code: analytics.code,
        error_message: analytics.message
      });
    }

    // In production, you could send to your own error tracking service
    // Example: sendToCustomErrorService(analytics);
  }

  getErrorStats(): {
    total: number;
    byCategory: Record<ErrorCategory, number>;
    bySeverity: Record<ErrorSeverity, number>;
    recentErrors: ErrorAnalyticsData[];
  } {
    const byCategory = Object.values(ErrorCategory).reduce((acc, category) => {
      acc[category] = this.errors.filter(e => e.category === category).length;
      return acc;
    }, {} as Record<ErrorCategory, number>);

    const bySeverity = Object.values(ErrorSeverity).reduce((acc, severity) => {
      acc[severity] = this.errors.filter(e => e.severity === severity).length;
      return acc;
    }, {} as Record<ErrorSeverity, number>);

    return {
      total: this.errors.length,
      byCategory,
      bySeverity,
      recentErrors: this.errors.slice(-10) // Last 10 errors
    };
  }

  clearErrors(): void {
    this.errors = [];
  }
}

// Error recovery mechanisms
export class ErrorRecovery {
  static async retryOperation<T>(
    operation: () => Promise<T>,
    maxRetries: number = 3,
    delay: number = 1000
  ): Promise<T> {
    let lastError: Error;

    for (let attempt = 1; attempt <= maxRetries; attempt++) {
      try {
        return await operation();
      } catch (error) {
        lastError = error as Error;
        
        if (attempt === maxRetries) {
          throw lastError;
        }

        // Exponential backoff
        const waitTime = delay * Math.pow(2, attempt - 1);
        await new Promise(resolve => setTimeout(resolve, waitTime));
      }
    }

    throw lastError!;
  }

  static createFallbackValue<T>(error: AppError, fallback: T): T {
    ErrorAnalytics.getInstance().trackError(error);
    return fallback;
  }

  static async handleAsyncError<T>(
    promise: Promise<T>,
    fallback: T,
    context?: Partial<ErrorContext>
  ): Promise<T> {
    try {
      return await promise;
    } catch (error) {
      const appError = error instanceof AppError ? error : new SystemError(
        error instanceof Error ? error.message : 'Unknown error occurred',
        context
      );
      return this.createFallbackValue(appError, fallback);
    }
  }
}

// Error prevention strategies
export class ErrorPrevention {
  static validateInput<T>(input: T, schema: any): T {
    try {
      return schema.parse(input);
    } catch (error) {
      throw new ValidationError(
        `Invalid input: ${error instanceof Error ? error.message : 'Unknown validation error'}`,
        { additionalData: { input } }
      );
    }
  }

  static sanitizeInput(input: string): string {
    // Basic XSS prevention
    return input
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;')
      .replace(/'/g, '&#x27;')
      .replace(/\//g, '&#x2F;');
  }

  static debounce<T extends (...args: any[]) => any>(
    func: T,
    wait: number
  ): (...args: Parameters<T>) => void {
    let timeout: NodeJS.Timeout;
    return (...args: Parameters<T>) => {
      clearTimeout(timeout);
      timeout = setTimeout(() => func(...args), wait);
    };
  }

  static throttle<T extends (...args: any[]) => any>(
    func: T,
    limit: number
  ): (...args: Parameters<T>) => void {
    let inThrottle: boolean;
    return (...args: Parameters<T>) => {
      if (!inThrottle) {
        func(...args);
        inThrottle = true;
        setTimeout(() => inThrottle = false, limit);
      }
    };
  }
}

// Global error handler
export const globalErrorHandler = (error: Error, context?: Partial<ErrorContext>): void => {
  let appError: AppError;

  if (error instanceof AppError) {
    appError = error;
  } else {
    appError = new SystemError(
      error.message || 'An unexpected error occurred',
      context
    );
  }

  ErrorAnalytics.getInstance().trackError(appError);
};

// Declare global gtag for analytics
declare global {
  interface Window {
    gtag?: (...args: any[]) => void;
  }
} 