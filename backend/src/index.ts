import app from "./app";
import dotenv from "dotenv";
import { createShutdownHandler } from "./lib/shutdown";

dotenv.config();

const port = process.env.PORT ? Number(process.env.PORT) : 3001;

if (require.main === module) {
  const server = app.listen(port, () => {
    console.log(`Backend server running at http://localhost:${port}`);
  });

  const shutdown = createShutdownHandler(server);
  process.on("SIGTERM", () => shutdown("SIGTERM"));
  process.on("SIGINT", () => shutdown("SIGINT"));
}

export default app;
