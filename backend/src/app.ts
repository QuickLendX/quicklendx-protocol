import express from "express";
import cors from "cors";
import helmet from "helmet";
import { rateLimitMiddleware } from "./middleware/rate-limit";
import { errorHandler } from "./middleware/error-handler";
import { browserCorsOptions, webhookCorsOptions } from "./config/cors";
import { csrfMiddleware } from "./middleware/csrf";
import v1Routes from "./routes/v1";
import webhookRoutes from "./routes/webhooks";

const app = express();

app.set("trust proxy", true);

// Security Middleware
app.use(helmet());
app.use(cors(browserCorsOptions));
app.use(express.json());
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
