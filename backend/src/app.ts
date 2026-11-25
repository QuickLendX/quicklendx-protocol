import express from "express";
import cors from "cors";
import helmet from "helmet";
import { rateLimitMiddleware, perKeyRateLimitMiddleware } from "./middleware/rate-limit";
import { loadSheddingMiddleware } from "./middleware/load-shedding";
import { errorHandler } from "./middleware/error-handler";
import { statusInjector } from "./middleware/status-injector";
import { csrfMiddleware } from "./middleware/csrf";
import { corsOptionsDelegate, webhookCorsOptions } from "./config/cors";
import { optionalApiKeyAuth } from "./middleware/api-key-auth";
import { config } from "./config";
import v1Routes from "./routes/v1";
import webhookRoutes from "./routes/webhooks";
import healthRoutes from "./routes/health";
import adminRoutes from "./routes/v1/admin";
import { requestLogger } from "./middleware/request-logger";
import { isEventIngestRequest } from "./middleware/event-ingest-limits";
import { lagMonitor } from "./services/lagMonitor";
import { alertRouter, Severity } from "./services/alertRouter";

// Initialize alert routing: subscribe to lagMonitor alerts
const unsubscribeLagAlerts = lagMonitor.onAlert((event) => {
  const severity =
    event.direction === "escalation"
      ? event.to === "critical"
        ? Severity.HIGH
        : Severity.MEDIUM
      : Severity.LOW;
  const message = `Lag ${event.direction}: ${event.from} → ${event.to} (lag: ${event.lag} ledgers)`;
  const alertKey = `lag-${event.to}`;

  alertRouter
    .routeAlert(alertKey, severity, message)
    .catch((err) => console.error("Failed to route lag alert:", err));
});

const app = express();

app.set("trust proxy", config.TRUST_PROXY);
// Disable Express's built-in ETag generation so our cache-headers middleware
// has full control over which responses get ETags and which do not.
app.set("etag", false);

// Extend Express Request to include rawBody
declare global {
  namespace Express {
    interface Request {
      rawBody?: Buffer;
    }
  }
}

// Security Middleware
app.use(helmet());
app.use(cors(corsOptionsDelegate));
const globalJsonParser = express.json({
  limit: "1mb",
  verify: (req: express.Request, res: express.Response, buf: Buffer) => {
    req.rawBody = buf;
  },
});

// The event ingest route enforces a stricter 256 KB budget with its own parser,
// so the 1 MB parser must not buffer that payload first.
app.use((req, res, next) => {
  if (isEventIngestRequest(req)) {
    next();
    return;
  }
  globalJsonParser(req, res, next);
});

// Rate Limiting (IP-based + per-API-key layered)
app.use(rateLimitMiddleware);
app.use(perKeyRateLimitMiddleware);

// Inject _system metadata into every JSON response
app.use(statusInjector);

// Structured request/response logging with automatic field-level redaction
app.use(requestLogger);

// Optional API key auth (populates req.apiKey if valid Bearer token present)
// Must run before CSRF so CSRF can check for validated API key
app.use(optionalApiKeyAuth);

// Routes
app.use("/api/webhooks", cors(webhookCorsOptions), webhookRoutes);
app.use(csrfMiddleware);
app.use("/api/v1", v1Routes);
app.use("/admin", adminRoutes);

// Liveness (/health, /livez) and readiness (/readyz) probes.
// Mounted at the root and left unauthenticated so orchestrators can probe them.
app.use(healthRoutes);

// 404 handler
app.use((req, res) => {
  res.status(404).json({
    error: {
      message: "Resource not found",
      code: "NOT_FOUND",
    },
  });
});

// Error handling
app.use(errorHandler);

export default app;
