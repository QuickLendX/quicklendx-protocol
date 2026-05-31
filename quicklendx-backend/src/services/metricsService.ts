/**
 * Prometheus metrics service
 * Aggregates operational signals from various backend services
 * and exposes them in Prometheus text exposition format
 */

/**
 * Simple logger utility
 */
const createLogger = (name: string) => ({
  warn: (msg: string, context?: any) => console.warn(`[${name}] ${msg}`, context),
  error: (msg: string, context?: any) => console.error(`[${name}] ${msg}`, context),
});

const logger = createLogger('metricsService');

/**
 * Prometheus metric types
 */
export type MetricType = 'gauge' | 'counter';

/**
 * Individual metric entry
 */
export interface MetricEntry {
  name: string;
  type: MetricType;
  value: number;
  labels?: Record<string, string>;
  help?: string;
}

/**
 * Service metrics aggregator
 */
export class MetricsService {
  private metrics: Map<string, MetricEntry> = new Map();

  constructor() {
    this.initializeDefaultMetrics();
  }

  /**
   * Initialize default metrics with zero values
   */
  private initializeDefaultMetrics(): void {
    // Ingest lag (ledgers behind)
    this.metrics.set('qlx_ingest_lag_ledgers', {
      name: 'qlx_ingest_lag_ledgers',
      type: 'gauge',
      value: 0,
      help: 'Current ingest lag in ledgers',
    });

    // Webhook queue depth
    this.metrics.set('qlx_webhook_queue_depth', {
      name: 'qlx_webhook_queue_depth',
      type: 'gauge',
      value: 0,
      help: 'Current webhook queue depth',
    });

    // Webhook overflow counter
    this.metrics.set('qlx_webhook_overflow_total', {
      name: 'qlx_webhook_overflow_total',
      type: 'counter',
      value: 0,
      help: 'Total webhook queue overflows',
    });

    // RPC circuit breaker state (0=closed, 1=open, 2=half-open)
    this.metrics.set('qlx_rpc_circuit_state', {
      name: 'qlx_rpc_circuit_state',
      type: 'gauge',
      value: 0,
      help: 'RPC circuit breaker state (0=closed, 1=open, 2=half-open)',
    });

    // Invariant violations counter
    this.metrics.set('qlx_invariant_violations_total', {
      name: 'qlx_invariant_violations_total',
      type: 'counter',
      value: 0,
      help: 'Total invariant violations detected',
    });
  }

  /**
   * Update metric value
   */
  updateMetric(name: string, value: number): void {
    const metric = this.metrics.get(name);
    if (metric) {
      metric.value = value;
    } else {
      logger.warn(`Attempted to update non-existent metric: ${name}`);
    }
  }

  /**
   * Increment counter metric
   */
  incrementCounter(name: string, increment: number = 1): void {
    const metric = this.metrics.get(name);
    if (metric && metric.type === 'counter') {
      metric.value += increment;
    } else {
      logger.warn(`Attempted to increment non-counter metric: ${name}`);
    }
  }

  /**
   * Get all metrics
   */
  getAllMetrics(): MetricEntry[] {
    return Array.from(this.metrics.values());
  }

  /**
   * Serialize metrics to Prometheus text exposition format
   * Reference: https://prometheus.io/docs/instrumenting/exposition_formats/
   */
  serializePrometheus(): string {
    const lines: string[] = [];

    // Group metrics by name for help text
    const processedNames = new Set<string>();

    for (const metric of this.getAllMetrics()) {
      if (!processedNames.has(metric.name)) {
        // Add HELP line
        if (metric.help) {
          const escapedHelp = this.escapeHelpText(metric.help);
          lines.push(`# HELP ${metric.name} ${escapedHelp}`);
        }

        // Add TYPE line
        lines.push(`# TYPE ${metric.name} ${metric.type}`);
        processedNames.add(metric.name);
      }

      // Add metric line
      const metricLine = this.formatMetricLine(metric);
      lines.push(metricLine);
    }

    // Add final newline per Prometheus format spec
    lines.push('');

    return lines.join('\n');
  }

  /**
   * Format individual metric line
   */
  private formatMetricLine(metric: MetricEntry): string {
    let line = metric.name;

    // Add labels if present
    if (metric.labels && Object.keys(metric.labels).length > 0) {
      const labelPairs = Object.entries(metric.labels)
        .map(([key, value]) => `${key}="${this.escapeLabelValue(value)}"`)
        .join(',');
      line += `{${labelPairs}}`;
    }

    // Add value
    line += ` ${metric.value}`;

    return line;
  }

  /**
   * Escape label value for Prometheus format
   * Per spec: backslash, newline, double quote must be escaped
   */
  private escapeLabelValue(value: string): string {
    return value
      .replace(/\\/g, '\\\\') // backslash
      .replace(/\n/g, '\\n') // newline
      .replace(/"/g, '\\"'); // double quote
  }

  /**
   * Escape help text
   * Per spec: only newlines need escaping
   */
  private escapeHelpText(text: string): string {
    return text.replace(/\n/g, '\\n');
  }

  /**
   * Aggregate metrics from various services with error handling
   * Returns gracefully even if individual services fail
   */
  async aggregateMetrics(services: {
    lagMonitor?: { getLagLedgers: () => Promise<number> };
    webhookQueue?: { getDepth: () => Promise<number>; getOverflowCount: () => Promise<number> };
    invariantService?: { getViolationCount: () => Promise<number> };
    rpcClient?: { getCircuitState: () => Promise<number> };
  }): Promise<void> {
    // Ingest lag
    if (services.lagMonitor) {
      try {
        const lag = await services.lagMonitor.getLagLedgers();
        this.updateMetric('qlx_ingest_lag_ledgers', lag);
      } catch (err) {
        logger.error('Failed to get ingest lag', { error: err });
        // Continue with default/previous value
      }
    }

    // Webhook queue metrics
    if (services.webhookQueue) {
      try {
        const [depth, overflow] = await Promise.all([
          services.webhookQueue.getDepth(),
          services.webhookQueue.getOverflowCount(),
        ]);
        this.updateMetric('qlx_webhook_queue_depth', depth);
        this.updateMetric('qlx_webhook_overflow_total', overflow);
      } catch (err) {
        logger.error('Failed to get webhook queue metrics', { error: err });
      }
    }

    // Invariant violations
    if (services.invariantService) {
      try {
        const violations = await services.invariantService.getViolationCount();
        this.updateMetric('qlx_invariant_violations_total', violations);
      } catch (err) {
        logger.error('Failed to get invariant violation count', { error: err });
      }
    }

    // RPC circuit state
    if (services.rpcClient) {
      try {
        const state = await services.rpcClient.getCircuitState();
        this.updateMetric('qlx_rpc_circuit_state', state);
      } catch (err) {
        logger.error('Failed to get RPC circuit state', { error: err });
      }
    }
  }
}

// Singleton instance
export const metricsService = new MetricsService();
