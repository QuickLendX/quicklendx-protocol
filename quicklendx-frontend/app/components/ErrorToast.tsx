'use client';

import { useEffect } from 'react';
import toast, { Toast } from 'react-hot-toast';
import { AppError, ErrorCategory, ErrorSeverity } from '../lib/errors';

interface ErrorToastProps {
  error: AppError;
  toastId?: string;
}

export const ErrorToast: React.FC<ErrorToastProps> = ({ error, toastId }) => {
  const getToastStyle = () => {
    switch (error.severity) {
      case ErrorSeverity.CRITICAL:
        return 'bg-red-600 text-white border-red-700';
      case ErrorSeverity.HIGH:
        return 'bg-orange-500 text-white border-orange-600';
      case ErrorSeverity.MEDIUM:
        return 'bg-yellow-500 text-black border-yellow-600';
      case ErrorSeverity.LOW:
        return 'bg-blue-500 text-white border-blue-600';
      default:
        return 'bg-gray-500 text-white border-gray-600';
    }
  };

  const getIcon = () => {
    switch (error.category) {
      case ErrorCategory.NETWORK:
        return 'Network Error';
      case ErrorCategory.AUTHENTICATION:
        return 'Authentication Error';
      case ErrorCategory.VALIDATION:
        return 'Validation Error';
      case ErrorCategory.SYSTEM:
        return 'System Error';
      case ErrorCategory.BUSINESS_LOGIC:
        return 'Business Logic Error';
      default:
        return 'Unknown Error';
    }
  };

  const getActionButton = () => {
    if (error.retryable && error.retryCount < 3) {
      return (
        <button
          onClick={() => {
            toast.dismiss(toastId);
            // Trigger retry logic here
            toast.success('Retrying operation...');
          }}
          className="ml-2 px-2 py-1 text-xs bg-white bg-opacity-20 rounded hover:bg-opacity-30 transition-all"
        >
          Retry
        </button>
      );
    }
    return null;
  };

  return (
    <div className={`flex items-center justify-between p-3 rounded-lg border ${getToastStyle()}`}>
      <div className="flex items-center space-x-2">
        <span className="text-lg">{getIcon()}</span>
        <div>
          <div className="font-medium text-sm">
            {error.category.replace('_', ' ').toUpperCase()}
          </div>
          <div className="text-xs opacity-90">
            {error.message}
          </div>
        </div>
      </div>
      {getActionButton()}
    </div>
  );
};

// Toast notification manager
export class ErrorToastManager {
  private static instance: ErrorToastManager;
  private activeToasts: Set<string> = new Set();

  static getInstance(): ErrorToastManager {
    if (!ErrorToastManager.instance) {
      ErrorToastManager.instance = new ErrorToastManager();
    }
    return ErrorToastManager.instance;
  }

  showError(error: AppError, options?: {
    duration?: number;
    dismissible?: boolean;
    position?: 'top-right' | 'top-center' | 'top-left' | 'bottom-right' | 'bottom-center' | 'bottom-left';
  }): string {
    const toastId = `error-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    
    const defaultDuration = error.severity === ErrorSeverity.CRITICAL ? 8000 : 5000;
    const duration = options?.duration ?? defaultDuration;
    const dismissible = options?.dismissible ?? true;
    const position = options?.position ?? 'top-right';

    toast.custom(
      (t) => (
        <ErrorToast 
          error={error} 
          toastId={t.id}
        />
      ),
      {
        id: toastId,
        duration: dismissible ? duration : Infinity,
        position,
        style: {
          background: 'transparent',
          padding: 0,
          margin: 0,
          boxShadow: 'none',
        },
      }
    );

    this.activeToasts.add(toastId);
    
    // Auto-remove from tracking when toast is dismissed
    setTimeout(() => {
      this.activeToasts.delete(toastId);
    }, duration + 1000);

    return toastId;
  }

  showSuccess(message: string, options?: {
    duration?: number;
    position?: 'top-right' | 'top-center' | 'top-left' | 'bottom-right' | 'bottom-center' | 'bottom-left';
  }): string {
    return toast.success(message, {
      duration: options?.duration ?? 3000,
      position: options?.position ?? 'top-right',
      style: {
        background: '#10B981',
        color: '#fff',
        fontWeight: '500',
      },
    });
  }

  showWarning(message: string, options?: {
    duration?: number;
    position?: 'top-right' | 'top-center' | 'top-left' | 'bottom-right' | 'bottom-center' | 'bottom-left';
  }): string {
    return toast(message, {
      duration: options?.duration ?? 4000,
      position: options?.position ?? 'top-right',
      icon: '⚠️',
      style: {
        background: '#F59E0B',
        color: '#fff',
        fontWeight: '500',
      },
    });
  }

  showInfo(message: string, options?: {
    duration?: number;
    position?: 'top-right' | 'top-center' | 'top-left' | 'bottom-right' | 'bottom-center' | 'bottom-left';
  }): string {
    return toast(message, {
      duration: options?.duration ?? 3000,
      position: options?.position ?? 'top-right',
      icon: 'ℹ️',
      style: {
        background: '#3B82F6',
        color: '#fff',
        fontWeight: '500',
      },
    });
  }

  dismissAll(): void {
    toast.dismiss();
    this.activeToasts.clear();
  }

  dismissToast(toastId: string): void {
    toast.dismiss(toastId);
    this.activeToasts.delete(toastId);
  }

  getActiveToastsCount(): number {
    return this.activeToasts.size;
  }
}

// Hook for using error toasts
export const useErrorToast = () => {
  const toastManager = ErrorToastManager.getInstance();

  return {
    showError: (error: AppError, options?: Parameters<typeof toastManager.showError>[1]) => 
      toastManager.showError(error, options),
    showSuccess: (message: string, options?: Parameters<typeof toastManager.showSuccess>[1]) => 
      toastManager.showSuccess(message, options),
    showWarning: (message: string, options?: Parameters<typeof toastManager.showWarning>[1]) => 
      toastManager.showWarning(message, options),
    showInfo: (message: string, options?: Parameters<typeof toastManager.showInfo>[1]) => 
      toastManager.showInfo(message, options),
    dismissAll: () => toastManager.dismissAll(),
    dismissToast: (toastId: string) => toastManager.dismissToast(toastId),
  };
};

// Global error toast handler
export const handleErrorWithToast = (error: AppError, context?: string): void => {
  const toastManager = ErrorToastManager.getInstance();
  
  // Add context to error message if provided
  const message = context ? `${context}: ${error.message}` : error.message;
  const contextualError = new AppError(
    message,
    error.category,
    error.severity,
    error.code,
    error.context,
    error.retryable,
    error.retryCount
  );

  toastManager.showError(contextualError);
}; 