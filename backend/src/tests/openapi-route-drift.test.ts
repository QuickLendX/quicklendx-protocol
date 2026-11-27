jest.mock("better-sqlite3", () => {
  const statement = {
    run: () => ({ changes: 0, lastInsertRowid: 0 }),
    get: () => undefined,
    all: () => [],
  };

  class MockDatabase {
    pragma() {}
    exec() {}
    prepare() {
      return statement;
    }
    close() {}
  }

  return { __esModule: true, default: MockDatabase };
});

import v1Routes from "../routes/v1";
import healthRoutes from "../routes/health";
import webhookRoutes from "../routes/webhooks";
import adminRoutes from "../routes/v1/admin";
import apiKeyRoutes from "../routes/v1/api-keys";
import invoiceRoutes from "../routes/v1/invoices";
import bidRoutes from "../routes/v1/bids";
import settlementRoutes from "../routes/v1/settlements";
import portfolioRoutes from "../routes/v1/portfolio";
import testErrorRoutes from "../routes/v1/test-errors";
import notificationRoutes from "../routes/v1/notifications";
import exportRoutes from "../routes/v1/exports";
import monitoringRoutes from "../routes/v1/monitoring";
import reconciliationRoutes from "../routes/v1/reconciliation";
import v1WebhookRoutes from "../routes/v1/webhooks";
import { loadSpec } from "./spec-loader";

type ExpressLayer = {
  route?: {
    path: string | string[];
    methods: Record<string, boolean>;
  };
  name?: string;
  regexp?: RegExp;
  handle?: { stack?: ExpressLayer[] };
};

const HTTP_METHODS = ["get", "post", "put", "patch", "delete", "options", "head"];

const ROUTE_ALLOWLIST: Record<string, string> = {
  "GET /test-errors/500": "Test-only fault injection endpoint.",
  "GET /test-errors/no-message": "Test-only fault injection endpoint.",
  "GET /test-errors/development": "Test-only fault injection endpoint.",
  "GET /test-errors/default-error": "Test-only fault injection endpoint.",
  "POST /write-action": "Internal degradation-demo endpoint.",
  "POST /critical-action": "Internal degradation-demo endpoint.",
  "GET /livez": "Kubernetes liveness alias; operational probe.",
  "GET /readyz": "Kubernetes readiness probe; operational endpoint.",
  "GET /db-diagnostics": "Operational database diagnostics; intentionally omitted from the public API.",
  "GET /db-metrics": "Operational Prometheus metrics; intentionally omitted from the public API.",
  "POST /api/webhooks/callbacks": "External webhook ingress route, outside the versioned public API.",
};

function normalizePath(path: string): string {
  let normalized = path
    .replace(/:([A-Za-z0-Z_][A-Za-z0-9_]*)/g, "{$1}")
    .replace(/\{([A-Za-z0-Z_][A-Za-z0-9_]*)\}/g, "{$1}");
  normalized = normalized.replace(/\/+/g, "/");
  if (!normalized.startsWith("/")) normalized = `/${normalized}`;
  if (normalized !== "/") normalized = normalized.replace(/\/$/, "");
  return normalized;
}

function enumerateRoutes(stack: ExpressLayer[], prefix = ""): Set<string> {
  const routes = new Set<string>();
  for (const layer of stack) {
    if (layer.route) {
      const routePaths = Array.isArray(layer.route.path)
        ? layer.route.path
        : [layer.route.path];
      for (const routePath of routePaths) {
        for (const method of HTTP_METHODS) {
          if (layer.route.methods[method]) {
            routes.add(`${method.toUpperCase()} ${normalizePath(prefix + routePath)}`);
          }
        }
      }
    }
  }
  return routes;
}

describe("OpenAPI route drift", () => {
  it("documents every public route and every documented path has a live route", () => {
    const liveRoutes = new Set<string>();
    const mounts: Array<[unknown, string]> = [
      [v1Routes, "/api/v1"],
      [healthRoutes, ""],
      [webhookRoutes, "/api/webhooks"],
      [adminRoutes, "/admin"],
      [apiKeyRoutes, "/api/v1/api-keys"],
      [invoiceRoutes, "/api/v1/invoices"],
      [bidRoutes, "/api/v1/bids"],
      [settlementRoutes, "/api/v1/settlements"],
      [portfolioRoutes, "/api/v1/portfolio"],
      [testErrorRoutes, "/api/v1/test-errors"],
      [notificationRoutes, "/api/v1/notifications"],
      [exportRoutes, "/api/v1/exports"],
      [adminRoutes, "/api/v1/admin"],
      [monitoringRoutes, "/api/v1/admin/monitoring"],
      [reconciliationRoutes, "/api/v1/reconciliation"],
      [v1WebhookRoutes, "/api/v1/webhooks"],
    ];
    for (const [router, prefix] of mounts) {
      for (const route of enumerateRoutes(
        (router as { stack: ExpressLayer[] }).stack,
        prefix
      )) liveRoutes.add(route);
    }
    const spec = loadSpec();

    const normalizedLiveRoutes = new Set<string>();
    for (const liveRoute of liveRoutes) {
      const match = liveRoute.match(/^([A-Z]+) (.*)$/);
      if (!match) continue;
      const [, method, path] = match;
      let publicPath = path;
      if (publicPath.startsWith("/api/v1")) {
        publicPath = publicPath.slice("/api/v1".length) || "/";
      } else if (publicPath.startsWith("/api/")) {
        normalizedLiveRoutes.add(liveRoute);
        continue;
      } else if (publicPath.startsWith("/admin")) {
        normalizedLiveRoutes.add(liveRoute);
        continue;
      }
      normalizedLiveRoutes.add(`${method} ${normalizePath(publicPath)}`);
    }

    const undocumented = [...normalizedLiveRoutes]
      .filter((route) => {
        if (ROUTE_ALLOWLIST[route]) return false;
        const [method, path] = route.split(" ", 2);
        return !spec.paths[path]?.[method.toLowerCase()];
      })
      .sort();

    const undocumentedSpecPaths = Object.keys(spec.paths)
      .filter((path) => {
        if (path === "/fees") return false;
        const operations = spec.paths[path];
        return !Object.keys(operations).some((method) =>
          normalizedLiveRoutes.has(`${method.toUpperCase()} ${normalizePath(path)}`)
        );
      })
      .sort();

    expect(undocumented).toEqual([]);
    expect(undocumentedSpecPaths).toEqual([]);
  });
});
