"use client";

import toast from "react-hot-toast";
import { AppError, ErrorCategory, ErrorSeverity } from "../lib/errors";

interface ErrorToastProps {
  error: AppError;
  toastId?: string;
}

// --- Severity icons (inline SVG, no extra deps). currentColor lets the
// parent set the accent color; aria-hidden because the text label conveys
// meaning (never color/icon alone). ---
type IconProps = { className?: string };

const AlertOctagonIcon: React.FC<IconProps> = ({ className }) => (
  <svg
    className={className}
    width="20"
    height="20"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <polygon points="7.86 2 16.14 2 22 7.86 22 16.14 16.14 22 7.86 22 2 16.14 2 7.86" />
    <line x1="12" y1="8" x2="12" y2="12" />
    <line x1="12" y1="16" x2="12.01" y2="16" />
  </svg>
);

const AlertTriangleIcon: React.FC<IconProps> = ({ className }) => (
  <svg
    className={className}
    width="20"
    height="20"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z" />
    <line x1="12" y1="9" x2="12" y2="13" />
    <line x1="12" y1="17" x2="12.01" y2="17" />
  </svg>
);

const InfoIcon: React.FC<IconProps> = ({ className }) => (
  <svg
    className={className}
    width="20"
    height="20"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <circle cx="12" cy="12" r="10" />
    <line x1="12" y1="16" x2="12" y2="12" />
    <line x1="12" y1="8" x2="12.01" y2="8" />
  </svg>
);

const CloseIcon: React.FC<IconProps> = ({ className }) => (
  <svg
    className={className}
    width="16"
    height="16"
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <line x1="18" y1="6" x2="6" y2="18" />
    <line x1="6" y1="6" x2="18" y2="18" />
  </svg>
);

// --- Severity → design-token visual mapping (docs/ux/design-tokens.md). ---
// A light neutral surface (#FFFFFF + neutral-200 border) with a severity-colored
// accent (left border + icon, using the danger/warning/info -500 tokens) keeps
// title/message text contrast well above WCAG AA, and never conveys state by
// color alone: every toast pairs the accent with an icon AND a text label.
type SeverityVisual = {
  accent: string; // semantic -500 token: left border + icon color
  Icon: React.FC<IconProps>;
  role: "alert" | "status";
  ariaLive: "assertive" | "polite";
};

const SEVERITY_VISUALS: Record<ErrorSeverity, SeverityVisual> = {
  [ErrorSeverity.CRITICAL]: {
    accent: "#EF4444",
    Icon: AlertOctagonIcon,
    role: "alert",
    ariaLive: "assertive",
  },
  [ErrorSeverity.HIGH]: {
    accent: "#EF4444",
    Icon: AlertTriangleIcon,
    role: "alert",
    ariaLive: "assertive",
  },
  [ErrorSeverity.MEDIUM]: {
    accent: "#F59E0B",
    Icon: AlertTriangleIcon,
    role: "status",
    ariaLive: "polite",
  },
  [ErrorSeverity.LOW]: {
    accent: "#3B82F6",
    Icon: InfoIcon,
    role: "status",
    ariaLive: "polite",
  },
};

const CATEGORY_LABELS: Record<ErrorCategory, string> = {
  [ErrorCategory.NETWORK]: "Network Error",
  [ErrorCategory.VALIDATION]: "Validation Error",
  [ErrorCategory.AUTHENTICATION]: "Authentication Error",
  [ErrorCategory.AUTHORIZATION]: "Authorization Error",
  [ErrorCategory.BUSINESS_LOGIC]: "Business Logic Error",
  [ErrorCategory.SYSTEM]: "System Error",
  [ErrorCategory.USER_INPUT]: "Input Error",
  [ErrorCategory.EXTERNAL_SERVICE]: "Service Error",
};

export const ErrorToast: React.FC<ErrorToastProps> = ({ error, toastId }) => {
  const visual =
    SEVERITY_VISUALS[error.severity] ?? SEVERITY_VISUALS[ErrorSeverity.LOW];
  const label = CATEGORY_LABELS[error.category] ?? "Error";
  const { Icon } = visual;

  const handleRetry = () => {
    toast.dismiss(toastId);
    // Retry handoff stays with the error handler; surface immediate feedback.
    toast.success("Retrying operation...");
  };

  return (
    <div
      role={visual.role}
      aria-live={visual.ariaLive}
      aria-atomic="true"
      className="flex items-start gap-3 rounded-lg border border-[#E2E8F0] bg-white p-4"
      style={{
        width: "min(92vw, 380px)",
        borderLeft: `4px solid ${visual.accent}`,
        boxShadow: "0 10px 15px -3px rgba(0, 0, 0, 0.1)",
      }}
    >
      <span className="mt-0.5 shrink-0" style={{ color: visual.accent }}>
        <Icon />
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-sm font-semibold text-[#0F172A]">{label}</p>
        <p className="mt-0.5 break-words text-xs text-[#334155]">
          {error.message}
        </p>
        {error.retryable && error.retryCount < 3 && (
          <button
            type="button"
            onClick={handleRetry}
            className="mt-2 rounded px-2 py-1 text-xs font-medium text-[#2563EB] hover:bg-[#DBEAFE] focus:outline-none focus-visible:ring-2 focus-visible:ring-[#2563EB]"
          >
            Retry
          </button>
        )}
      </div>
      <button
        type="button"
        onClick={() => toast.dismiss(toastId)}
        aria-label="Dismiss notification"
        className="-mr-1 -mt-1 shrink-0 rounded p-1 text-[#64748B] hover:bg-[#F1F5F9] hover:text-[#0F172A] focus:outline-none focus-visible:ring-2 focus-visible:ring-[#2563EB]"
      >
        <CloseIcon />
      </button>
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

  showError(
    error: AppError,
    options?: {
      duration?: number;
      dismissible?: boolean;
      position?:
        | "top-right"
        | "top-center"
        | "top-left"
        | "bottom-right"
        | "bottom-center"
        | "bottom-left";
    }
  ): string {
    const toastId = `error-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;

    // Auto-dismiss timing per severity (docs/ux/toasts.md). Critical errors
    // persist until manually dismissed so they cannot be missed.
    const severityDuration: Record<ErrorSeverity, number> = {
      [ErrorSeverity.LOW]: 4000,
      [ErrorSeverity.MEDIUM]: 6000,
      [ErrorSeverity.HIGH]: 8000,
      [ErrorSeverity.CRITICAL]: Infinity,
    };
    const defaultDuration = severityDuration[error.severity] ?? 5000;
    const duration = options?.duration ?? defaultDuration;
    const dismissible = options?.dismissible ?? true;
    const position = options?.position ?? "top-right";

    toast.custom((t) => <ErrorToast error={error} toastId={t.id} />, {
      id: toastId,
      duration: dismissible ? duration : Infinity,
      position,
      style: {
        background: "transparent",
        padding: 0,
        margin: 0,
        boxShadow: "none",
      },
    });

    this.activeToasts.add(toastId);

    // Auto-remove from tracking once the toast has been dismissed. Persistent
    // (Infinity) toasts are cleaned up by dismissToast/dismissAll instead.
    if (Number.isFinite(duration)) {
      setTimeout(() => {
        this.activeToasts.delete(toastId);
      }, duration + 1000);
    }

    return toastId;
  }

  showSuccess(
    message: string,
    options?: {
      duration?: number;
      position?:
        | "top-right"
        | "top-center"
        | "top-left"
        | "bottom-right"
        | "bottom-center"
        | "bottom-left";
    }
  ): string {
    return toast.success(message, {
      duration: options?.duration ?? 3000,
      position: options?.position ?? "top-right",
      style: {
        background: "#10B981",
        color: "#fff",
        fontWeight: "500",
      },
    });
  }

  showWarning(
    message: string,
    options?: {
      duration?: number;
      position?:
        | "top-right"
        | "top-center"
        | "top-left"
        | "bottom-right"
        | "bottom-center"
        | "bottom-left";
    }
  ): string {
    return toast(message, {
      duration: options?.duration ?? 4000,
      position: options?.position ?? "top-right",
      icon: "⚠️",
      style: {
        background: "#F59E0B",
        color: "#fff",
        fontWeight: "500",
      },
    });
  }

  showInfo(
    message: string,
    options?: {
      duration?: number;
      position?:
        | "top-right"
        | "top-center"
        | "top-left"
        | "bottom-right"
        | "bottom-center"
        | "bottom-left";
    }
  ): string {
    return toast(message, {
      duration: options?.duration ?? 3000,
      position: options?.position ?? "top-right",
      icon: "ℹ️",
      style: {
        background: "#3B82F6",
        color: "#fff",
        fontWeight: "500",
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
    showError: (
      error: AppError,
      options?: Parameters<typeof toastManager.showError>[1]
    ) => toastManager.showError(error, options),
    showSuccess: (
      message: string,
      options?: Parameters<typeof toastManager.showSuccess>[1]
    ) => toastManager.showSuccess(message, options),
    showWarning: (
      message: string,
      options?: Parameters<typeof toastManager.showWarning>[1]
    ) => toastManager.showWarning(message, options),
    showInfo: (
      message: string,
      options?: Parameters<typeof toastManager.showInfo>[1]
    ) => toastManager.showInfo(message, options),
    dismissAll: () => toastManager.dismissAll(),
    dismissToast: (toastId: string) => toastManager.dismissToast(toastId),
  };
};

// Global error toast handler
export const handleErrorWithToast = (
  error: AppError,
  context?: string
): void => {
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
