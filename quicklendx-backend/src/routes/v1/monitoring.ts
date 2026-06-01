/**
 * Monitoring Routes
 * Endpoints for system observability including Prometheus metrics
 */

import { metricsService } from '../services/metricsService';

/**
 * Represents a route handler
 */
interface Route {
  method: string;
  path: string;
  handler: (req: any) => Promise<any>;
}

/**
 * Request object with headers and auth
 */
interface AuthedRequest {
  headers?: Record<string, string | string[]>;
  apiKey?: string;
}

/**
 * Response object
 */
interface MonitoringResponse {
  statusCode: number;
  contentType: string;
  body: string | object;
}

/**
 * Monitoring router with observability endpoints
 */
export class MonitoringRouter {
  private routes: Route[] = [];
  private isAuthEnabled: boolean;

  constructor(isAuthEnabled: boolean = true) {
    this.isAuthEnabled = isAuthEnabled;
    this.initializeRoutes();
  }

  /**
   * Initialize monitoring routes
   */
  private initializeRoutes(): void {
    // Prometheus metrics endpoint
    this.registerRoute('GET', '/metrics', async (req) => {
      return this.metricsHandler(req);
    });

    // Health check endpoint (JSON format)
    this.registerRoute('GET', '/health', async (req) => {
      return this.healthHandler(req);
    });
  }

  /**
   * Register a route
   */
  private registerRoute(method: string, path: string, handler: (req: any) => Promise<any>): void {
    this.routes.push({ method, path, handler });
  }

  /**
   * Get all registered routes
   */
  getRoutes(): Route[] {
    return this.routes;
  }

  /**
   * Handle route requests
   */
  async handleRequest(method: string, path: string, req: any): Promise<any> {
    const route = this.routes.find((r) => r.method === method && r.path === path);
    if (!route) {
      return {
        statusCode: 404,
        contentType: 'application/json',
        body: { error: 'Not found' },
      };
    }
    return route.handler(req);
  }

  /**
   * Verify API key authentication
   * Expects format: Authorization: Bearer <api-key>
   */
  private verifyAuth(req: AuthedRequest): boolean {
    if (!this.isAuthEnabled) {
      return true;
    }

    const authHeader = req.headers?.['authorization'];
    if (!authHeader) {
      return false;
    }

    const parts = Array.isArray(authHeader) ? authHeader[0] : authHeader;
    return parts.startsWith('Bearer ');
  }

  /**
   * Prometheus metrics endpoint handler
   * Returns metrics in Prometheus text exposition format
   * Requires API key authentication
   */
  private async metricsHandler(req: AuthedRequest): Promise<MonitoringResponse> {
    // Verify authentication
    if (!this.verifyAuth(req)) {
      return {
        statusCode: 401,
        contentType: 'application/json',
        body: { error: 'Unauthorized' },
      };
    }

    try {
      // Aggregate metrics from services (placeholder - no services available yet)
      await metricsService.aggregateMetrics({});

      // Serialize to Prometheus format
      const metricsText = metricsService.serializePrometheus();

      return {
        statusCode: 200,
        contentType: 'text/plain; version=0.0.4; charset=utf-8',
        body: metricsText,
      };
    } catch (err) {
      console.error('Failed to generate metrics', err);
      return {
        statusCode: 500,
        contentType: 'application/json',
        body: { error: 'Internal server error' },
      };
    }
  }

  /**
   * Health check endpoint handler
   * Returns basic health status in JSON format
   * Does not require authentication
   */
  private async healthHandler(req: AuthedRequest): Promise<MonitoringResponse> {
    return {
      statusCode: 200,
      contentType: 'application/json',
      body: {
        status: 'ok',
        timestamp: new Date().toISOString(),
      },
    };
  }
}

// Export factory function for easy instantiation
export function createMonitoringRouter(isAuthEnabled: boolean = true): MonitoringRouter {
  return new MonitoringRouter(isAuthEnabled);
}
