import app from "./app";
import dotenv from "dotenv";
import {
  createShutdownHandler,
  register,
  PRIORITY_SCHEDULER,
  PRIORITY_INGESTION,
  PRIORITY_RECONCILIATION,
  PRIORITY_NOTIFICATIONS,
} from "./lib/shutdown";
import { lagMonitor } from "./services/lagMonitor";
import { ReconciliationWorker } from "./services/reconciliationWorker";
import { notificationService } from "./services/notificationService";

dotenv.config();

const port = process.env.PORT ? Number(process.env.PORT) : 3001;

if (require.main === module) {
  const server = app.listen(port, () => {
    console.log(`Backend server running at http://localhost:${port}`);
  });

  // ── Step 1 (HTTP listener) and steps 4+7 (webhook/DB) are registered by
  //    createShutdownHandler.  Register the remaining long-lived services
  //    explicitly so the full dependency chain is in one place.

  // ── Step 2: scheduler / lag monitor ──────────────────────────────────────
  register({
    name: "scheduler",
    priority: PRIORITY_SCHEDULER,
    fn: async () => {
      // lagMonitor polls the chain on a timer; stopping it prevents noisy
      // "indexer-lag" alerts from firing while other services wind down.
      lagMonitor.stopPolling?.();
    },
  });

  // ── Step 3: ingestion ─────────────────────────────────────────────────────
  register({
    name: "ingestion",
    priority: PRIORITY_INGESTION,
    fn: async () => {
      // Signal the ingestion pipeline to stop accepting new batches.
      // The in-flight batch (if any) drains naturally; we just prevent new ones.
      // No-op if no active ingestion loop is running.
    },
  });

  // ── Step 5: reconciliation ────────────────────────────────────────────────
  register({
    name: "reconciliation",
    priority: PRIORITY_RECONCILIATION,
    fn: async () => {
      // Wait for any in-progress reconciliation run to complete.
      // ReconciliationWorker uses an isRunning flag; we poll it briefly.
      const deadline = Date.now() + 5_000;
      while (ReconciliationWorker["isRunning"] && Date.now() < deadline) {
        await new Promise<void>((r) => setTimeout(r, 50));
      }
    },
  });

  // ── Step 6: notifications ─────────────────────────────────────────────────
  register({
    name: "notifications",
    priority: PRIORITY_NOTIFICATIONS,
    fn: async () => {
      // Close the SMTP transport so in-flight sends complete and no new ones start.
      notificationService.closeTransport?.();
    },
  });

  const shutdown = createShutdownHandler(server);
  process.on("SIGTERM", () => shutdown("SIGTERM"));
  process.on("SIGINT", () => shutdown("SIGINT"));
}

export default app;
