"use client";

import React from "react";
import { ErrorBoundary as ReactErrorBoundary } from "react-error-boundary";
import {
  AppError,
  ErrorCategory,
  ErrorSeverity,
  globalErrorHandler,
} from "../lib/errors";

export interface ErrorFallbackProps {
  error: Error;
  resetErrorBoundary: () => void;
  /**
   * Invoked by the "Reconnect wallet" action. When it is not provided the
   * fallback navigates back to the app root instead.
   */
  onReauthenticate?: () => void;
  /**
   * Whether the secondary "Refresh Page" action is offered. The App Router
   * error pages disable it because recovery there is driven by Next's
   * `reset()`, which re-renders the segment instead of reloading the document.
   */
  showReloadButton?: boolean;
}

// --- Category artwork (inline SVG, no extra deps) -----------------------------
// The artwork is purely decorative: it is `aria-hidden` and the heading below
// is the only text label describing the failure, so the error type is never
// communicated by an icon alone.
type IconProps = { className?: string };

const NetworkErrorIcon: React.FC<IconProps> = ({ className }) => (
  <svg
    className={className}
    width="48"
    height="48"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <circle cx="12" cy="12" r="9" />
    <path d="M5.6 5.6l12.8 12.8" />
  </svg>
);

const AuthenticationErrorIcon: React.FC<IconProps> = ({ className }) => (
  <svg
    className={className}
    width="48"
    height="48"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <path d="M12 3l7 3v6c0 4.4-2.9 7.7-7 9-4.1-1.3-7-4.6-7-9V6z" />
    <path d="M12 9v4" />
    <path d="M12 16.5v.01" />
  </svg>
);

const ValidationErrorIcon: React.FC<IconProps> = ({ className }) => (
  <svg
    className={className}
    width="48"
    height="48"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <path d="M6 3h8l4 4v14H6z" />
    <path d="M14 3v4h4" />
    <path d="M10 13l4 4" />
    <path d="M14 13l-4 4" />
  </svg>
);

const SystemErrorIcon: React.FC<IconProps> = ({ className }) => (
  <svg
    className={className}
    width="48"
    height="48"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <path d="M12 4l9 16H3z" />
    <path d="M12 10v4" />
    <path d="M12 17v.01" />
  </svg>
);

const UnknownErrorIcon: React.FC<IconProps> = ({ className }) => (
  <svg
    className={className}
    width="48"
    height="48"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <circle cx="12" cy="12" r="9" />
    <path d="M9.6 9.4a2.5 2.5 0 1 1 3.3 2.4c-.7.2-.9.8-.9 1.4" />
    <path d="M12 16.5v.01" />
  </svg>
);

const CATEGORY_ARTWORK: Record<
  ErrorCategory,
  { heading: string; Icon: React.FC<IconProps> }
> = {
  [ErrorCategory.NETWORK]: {
    heading: "Network Error",
    Icon: NetworkErrorIcon,
  },
  [ErrorCategory.VALIDATION]: {
    heading: "Validation Error",
    Icon: ValidationErrorIcon,
  },
  [ErrorCategory.AUTHENTICATION]: {
    heading: "Authentication Error",
    Icon: AuthenticationErrorIcon,
  },
  [ErrorCategory.AUTHORIZATION]: {
    heading: "Authorization Error",
    Icon: AuthenticationErrorIcon,
  },
  [ErrorCategory.BUSINESS_LOGIC]: {
    heading: "Business Logic Error",
    Icon: ValidationErrorIcon,
  },
  [ErrorCategory.SYSTEM]: {
    heading: "System Error",
    Icon: SystemErrorIcon,
  },
  [ErrorCategory.USER_INPUT]: {
    heading: "Input Error",
    Icon: ValidationErrorIcon,
  },
  [ErrorCategory.EXTERNAL_SERVICE]: {
    heading: "Service Error",
    Icon: NetworkErrorIcon,
  },
};

const GENERIC_ARTWORK = {
  heading: "Something Went Wrong",
  Icon: UnknownErrorIcon,
};

export const ErrorFallback: React.FC<ErrorFallbackProps> = ({
  error,
  resetErrorBoundary,
  onReauthenticate,
  showReloadButton = true,
}) => {
  const isAppError = error instanceof AppError;
  const category = isAppError ? error.category : ErrorCategory.SYSTEM;
  const severity = isAppError ? error.severity : ErrorSeverity.CRITICAL;

  const { heading, Icon } = CATEGORY_ARTWORK[category] ?? GENERIC_ARTWORK;

  const getErrorMessage = () => {
    if (isAppError) {
      return error.message;
    }

    // Provide user-friendly messages for common errors
    if (error.message.includes("fetch")) {
      return "Unable to connect to the server. Please check your internet connection and try again.";
    }
    if (error.message.includes("JSON")) {
      return "There was an issue processing the data. Please try again.";
    }
    if (error.message.includes("timeout")) {
      return "The request took too long to complete. Please try again.";
    }

    return "Something went wrong. Please try again.";
  };

  const handleReauthenticate = () => {
    if (onReauthenticate) {
      onReauthenticate();
      return;
    }

    // No re-authentication handler was supplied: return to the app root. The
    // app is wallet-based, so there is no dedicated sign-in page to send the
    // user to.
    window.location.href = "/";
  };

  const getActionButton = () => {
    switch (category) {
      case ErrorCategory.NETWORK:
        return (
          <button
            type="button"
            onClick={resetErrorBoundary}
            className="bg-blue-500 hover:bg-blue-600 text-white px-4 py-2 rounded-md transition-colors"
          >
            Retry Connection
          </button>
        );
      case ErrorCategory.AUTHENTICATION:
        return (
          <button
            type="button"
            onClick={handleReauthenticate}
            className="bg-red-500 hover:bg-red-600 text-white px-4 py-2 rounded-md transition-colors"
          >
            Reconnect wallet
          </button>
        );
      default:
        return (
          <button
            type="button"
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
        <div className="mb-4 flex justify-center text-gray-400">
          <Icon />
        </div>

        <h1 className="text-2xl font-bold text-gray-900 mb-4">{heading}</h1>

        <p className="text-gray-600 mb-6 leading-relaxed">
          {getErrorMessage()}
        </p>

        <div className="space-y-3">
          {getActionButton()}

          {showReloadButton && (
            <button
              type="button"
              onClick={() => window.location.reload()}
              className="block w-full bg-gray-100 hover:bg-gray-200 text-gray-700 px-4 py-2 rounded-md transition-colors"
            >
              Refresh Page
            </button>
          )}
        </div>

        {process.env.NODE_ENV === "development" && (
          <details className="mt-6 text-left">
            <summary className="cursor-pointer text-sm text-gray-500 hover:text-gray-700">
              Technical Details
            </summary>
            <div className="mt-2 p-3 bg-gray-100 rounded text-xs font-mono text-gray-700 overflow-auto">
              <div>
                <strong>Error:</strong> {error.name}
              </div>
              <div>
                <strong>Message:</strong> {error.message}
              </div>
              <div>
                <strong>Severity:</strong> {severity}
              </div>
              <div>
                <strong>Stack:</strong>
              </div>
              <pre className="whitespace-pre-wrap">{error.stack}</pre>
            </div>
          </details>
        )}
      </div>
    </div>
  );
};

export interface ErrorBoundaryProps {
  children: React.ReactNode;
  fallback?: React.ComponentType<ErrorFallbackProps>;
  onError?: (error: Error, errorInfo: React.ErrorInfo) => void;
  /**
   * Called whenever the boundary resets. The default behaviour is to re-render
   * the failed subtree only — the page is never reloaded, so client state
   * (form input, wallet session) survives.
   */
  onReset?: (details: {
    reason: "imperative-api" | "keys";
    args?: any[];
    prev?: any[] | undefined;
    next?: any[] | undefined;
  }) => void;
  /**
   * Reset the boundary automatically whenever one of these values changes.
   */
  resetKeys?: any[];
  /**
   * Handler for the fallback's "Reconnect wallet" action.
   */
  onReauthenticate?: () => void;
  /**
   * Forwarded to the fallback; see {@link ErrorFallbackProps.showReloadButton}.
   */
  showReloadButton?: boolean;
}

export const ErrorBoundary: React.FC<ErrorBoundaryProps> = ({
  children,
  fallback: Fallback = ErrorFallback,
  onError,
  onReset,
  resetKeys,
  onReauthenticate,
  showReloadButton,
}) => {
  const handleError = (error: Error, errorInfo: React.ErrorInfo) => {
    // Log error with context
    globalErrorHandler(error, {
      component: errorInfo.componentStack?.split("\n")[1]?.trim(),
      additionalData: { errorInfo },
    });

    // Call custom error handler if provided
    if (onError) {
      onError(error, errorInfo);
    }
  };

  return (
    <ReactErrorBoundary
      fallbackRender={({ error, resetErrorBoundary }) => (
        <Fallback
          error={error}
          resetErrorBoundary={resetErrorBoundary}
          onReauthenticate={onReauthenticate}
          showReloadButton={showReloadButton}
        />
      )}
      onError={handleError}
      onReset={onReset}
      resetKeys={resetKeys}
    >
      {children}
    </ReactErrorBoundary>
  );
};

export default ErrorBoundary;
