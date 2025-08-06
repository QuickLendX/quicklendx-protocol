'use client';

import React from 'react';
import { ErrorBoundary as ReactErrorBoundary } from 'react-error-boundary';
import { AppError, ErrorCategory, ErrorSeverity, globalErrorHandler } from '../lib/errors';

interface ErrorFallbackProps {
  error: Error;
  resetErrorBoundary: () => void;
}

const ErrorFallback: React.FC<ErrorFallbackProps> = ({ error, resetErrorBoundary }) => {
  const isAppError = error instanceof AppError;
  const category = isAppError ? error.category : ErrorCategory.SYSTEM;
  const severity = isAppError ? error.severity : ErrorSeverity.CRITICAL;

  const getErrorIcon = () => {
    switch (category) {
      case ErrorCategory.NETWORK:
        return 'Network Error';
      case ErrorCategory.AUTHENTICATION:
        return 'Authentication Error';
      case ErrorCategory.VALIDATION:
        return 'Validation Error';
      case ErrorCategory.SYSTEM:
        return 'System Error';
      default:
        return 'Unknown Error';
    }
  };

  const getErrorMessage = () => {
    if (isAppError) {
      return error.message;
    }

    // Provide user-friendly messages for common errors
    if (error.message.includes('fetch')) {
      return 'Unable to connect to the server. Please check your internet connection and try again.';
    }
    if (error.message.includes('JSON')) {
      return 'There was an issue processing the data. Please refresh the page and try again.';
    }
    if (error.message.includes('timeout')) {
      return 'The request took too long to complete. Please try again.';
    }

    return 'Something went wrong. Please try refreshing the page.';
  };

  const getActionButton = () => {
    switch (category) {
      case ErrorCategory.NETWORK:
        return (
          <button
            onClick={resetErrorBoundary}
            className="bg-blue-500 hover:bg-blue-600 text-white px-4 py-2 rounded-md transition-colors"
          >
            Retry Connection
          </button>
        );
      case ErrorCategory.AUTHENTICATION:
        return (
          <button
            onClick={() => window.location.href = '/login'}
            className="bg-red-500 hover:bg-red-600 text-white px-4 py-2 rounded-md transition-colors"
          >
            Go to Login
          </button>
        );
      default:
        return (
          <button
            onClick={resetErrorBoundary}
            className="bg-gray-500 hover:bg-gray-600 text-white px-4 py-2 rounded-md transition-colors"
          >
            Try Again
          </button>
        );
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50">
      <div className="max-w-md w-full bg-white rounded-lg shadow-lg p-8 text-center">
        <div className="text-6xl mb-4">{getErrorIcon()}</div>
        
        <h1 className="text-2xl font-bold text-gray-900 mb-4">
          {severity === ErrorSeverity.CRITICAL ? 'Critical Error' : 'Something Went Wrong'}
        </h1>
        
        <p className="text-gray-600 mb-6 leading-relaxed">
          {getErrorMessage()}
        </p>

        <div className="space-y-3">
          {getActionButton()}
          
          <button
            onClick={() => window.location.reload()}
            className="block w-full bg-gray-100 hover:bg-gray-200 text-gray-700 px-4 py-2 rounded-md transition-colors"
          >
            Refresh Page
          </button>
        </div>

        {process.env.NODE_ENV === 'development' && (
          <details className="mt-6 text-left">
            <summary className="cursor-pointer text-sm text-gray-500 hover:text-gray-700">
              Technical Details
            </summary>
            <div className="mt-2 p-3 bg-gray-100 rounded text-xs font-mono text-gray-700 overflow-auto">
              <div><strong>Error:</strong> {error.name}</div>
              <div><strong>Message:</strong> {error.message}</div>
              <div><strong>Stack:</strong></div>
              <pre className="whitespace-pre-wrap">{error.stack}</pre>
            </div>
          </details>
        )}
      </div>
    </div>
  );
};

interface ErrorBoundaryProps {
  children: React.ReactNode;
  fallback?: React.ComponentType<ErrorFallbackProps>;
  onError?: (error: Error, errorInfo: React.ErrorInfo) => void;
}

export const ErrorBoundary: React.FC<ErrorBoundaryProps> = ({
  children,
  fallback = ErrorFallback,
  onError
}) => {
  const handleError = (error: Error, errorInfo: React.ErrorInfo) => {
    // Log error with context
    globalErrorHandler(error, {
      component: errorInfo.componentStack?.split('\n')[1]?.trim(),
      additionalData: { errorInfo }
    });

    // Call custom error handler if provided
    if (onError) {
      onError(error, errorInfo);
    }
  };

  return (
    <ReactErrorBoundary
      FallbackComponent={fallback}
      onError={handleError}
      onReset={() => {
        // Clear any error state when resetting
        window.location.reload();
      }}
    >
      {children}
    </ReactErrorBoundary>
  );
}; 