import axios, { AxiosInstance, AxiosRequestConfig, AxiosResponse, AxiosError } from 'axios';
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
  globalErrorHandler
} from './errors';

// API configuration
const API_CONFIG = {
  baseURL: process.env.NEXT_PUBLIC_API_URL || 'http://localhost:3001/api',
  timeout: 10000,
  retryAttempts: 3,
  retryDelay: 1000,
};

// Request/Response interceptors for error handling
class ApiClient {
  private client: AxiosInstance;
  private requestQueue: Map<string, Promise<any>> = new Map();

  constructor() {
    this.client = axios.create({
      baseURL: API_CONFIG.baseURL,
      timeout: API_CONFIG.timeout,
      headers: {
        'Content-Type': 'application/json',
      },
    });

    this.setupInterceptors();
  }

  private setupInterceptors(): void {
    // Request interceptor
    this.client.interceptors.request.use(
      (config) => {
        // Add authentication token if available
        const token = this.getAuthToken();
        if (token) {
          config.headers.Authorization = `Bearer ${token}`;
        }

        // Add request ID for tracking
        config.headers['X-Request-ID'] = this.generateRequestId();

        return config;
      },
      (error) => {
        const appError = new SystemError(
          'Failed to prepare request',
          { action: 'request_interceptor' }
        );
        globalErrorHandler(appError);
        return Promise.reject(appError);
      }
    );

    // Response interceptor
    this.client.interceptors.response.use(
      (response) => {
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

    // Network error (no response received)
    if (!response && request) {
      return new NetworkError(
        'Network connection failed. Please check your internet connection.',
        {
          action: config?.method?.toUpperCase() || 'UNKNOWN',
          additionalData: {
            url: config?.url,
            timeout: config?.timeout,
          }
        }
      );
    }

    // HTTP error response
    if (response) {
      const status = response.status;
      const data = response.data as any;
      const message = data?.message || data?.error || `HTTP ${status} error`;

      switch (status) {
        case 400:
          return new ValidationError(
            message,
            {
              action: config?.method?.toUpperCase() || 'UNKNOWN',
              additionalData: {
                url: config?.url,
                validationErrors: data?.errors,
                field: data?.field,
              }
            }
          );

        case 401:
          return new AuthenticationError(
            'Authentication required. Please log in again.',
            {
              action: config?.method?.toUpperCase() || 'UNKNOWN',
              additionalData: {
                url: config?.url,
                originalMessage: message,
              }
            }
          );

        case 403:
          return new AuthorizationError(
            'You do not have permission to perform this action.',
            {
              action: config?.method?.toUpperCase() || 'UNKNOWN',
              additionalData: {
                url: config?.url,
                originalMessage: message,
              }
            }
          );

        case 404:
          return new BusinessLogicError(
            'The requested resource was not found.',
            {
              action: config?.method?.toUpperCase() || 'UNKNOWN',
              additionalData: {
                url: config?.url,
                originalMessage: message,
              }
            }
          );

        case 422:
          return new ValidationError(
            message,
            {
              action: config?.method?.toUpperCase() || 'UNKNOWN',
              additionalData: {
                url: config?.url,
                validationErrors: data?.errors,
              }
            }
          );

        case 429:
          return new NetworkError(
            'Too many requests. Please try again later.',
            {
              action: config?.method?.toUpperCase() || 'UNKNOWN',
              additionalData: {
                url: config?.url,
                retryAfter: response.headers['retry-after'],
              }
            },
            0 // Don't retry rate limit errors
          );

        case 500:
        case 502:
        case 503:
        case 504:
          return new SystemError(
            'Server error. Please try again later.',
            {
              action: config?.method?.toUpperCase() || 'UNKNOWN',
              additionalData: {
                url: config?.url,
                status,
                originalMessage: message,
              }
            }
          );

        default:
          return new SystemError(
            `Unexpected error (${status}): ${message}`,
            {
              action: config?.method?.toUpperCase() || 'UNKNOWN',
              additionalData: {
                url: config?.url,
                status,
                originalMessage: message,
              }
            }
          );
      }
    }

    // Timeout error
    if (error.code === 'ECONNABORTED') {
      return new NetworkError(
        'Request timed out. Please try again.',
        {
          action: config?.method?.toUpperCase() || 'UNKNOWN',
          additionalData: {
            url: config?.url,
            timeout: config?.timeout,
          }
        }
      );
    }

    // Unknown error
    return new SystemError(
      'An unexpected error occurred.',
      {
        action: config?.method?.toUpperCase() || 'UNKNOWN',
        additionalData: {
          url: config?.url,
          originalError: error.message,
        }
      }
    );
  }

  private getAuthToken(): string | null {
    if (typeof window === 'undefined') return null;
    return localStorage.getItem('auth_token') || sessionStorage.getItem('auth_token');
  }

  private generateRequestId(): string {
    return `req_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
  }

  private getRequestKey(config: AxiosRequestConfig): string {
    return `${config.method?.toUpperCase()}_${config.url}_${JSON.stringify(config.params || {})}`;
  }

  // Main request method with retry logic
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

  // Convenience methods
  async get<T = any>(url: string, config?: AxiosRequestConfig): Promise<T> {
    return this.request<T>({ ...config, method: 'GET', url });
  }

  async post<T = any>(url: string, data?: any, config?: AxiosRequestConfig): Promise<T> {
    return this.request<T>({ ...config, method: 'POST', url, data });
  }

  async put<T = any>(url: string, data?: any, config?: AxiosRequestConfig): Promise<T> {
    return this.request<T>({ ...config, method: 'PUT', url, data });
  }

  async patch<T = any>(url: string, data?: any, config?: AxiosRequestConfig): Promise<T> {
    return this.request<T>({ ...config, method: 'PATCH', url, data });
  }

  async delete<T = any>(url: string, config?: AxiosRequestConfig): Promise<T> {
    return this.request<T>({ ...config, method: 'DELETE', url });
  }

  // Upload method with progress tracking
  async upload<T = any>(
    url: string, 
    file: File, 
    onProgress?: (progress: number) => void,
    config?: AxiosRequestConfig
  ): Promise<T> {
    const formData = new FormData();
    formData.append('file', file);

    return this.request<T>({
      ...config,
      method: 'POST',
      url,
      data: formData,
      headers: {
        'Content-Type': 'multipart/form-data',
      },
      onUploadProgress: (progressEvent) => {
        if (onProgress && progressEvent.total) {
          const progress = Math.round((progressEvent.loaded * 100) / progressEvent.total);
          onProgress(progress);
        }
      },
    });
  }

  // Health check method
  async healthCheck(): Promise<boolean> {
    try {
      await this.get('/health');
      return true;
    } catch (error) {
      return false;
    }
  }

  // Clear request queue
  clearQueue(): void {
    this.requestQueue.clear();
  }
}

// Singleton instance
export const apiClient = new ApiClient();

// Hook for using API client with error handling
export const useApiClient = () => {
  return {
    get: <T = any>(url: string, config?: AxiosRequestConfig) => 
      ErrorRecovery.handleAsyncError(apiClient.get<T>(url, config), null as T),
    post: <T = any>(url: string, data?: any, config?: AxiosRequestConfig) => 
      ErrorRecovery.handleAsyncError(apiClient.post<T>(url, data, config), null as T),
    put: <T = any>(url: string, data?: any, config?: AxiosRequestConfig) => 
      ErrorRecovery.handleAsyncError(apiClient.put<T>(url, data, config), null as T),
    patch: <T = any>(url: string, data?: any, config?: AxiosRequestConfig) => 
      ErrorRecovery.handleAsyncError(apiClient.patch<T>(url, data, config), null as T),
    delete: <T = any>(url: string, config?: AxiosRequestConfig) => 
      ErrorRecovery.handleAsyncError(apiClient.delete<T>(url, config), null as T),
    upload: <T = any>(
      url: string, 
      file: File, 
      onProgress?: (progress: number) => void,
      config?: AxiosRequestConfig
    ) => ErrorRecovery.handleAsyncError(apiClient.upload<T>(url, file, onProgress, config), null as T),
    healthCheck: () => apiClient.healthCheck(),
  };
}; 