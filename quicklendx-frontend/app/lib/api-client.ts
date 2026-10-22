import { randomUUID } from "crypto";
import axios, {
  AxiosInstance,
  AxiosRequestConfig,
  AxiosResponse,
  AxiosError,
} from "axios";
import { z } from "zod";
import {
  AppError,
  NetworkError,
  AuthenticationError,
  AuthorizationError,
  ValidationError,
  BusinessLogicError,
  SystemError,
  ErrorRecovery,
  ErrorCategory,
  ErrorSeverity,
  globalErrorHandler,
} from "./errors";

// ---------------------------------------------------------------------------
// API configuration
// ---------------------------------------------------------------------------

/**
 * Default base URL matches the backend's v1 router mount point:
 *   backend/src/app.ts → app.use("/api/v1", v1Routes)
 *   backend/openapi.yaml → servers[0].url = http://localhost:3001/api/v1
 *
 * NEXT_PUBLIC_API_URL, when set, must include the full prefix including /api/v1.
 */
const API_CONFIG = {
  baseURL: process.env.NEXT_PUBLIC_API_URL || "http://localhost:3001/api/v1",
  timeout: 10000,
  retryAttempts: 3,
  retryDelay: 1000,
};

// ---------------------------------------------------------------------------
// CSRF token
// ---------------------------------------------------------------------------

/**
 * Returns a per-session random CSRF token.
 *
 * The backend's CSRF middleware (backend/src/middleware/csrf.ts) only requires
 * the X-CSRF-Token header to be *present* and non-empty for browser-driven
 * requests (those carrying an Origin header). A stable per-session value
 * satisfies that requirement without a round-trip to fetch a server-issued token.
 */
let _csrfToken: string | null = null;

export function getCsrfToken(): string {
  if (!_csrfToken) {
    _csrfToken =
      typeof crypto !== "undefined" && crypto.randomUUID
        ? crypto.randomUUID()
        : Math.random().toString(36).slice(2);
  }
  return _csrfToken;
}

// Exported for testing – allows resetting the singleton between test runs.
export function _resetCsrfToken(): void {
  _csrfToken = null;
}

// ---------------------------------------------------------------------------
// Error envelope schema (matches backend/openapi.yaml → ErrorEnvelope)
// ---------------------------------------------------------------------------

const ErrorEnvelopeSchema = z.object({
  error: z.object({
    message: z.string(),
    code: z.string(),
    details: z.unknown().optional(),
  }),
});

type ErrorEnvelope = z.infer<typeof ErrorEnvelopeSchema>;

// ---------------------------------------------------------------------------
// Write methods that require CSRF + Content-Type
// ---------------------------------------------------------------------------

const CSRF_METHODS = new Set(["post", "put", "patch", "delete"]);

// ---------------------------------------------------------------------------
// ApiClient
// ---------------------------------------------------------------------------

class ApiClient {
  private client: AxiosInstance;
  private requestQueue: Map<string, Promise<any>> = new Map();

  constructor() {
    this.client = axios.create({
      baseURL: API_CONFIG.baseURL,
      timeout: API_CONFIG.timeout,
      headers: {
        "Content-Type": "application/json",
      },
    });

    this.setupInterceptors();
  }

  private setupInterceptors(): void {
    // -----------------------------------------------------------------------
    // Request interceptor
    // -----------------------------------------------------------------------
    this.client.interceptors.request.use(
      (config) => {
        // Add authentication token if available
        const token = this.getAuthToken();
        if (token) {
          config.headers.Authorization = `Bearer ${token}`;
        }

        // Add request ID for tracking
        config.headers["X-Request-ID"] = this.generateRequestId();

        // CSRF token + Content-Type for all state-changing methods.
        // The backend's csrf.ts checks for X-CSRF-Token when Origin is present
        // and requires Content-Type: application/json.
        // Reference: backend/src/middleware/csrf.ts – STATE_CHANGING_METHODS
        const method = (config.method ?? "").toLowerCase();
        if (CSRF_METHODS.has(method)) {
          config.headers["X-CSRF-Token"] = getCsrfToken();
          config.headers["Content-Type"] = "application/json";
        }

        return config;
      },
      (error) => {
        const appError = new SystemError("Failed to prepare request", {
          action: "request_interceptor",
        });
        globalErrorHandler(appError);
        return Promise.reject(appError);
      }
    );

    // -----------------------------------------------------------------------
    // Response interceptor
    // -----------------------------------------------------------------------
    this.client.interceptors.response.use(
      (response: AxiosResponse) => {
        return response;
      },
      (error: AxiosError) => {
        const appError = this.handleAxiosError(error);
        globalErrorHandler(appError);
        return Promise.reject(appError);
      }
    );
  }

  private handleAxiosError(error: AxiosError): AppError {
    const config = error.config;
    const response = error.response;
    const request = error.request;

    // ------------------------------------------------------------------
    // 1. Timeout – must be checked BEFORE the generic no-response branch
    //    because ECONNABORTED errors always have error.request set, so the
    //    !response && request guard below would fire first otherwise.
    // ------------------------------------------------------------------
    if (error.code === "ECONNABORTED" || error.code === "ETIMEDOUT") {
      return new NetworkError(
        "Request timed out. Please try again.",
        {
          action: config?.method?.toUpperCase() || "UNKNOWN",
          additionalData: {
            url: config?.url,
            timeout: config?.timeout,
          },
        },
        0
      );
    }

    // ------------------------------------------------------------------
    // 2. Network error (no response received, not a timeout)
    // ------------------------------------------------------------------
    if (!response && request) {
      return new NetworkError(
        "Network connection failed. Please check your internet connection.",
        {
          action: config?.method?.toUpperCase() || "UNKNOWN",
          additionalData: {
            url: config?.url,
            timeout: config?.timeout,
          },
        }
      );
    }

    // ------------------------------------------------------------------
    // 3. HTTP error response – parse the ErrorEnvelope
    //    backend/openapi.yaml → components.schemas.ErrorEnvelope:
    //      { error: { message: string, code: string, details?: unknown } }
    // ------------------------------------------------------------------
    if (response) {
      const status = response.status;
      const rawData = response.data;
      const method = config?.method?.toUpperCase() || "UNKNOWN";
      const url = config?.url;

      // Try to parse the standard error envelope.
      const parsed = ErrorEnvelopeSchema.safeParse(rawData);

      const serverMessage = parsed.success
        ? parsed.data.error.message
        : undefined;
      const serverCode = parsed.success ? parsed.data.error.code : undefined;
      const serverDetails = parsed.success
        ? parsed.data.error.details
        : undefined;

      // Fall back to a generic status-based message when the body is not a
      // well-formed ErrorEnvelope (e.g., HTML error pages from a reverse proxy).
      const fallbackMessage = `HTTP ${status} error`;

      switch (status) {
        case 400:
          return new ValidationError(
            serverMessage ?? fallbackMessage,
            {
              action: method,
              additionalData: {
                url,
                ...(serverDetails !== undefined
                  ? { details: serverDetails }
                  : {}),
              },
            },
            serverCode
          );

        case 401:
          return new AuthenticationError(
            serverMessage ?? "Authentication required. Please log in again.",
            {
              action: method,
              additionalData: { url },
            },
            serverCode
          );

        case 403:
          return new AuthorizationError(
            serverMessage ??
              "You do not have permission to perform this action.",
            {
              action: method,
              additionalData: { url },
            },
            serverCode
          );

        case 404:
          return new BusinessLogicError(
            serverMessage ?? "The requested resource was not found.",
            {
              action: method,
              additionalData: { url },
            },
            serverCode
          );

        case 422:
          return new ValidationError(
            serverMessage ?? fallbackMessage,
            {
              action: method,
              additionalData: {
                url,
                ...(serverDetails !== undefined
                  ? { details: serverDetails }
                  : {}),
              },
            },
            serverCode
          );

        case 429:
          return new NetworkError(
            serverMessage ?? "Too many requests. Please try again later.",
            {
              action: method,
              additionalData: {
                url,
                retryAfter: response.headers["retry-after"],
              },
            },
            0, // Don't retry rate limit errors
            serverCode
          );

        case 500:
        case 502:
        case 503:
        case 504:
          return new SystemError(
            serverMessage ?? "Server error. Please try again later.",
            {
              action: method,
              additionalData: { url, status },
            },
            serverCode
          );

        default:
          return new SystemError(
            serverMessage ?? `Unexpected error (${status})`,
            {
              action: method,
              additionalData: { url, status },
            },
            serverCode
          );
      }
    }

    // ------------------------------------------------------------------
    // 4. Unknown error (no request, no response)
    // ------------------------------------------------------------------
    return new SystemError("An unexpected error occurred.", {
      action: config?.method?.toUpperCase() || "UNKNOWN",
      additionalData: {
        url: config?.url,
        originalError: error.message,
      },
    });
  }

  private getAuthToken(): string | null {
    if (typeof window === "undefined") return null;
    return (
      localStorage.getItem("auth_token") || sessionStorage.getItem("auth_token")
    );
  }

  private generateRequestId(): string {
    return `req_${randomUUID()}`;
  }

  private getRequestKey(config: AxiosRequestConfig): string {
    return `${config.method?.toUpperCase()}_${config.url}_${JSON.stringify(config.params || {})}`;
  }

  // -------------------------------------------------------------------------
  // Main request method with retry logic
  // -------------------------------------------------------------------------
  async request<T = any>(config: AxiosRequestConfig): Promise<T> {
    const requestKey = this.getRequestKey(config);

    // Check if there's already a pending request with the same key
    if (this.requestQueue.has(requestKey)) {
      return this.requestQueue.get(requestKey)!;
    }

    const requestPromise = ErrorRecovery.retryOperation(
      async () => {
        const response = await this.client.request(config);
        return response.data;
      },
      API_CONFIG.retryAttempts,
      API_CONFIG.retryDelay
    );

    this.requestQueue.set(requestKey, requestPromise);

    try {
      const result = await requestPromise;
      return result;
    } finally {
      this.requestQueue.delete(requestKey);
    }
  }

  // -------------------------------------------------------------------------
  // Convenience methods
  // -------------------------------------------------------------------------

  async get<T = any>(url: string, config?: AxiosRequestConfig): Promise<T> {
    return this.request<T>({ ...config, method: "GET", url });
  }

  async post<T = any>(
    url: string,
    data?: any,
    config?: AxiosRequestConfig
  ): Promise<T> {
    return this.request<T>({ ...config, method: "POST", url, data });
  }

  async put<T = any>(
    url: string,
    data?: any,
    config?: AxiosRequestConfig
  ): Promise<T> {
    return this.request<T>({ ...config, method: "PUT", url, data });
  }

  async patch<T = any>(
    url: string,
    data?: any,
    config?: AxiosRequestConfig
  ): Promise<T> {
    return this.request<T>({ ...config, method: "PATCH", url, data });
  }

  async delete<T = any>(url: string, config?: AxiosRequestConfig): Promise<T> {
    return this.request<T>({ ...config, method: "DELETE", url });
  }

  /**
   * File uploads are NOT supported by the v1 API.
   *
   * The backend's CSRF middleware rejects multipart/form-data with a 415
   * INVALID_CONTENT_TYPE response (backend/src/middleware/csrf.ts).
   * Throwing locally avoids a confusing remote rejection.
   */
  async upload<T = any>(
    _url: string,
    _file: File,
    _onProgress?: (progress: number) => void,
    _config?: AxiosRequestConfig
  ): Promise<T> {
    throw new ValidationError("Uploads are not supported by the v1 API");
  }

  // -------------------------------------------------------------------------
  // Utility
  // -------------------------------------------------------------------------

  async healthCheck(): Promise<boolean> {
    try {
      await this.get("/health");
      return true;
    } catch (error) {
      return false;
    }
  }

  clearQueue(): void {
    this.requestQueue.clear();
  }
}

// ---------------------------------------------------------------------------
// Singleton instance
// ---------------------------------------------------------------------------
export const apiClient = new ApiClient();

// ---------------------------------------------------------------------------
// Hook for using API client with error handling
// ---------------------------------------------------------------------------
export const useApiClient = () => {
  return {
    get: <T = any>(url: string, config?: AxiosRequestConfig) =>
      ErrorRecovery.handleAsyncError(apiClient.get<T>(url, config), null as T),
    post: <T = any>(url: string, data?: any, config?: AxiosRequestConfig) =>
      ErrorRecovery.handleAsyncError(
        apiClient.post<T>(url, data, config),
        null as T
      ),
    put: <T = any>(url: string, data?: any, config?: AxiosRequestConfig) =>
      ErrorRecovery.handleAsyncError(
        apiClient.put<T>(url, data, config),
        null as T
      ),
    patch: <T = any>(url: string, data?: any, config?: AxiosRequestConfig) =>
      ErrorRecovery.handleAsyncError(
        apiClient.patch<T>(url, data, config),
        null as T
      ),
    delete: <T = any>(url: string, config?: AxiosRequestConfig) =>
      ErrorRecovery.handleAsyncError(
        apiClient.delete<T>(url, config),
        null as T
      ),
    upload: <T = any>(
      url: string,
      file: File,
      onProgress?: (progress: number) => void,
      config?: AxiosRequestConfig
    ) =>
      ErrorRecovery.handleAsyncError(
        apiClient.upload<T>(url, file, onProgress, config),
        null as T
      ),
    healthCheck: () => apiClient.healthCheck(),
  };
};
