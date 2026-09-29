/**
 * Admin Routes
 * Routes for administrative endpoints including audit export
 */

import { ExportController } from '../controllers/v1/exports';
import { ExportService } from '../services/exportService';

/**
 * Represents a route handler
 */
interface Route {
  method: string;
  path: string;
  handler: (req: any) => Promise<any>;
}

/**
 * Simple router registry
 */
export class AdminRouter {
  private routes: Route[] = [];
  private exportController: ExportController;

  constructor(exportService?: ExportService) {
    this.exportController = new ExportController(exportService);
    this.initializeRoutes();
  }

  /**
   * Initialize admin routes
   */
  private initializeRoutes(): void {
    // Audit export endpoint
    this.registerRoute('GET', '/admin/exports/audit', async (req) => {
      return this.exportController.exportAuditLog(req);
    });

    // Export statistics endpoint
    this.registerRoute('GET', '/admin/exports/stats', async (req) => {
      return this.exportController.getExportStats(req);
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
    return [...this.routes];
  }

  /**
   * Handle a request by matching against registered routes
   */
  async handleRequest(method: string, path: string, req: any): Promise<any> {
    const route = this.routes.find((r) => r.method === method && r.path === path);
    if (!route) {
      return {
        statusCode: 404,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ error: 'Route not found' }),
      };
    }

    try {
      return await route.handler(req);
    } catch (error) {
      const errorMessage = error instanceof Error ? error.message : 'Unknown error';
      return {
        statusCode: 500,
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          error: 'Internal server error',
          message: errorMessage,
        }),
      };
    }
  }
}

export default AdminRouter;
