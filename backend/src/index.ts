import express from "express";
import cors from "cors";
import dotenv from "dotenv";
import { statusService } from "./services/statusService";
import { rateLimitMiddleware } from "./middleware/rate-limit";
import { requestLimitsMiddleware } from "./middleware/request-limits";
import helmet from "helmet";
import { errorHandler } from "./middleware/error-handler";
import v1Routes from "./routes/v1";

dotenv.config();

const app = express();
const port = process.env.PORT || 3001;

app.set("trust proxy", true);
app.use(helmet());
app.use(cors());
app.use(express.json({ limit: "1mb" }));
app.use(rateLimitMiddleware);
app.use(requestLimitsMiddleware);

app.get("/api/status", async (req, res) => {
  try {
    const status = await statusService.getStatus();
    res.setHeader("Cache-Control", "public, max-age=30");
    res.json(status);
  } catch (error) {
    console.error("Status check failed:", error);
    res.status(500).json({ error: "Internal server error" });
  }
});

app.post("/api/admin/maintenance", (req, res) => {
  const { enabled } = req.body;
  if (typeof enabled !== "boolean") {
    return res.status(400).json({ error: "Invalid enabled flag" });
  }
  statusService.setMaintenanceMode(enabled);
  res.json({ success: true, maintenance: enabled });
});

app.use("/api/v1", v1Routes);

app.get("/health", (req, res) => {
  res.json({ status: "ok", version: "1.0.0", timestamp: new Date().toISOString() });
});

app.use((req, res) => {
  res.status(404).json({ error: { message: "Resource not found", code: "NOT_FOUND" } });
});

app.use(errorHandler);

if (require.main === module) {
  app.listen(port, () => {
    console.log(`Backend server running at http://localhost:${port}`);
  });
}

export default app;