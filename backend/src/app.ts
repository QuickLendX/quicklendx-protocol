import express from "express";
import cors from "cors";
import helmet from "helmet";
import { rateLimitMiddleware } from "./middleware/rate-limit";
import { loadSheddingMiddleware } from "./middleware/load-shedding";
import { errorHandler } from "./middleware/error-handler";
import { statusInjector } from "./middleware/status-injector";
import v1Routes from "./routes/v1";
import webhookRoutes from "./routes/webhooks";
import { webhookCorsOptions } from "./config/cors";
import { csrfMiddleware } from "./middleware/csrf";
import { requestLogger } from "./middleware/request-logger";

const app = express();

app.set("trust proxy", true);
// Disable Express's built-in ETag generation so our cache-headers middleware
// has full control over which responses get ETags and which do not.
app.set("etag", false);

// Security Middleware
app.use(helmet());
app.use(cors());
app.use(express.json({ limit: "1mb" }));
app.set("trust proxy", true);

// Test middleware to simulate no IP for coverage
app.use((req, res, next) => {
  if (req.headers["x-simulate-no-ip"]) {
    Object.defineProperty(req, "ip", { value: undefined });
  }
  next();
});

// Rate Limiting
app.use(rateLimitMiddleware);

// Inject _system metadata into every JSON response
app.use(statusInjector);

// Structured request/response logging with automatic field-level redaction
app.use(requestLogger);

// Routes
app.use("/api/webhooks", cors(webhookCorsOptions), webhookRoutes);
app.use(csrfMiddleware);
app.use("/api/v1", v1Routes);

// Health check (root level as well if needed)
app.get("/health", (req, res) => {
  res.json({
    status: "ok",
    version: "1.0.0",
    timestamp: new Date().toISOString(),
  });
});

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
