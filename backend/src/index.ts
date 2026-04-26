import app from "./app";
import dotenv from "dotenv";
import { getDatabase } from "./lib/database";
import { runMigrations } from "./lib/migrations/runner";
import { ensureRuntimeDirs } from "./lib/bootstrap";

dotenv.config();

const PORT = process.env.PORT || 3001;

async function boot(): Promise<void> {
  console.log("🚀 QuickLendX Backend starting...");

  // 0. Ensure runtime directories exist (.data for SQLite, .hotfix-approvals)
  await ensureRuntimeDirs();

  // 1. Database + Migrations
  try {
    const db = getDatabase();
    console.log("   ✅ Database connected");

    // Run migrations (forward-only by default)
    // In production, failures exit the process. In dev, we warn but continue.
    try {
      const result = await runMigrations({ verbose: true });
      console.log(`   ✅ Migrations: ${result.applied.length} applied, ${result.skipped} skipped`);
    } catch (migErr: any) {
      console.error("❌ Migration error:", migErr.message);
      if (process.env.NODE_ENV === "production") {
        console.error("🛑 Refusing to start with failed migrations in production");
        process.exit(1);
      } else {
        console.warn("⚠️  Continuing in dev mode despite migration failure");
      }
    }
  } catch (dbErr: any) {
    console.error("❌ Database connection failed:", dbErr.message);
    if (process.env.NODE_ENV === "production") {
      process.exit(1);
    }
    console.warn("⚠️  Continuing in dev mode without database");
  }

  // 2. Start HTTP server
  app.listen(PORT, () => {
    console.log(`   ✅ HTTP listening on http://localhost:${PORT}`);
    console.log(`   🔍 Health: /health`);
  });
}

if (require.main === module) {
  boot().catch((err: any) => {
    console.error("Fatal startup error:", err.message);
    process.exit(1);
  });
}

export { app };
