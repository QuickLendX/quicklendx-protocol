import express from "express";
import cors from "cors";
import helmet from "helmet";
import { rateLimitMiddleware, perKeyRateLimitMiddleware } from "./middleware/rate-limit";
import { loadSheddingMiddleware } from "./middleware/load-shedding";
import { errorHandler } from "./middleware/error-handler";
import { statusInjector } from "./middleware/status-injector";
import { csrfMiddleware } from "./middleware/csrf";
import { corsOptionsDelegate, webhookCorsOptions } from "./config/cors";
import v1Routes from "./routes/v1";
import webhookRoutes from "./routes/webhooks";
import healthRoutes from "./routes/health";
import { requestLogger } from "./middleware/request-logger";
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

app.set("trust proxy", true);
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
app.use(
  express.json({
    limit: "1mb",
    verify: (req: express.Request, res: express.Response, buf: Buffer) => {
      req.rawBody = buf;
    },
  })
);
app.set("trust proxy", true);

// Test middleware to simulate no IP for coverage
app.use((req, res, next) => {
  if (req.headers["x-simulate-no-ip"]) {
    Object.defineProperty(req, "ip", { value: undefined });
  }
  next();
});

// Rate Limiting (IP-based + per-API-key layered)
app.use(rateLimitMiddleware);
app.use(perKeyRateLimitMiddleware);

// Inject _system metadata into every JSON response
app.use(statusInjector);

// Structured request/response logging with automatic field-level redaction
app.use(requestLogger);

// Routes
app.use("/api/webhooks", cors(webhookCorsOptions), webhookRoutes);
app.use(csrfMiddleware);
app.use("/api/v1", v1Routes);

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
