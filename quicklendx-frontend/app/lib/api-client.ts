import type { CreateInvoiceBody } from "@/app/business/invoices/new/_lib/schema";
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

/**
 * `.passthrough()` keeps fields the backend adds next to `message` and `code`.
 * `POST /bids` answers 429 EXPOSURE_CAP_EXCEEDED with `currentExposureUsd`,
 * `attemptedUsd` and `capUsd` directly on `error`; a stripping schema would drop
 * exactly the numbers the caller needs to explain the refusal.
 */
const ErrorEnvelopeSchema = z.object({
  error: z
    .object({
      message: z.string(),
      code: z.string(),
      details: z.unknown().optional(),
    })
    .passthrough(),
});

type ErrorEnvelope = z.infer<typeof ErrorEnvelopeSchema>;

// ---------------------------------------------------------------------------
// Write methods that require CSRF + Content-Type
// ---------------------------------------------------------------------------

const CSRF_METHODS = new Set(["post", "put", "patch", "delete"]);

/**
 * Axios config plus the options this client adds.
 *
 * `retry` controls how many times a failed request is attempted in total:
 * `false` sends it exactly once, a number sets the attempt count, and leaving it
 * out keeps the default of {@link API_CONFIG.retryAttempts}. Every failure is
 * retried by default (including 4xx and 429), which is acceptable for reads but
 * wrong for a request that moves money: a timeout does not mean the server did
 * not process it, and resending would place the same bid twice. Pass
 * `retry: false` for those.
 */
export interface ApiRequestConfig extends AxiosRequestConfig {
  retry?: false | number;
}

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
      // `details` when the backend sends it, otherwise any extra fields on the
      // envelope (see ErrorEnvelopeSchema), otherwise nothing.
      const serverDetails = (() => {
        if (!parsed.success) return undefined;
        const { message: _message, code: _code, details, ...extra } =
          parsed.data.error;
        if (details !== undefined) return details;
        return Object.keys(extra).length > 0 ? extra : undefined;
      })();

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
                ...(serverDetails !== undefined
                  ? { details: serverDetails }
                  : {}),
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
    if (
      typeof crypto !== "undefined" &&
      typeof crypto.randomUUID === "function"
    ) {
      return `req_${crypto.randomUUID()}`;
    }
    return `req_${Math.random().toString(36).slice(2)}${Date.now().toString(36)}`;
  }

  private getRequestKey(config: AxiosRequestConfig): string {
    // The body is part of the identity of a write: two POSTs to the same URL with
    // different bodies are different requests, and must not be collapsed into one
    // (the second caller would silently get the first caller's result).
    let body = "";
    if (config.data !== undefined) {
      try {
        body = JSON.stringify(config.data);
      } catch {
        // Not serialisable: give it a key no other request can share.
        body = `unique_${this.generateRequestId()}`;
      }
    }
    return `${config.method?.toUpperCase()}_${config.url}_${JSON.stringify(config.params || {})}_${body}`;
  }

  // -------------------------------------------------------------------------
  // Main request method with retry logic
  // -------------------------------------------------------------------------
  async request<T = any>(config: ApiRequestConfig): Promise<T> {
    const { retry, ...axiosConfig } = config;
    const attempts =
      retry === false
        ? 1
        : typeof retry === "number" && retry >= 1
          ? Math.floor(retry)
          : API_CONFIG.retryAttempts;
    const requestKey = this.getRequestKey(axiosConfig);

    // Check if there's already a pending request with the same key
    if (this.requestQueue.has(requestKey)) {
      return this.requestQueue.get(requestKey)!;
    }

    const requestPromise = ErrorRecovery.retryOperation(
      async () => {
        const response = await this.client.request(axiosConfig);
        return response.data;
      },
      attempts,
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

  async get<T = any>(url: string, config?: ApiRequestConfig): Promise<T> {
    return this.request<T>({ ...config, method: "GET", url });
  }

  async post<T = any>(
    url: string,
    data?: any,
    config?: ApiRequestConfig
  ): Promise<T> {
    return this.request<T>({ ...config, method: "POST", url, data });
  }

  async postOnce<T = any>(
    url: string,
    data?: any,
    config?: AxiosRequestConfig
  ): Promise<T> {
    const response = await this.client.request({
      ...config,
      method: "POST",
      url,
      data,
    });
    return response.data;
  }

  async put<T = any>(
    url: string,
    data?: any,
    config?: ApiRequestConfig
  ): Promise<T> {
    return this.request<T>({ ...config, method: "PUT", url, data });
  }

  async patch<T = any>(
    url: string,
    data?: any,
    config?: ApiRequestConfig
  ): Promise<T> {
    return this.request<T>({ ...config, method: "PATCH", url, data });
  }

  async delete<T = any>(url: string, config?: ApiRequestConfig): Promise<T> {
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
export function createInvoice(body: CreateInvoiceBody) {
  return apiClient.post("/invoices", body);
}

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
