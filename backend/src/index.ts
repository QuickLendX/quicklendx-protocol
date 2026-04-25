import express from "express";
import cors from "cors";
import dotenv from "dotenv";
import { statusService } from "./services/statusService";
import { browserCorsOptions } from "./config/cors";
import { csrfMiddleware } from "./middleware/csrf";

dotenv.config();

const app = express();
const port = process.env.PORT || 3001;

app.use(cors(browserCorsOptions));
app.use(express.json());

/**
 * @openapi
 * /api/status:
 *   get:
 *     summary: Get system status
 *     description: Reports maintenance, degraded mode, and index lag.
 *     responses:
 *       200:
 *         description: OK
 *         content:
 *           application/json:
 *             schema:
 *               $ref: '#/components/schemas/Status'
 */
app.get("/api/status", async (req, res) => {
  try {
    const status = await statusService.getStatus();
    
    // Cache safely: 30 seconds max age
    res.setHeader("Cache-Control", "public, max-age=30");
    res.json(status);
  } catch (error) {
    console.error("Status check failed:", error);
    res.status(500).json({ error: "Internal server error" });
  }
});

// Admin-only (internal/secured) endpoint to toggle maintenance mode
// In a real app, this would be protected by API key or Auth
app.post("/api/admin/maintenance", csrfMiddleware, (req, res) => {
  const { enabled } = req.body;
  if (typeof enabled !== "boolean") {
    return res.status(400).json({ error: "Invalid enabled flag" });
  }

  statusService.setMaintenanceMode(enabled);
  res.json({ success: true, maintenance: enabled });
});

if (require.main === module) {
  app.listen(port, () => {
    console.log(`Backend server running at http://localhost:${port}`);
  });
}

export default app;
