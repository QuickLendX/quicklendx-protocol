/**
 * Metrics Service Tests
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { MetricsService, metricsService } from '../services/metricsService';
import { MonitoringRouter } from '../routes/v1/monitoring';

describe('MetricsService', () => {
  let service: MetricsService;

  beforeEach(() => {
    service = new MetricsService();
  });

  describe('initialization', () => {
    it('should initialize with default metrics at zero', () => {
      const metrics = service.getAllMetrics();
      expect(metrics).toHaveLength(5);
      expect(metrics.every((m) => m.value === 0)).toBe(true);
    });

    it('should have correct metric names', () => {
      const metrics = service.getAllMetrics();
      const names = metrics.map((m) => m.name);
      expect(names).toContain('qlx_ingest_lag_ledgers');
      expect(names).toContain('qlx_webhook_queue_depth');
      expect(names).toContain('qlx_webhook_overflow_total');
      expect(names).toContain('qlx_rpc_circuit_state');
      expect(names).toContain('qlx_invariant_violations_total');
    });

    it('should have correct metric types', () => {
      const metrics = service.getAllMetrics();
      const gauges = metrics.filter((m) => m.type === 'gauge');
      const counters = metrics.filter((m) => m.type === 'counter');

      expect(gauges).toHaveLength(3);
      expect(counters).toHaveLength(2);
    });
  });

  describe('updateMetric', () => {
    it('should update gauge metric value', () => {
      service.updateMetric('qlx_ingest_lag_ledgers', 42);
      const metric = service.getAllMetrics().find((m) => m.name === 'qlx_ingest_lag_ledgers');
      expect(metric?.value).toBe(42);
    });

    it('should handle non-existent metric gracefully', () => {
      expect(() => service.updateMetric('non_existent_metric', 10)).not.toThrow();
    });
  });

  describe('incrementCounter', () => {
    it('should increment counter metric', () => {
      service.incrementCounter('qlx_webhook_overflow_total', 1);
      const metric = service.getAllMetrics().find((m) => m.name === 'qlx_webhook_overflow_total');
      expect(metric?.value).toBe(1);
    });

    it('should increment by custom amount', () => {
      service.incrementCounter('qlx_webhook_overflow_total', 5);
      const metric = service.getAllMetrics().find((m) => m.name === 'qlx_webhook_overflow_total');
      expect(metric?.value).toBe(5);
    });

    it('should handle non-existent counter gracefully', () => {
      expect(() => service.incrementCounter('non_existent_counter', 1)).not.toThrow();
    });
  });

  describe('serializePrometheus', () => {
    it('should emit valid Prometheus format', () => {
      service.updateMetric('qlx_ingest_lag_ledgers', 100);
      const output = service.serializePrometheus();

      expect(output).toContain('# HELP qlx_ingest_lag_ledgers');
      expect(output).toContain('# TYPE qlx_ingest_lag_ledgers gauge');
      expect(output).toContain('qlx_ingest_lag_ledgers 100');
    });

    it('should include all metrics', () => {
      const output = service.serializePrometheus();

      expect(output).toContain('qlx_ingest_lag_ledgers');
      expect(output).toContain('qlx_webhook_queue_depth');
      expect(output).toContain('qlx_webhook_overflow_total');
      expect(output).toContain('qlx_rpc_circuit_state');
      expect(output).toContain('qlx_invariant_violations_total');
    });

    it('should end with newline', () => {
      const output = service.serializePrometheus();
      expect(output.endsWith('\n')).toBe(true);
    });

    it('should have HELP and TYPE lines before metric', () => {
      const output = service.serializePrometheus();
      const lines = output.split('\n');

      let foundHelp = false;
      let foundType = false;
      let foundMetric = false;

      for (let i = 0; i < lines.length; i++) {
        if (lines[i].startsWith('# HELP qlx_ingest_lag_ledgers')) {
          foundHelp = true;
        }
        if (lines[i].startsWith('# TYPE qlx_ingest_lag_ledgers')) {
          foundType = true;
        }
        if (lines[i] === 'qlx_ingest_lag_ledgers 0' && foundHelp && foundType) {
          foundMetric = true;
          break;
        }
      }

      expect(foundHelp && foundType && foundMetric).toBe(true);
    });
  });

  describe('label escaping', () => {
    it('should escape backslashes in label values', () => {
      service.updateMetric('qlx_ingest_lag_ledgers', 10);
      const metrics = service.getAllMetrics();
      const metric = metrics[0];
      metric.labels = { path: 'C:\\Users\\test' };

      const output = service.serializePrometheus();
      expect(output).toContain('path="C:\\\\Users\\\\test"');
    });

    it('should escape newlines in label values', () => {
      service.updateMetric('qlx_ingest_lag_ledgers', 10);
      const metrics = service.getAllMetrics();
      const metric = metrics[0];
      metric.labels = { error: 'Line1\nLine2' };

      const output = service.serializePrometheus();
      expect(output).toContain('error="Line1\\nLine2"');
    });

    it('should escape quotes in label values', () => {
      service.updateMetric('qlx_ingest_lag_ledgers', 10);
      const metrics = service.getAllMetrics();
      const metric = metrics[0];
      metric.labels = { message: 'Say "hello"' };

      const output = service.serializePrometheus();
      expect(output).toContain('message="Say \\"hello\\""');
    });

    it('should escape newlines in help text', () => {
      const output = service.serializePrometheus();
      // All help texts should not contain unescaped newlines
      const helpLines = output.split('\n').filter((line) => line.startsWith('# HELP'));
      for (const line of helpLines) {
        expect(line.split('# HELP ')[1]).not.toContain('\n');
      }
    });
  });

  describe('aggregateMetrics', () => {
    it('should handle missing services gracefully', async () => {
      await service.aggregateMetrics({});
      const metrics = service.getAllMetrics();
      expect(metrics.every((m) => m.value === 0)).toBe(true);
    });

    it('should update lag from lagMonitor service', async () => {
      const mockLagMonitor = {
        getLagLedgers: async () => 25,
      };
      await service.aggregateMetrics({ lagMonitor: mockLagMonitor });
      const metric = service.getAllMetrics().find((m) => m.name === 'qlx_ingest_lag_ledgers');
      expect(metric?.value).toBe(25);
    });

    it('should update webhook queue metrics', async () => {
      const mockWebhookQueue = {
        getDepth: async () => 150,
        getOverflowCount: async () => 3,
      };
      await service.aggregateMetrics({ webhookQueue: mockWebhookQueue });

      const depth = service.getAllMetrics().find((m) => m.name === 'qlx_webhook_queue_depth');
      const overflow = service.getAllMetrics().find((m) => m.name === 'qlx_webhook_overflow_total');

      expect(depth?.value).toBe(150);
      expect(overflow?.value).toBe(3);
    });

    it('should update invariant violations', async () => {
      const mockInvariantService = {
        getViolationCount: async () => 2,
      };
      await service.aggregateMetrics({ invariantService: mockInvariantService });
      const metric = service.getAllMetrics().find((m) => m.name === 'qlx_invariant_violations_total');
      expect(metric?.value).toBe(2);
    });

    it('should update RPC circuit state', async () => {
      const mockRpcClient = {
        getCircuitState: async () => 1, // 1 = open
      };
      await service.aggregateMetrics({ rpcClient: mockRpcClient });
      const metric = service.getAllMetrics().find((m) => m.name === 'qlx_rpc_circuit_state');
      expect(metric?.value).toBe(1);
    });

    it('should continue if lagMonitor fails', async () => {
      const mockServices = {
        lagMonitor: {
          getLagLedgers: async () => {
            throw new Error('Service error');
          },
        },
        webhookQueue: {
          getDepth: async () => 100,
          getOverflowCount: async () => 0,
        },
      };

      await service.aggregateMetrics(mockServices);

      const depth = service.getAllMetrics().find((m) => m.name === 'qlx_webhook_queue_depth');
      expect(depth?.value).toBe(100); // Should still work
    });

    it('should continue if webhook queue fails', async () => {
      const mockServices = {
        webhookQueue: {
          getDepth: async () => {
            throw new Error('Queue error');
          },
          getOverflowCount: async () => {
            throw new Error('Queue error');
          },
        },
        invariantService: {
          getViolationCount: async () => 5,
        },
      };

      await service.aggregateMetrics(mockServices);

      const violations = service.getAllMetrics().find((m) => m.name === 'qlx_invariant_violations_total');
      expect(violations?.value).toBe(5); // Should still work
    });
  });
});

describe('MonitoringRouter', () => {
  let router: MonitoringRouter;

  beforeEach(() => {
    router = new MonitoringRouter(true); // Auth enabled
  });

  describe('initialization', () => {
    it('should register metrics and health routes', () => {
      const routes = router.getRoutes();
      expect(routes).toHaveLength(2);
      expect(routes.some((r) => r.path === '/metrics')).toBe(true);
      expect(routes.some((r) => r.path === '/health')).toBe(true);
    });
  });

  describe('/metrics endpoint', () => {
    it('should return 401 without auth', async () => {
      const result = await router.handleRequest('GET', '/metrics', {});
      expect(result.statusCode).toBe(401);
      expect(result.contentType).toBe('application/json');
    });

    it('should return 401 with empty auth header', async () => {
      const result = await router.handleRequest('GET', '/metrics', {
        headers: { authorization: '' },
      });
      expect(result.statusCode).toBe(401);
    });

    it('should return 200 with valid Bearer token', async () => {
      const result = await router.handleRequest('GET', '/metrics', {
        headers: { authorization: 'Bearer test-key-123' },
      });
      expect(result.statusCode).toBe(200);
      expect(result.contentType).toContain('text/plain');
    });

    it('should return Prometheus format', async () => {
      const result = await router.handleRequest('GET', '/metrics', {
        headers: { authorization: 'Bearer test-key' },
      });

      const body = result.body as string;
      expect(body).toContain('# HELP');
      expect(body).toContain('# TYPE');
      expect(body).toContain('qlx_ingest_lag_ledgers');
      expect(body.endsWith('\n')).toBe(true);
    });

    it('should work without auth when disabled', async () => {
      const noAuthRouter = new MonitoringRouter(false);
      const result = await noAuthRouter.handleRequest('GET', '/metrics', {});
      expect(result.statusCode).toBe(200);
      expect(result.contentType).toContain('text/plain');
    });
  });

  describe('/health endpoint', () => {
    it('should return 200 without auth', async () => {
      const result = await router.handleRequest('GET', '/health', {});
      expect(result.statusCode).toBe(200);
      expect(result.contentType).toBe('application/json');
    });

    it('should return valid health response', async () => {
      const result = await router.handleRequest('GET', '/health', {});
      const body = result.body as { status: string; timestamp: string };
      expect(body.status).toBe('ok');
      expect(body.timestamp).toBeDefined();
    });
  });

  describe('unknown routes', () => {
    it('should return 404 for unknown path', async () => {
      const result = await router.handleRequest('GET', '/unknown', {});
      expect(result.statusCode).toBe(404);
      expect(result.contentType).toBe('application/json');
    });
  });
});
