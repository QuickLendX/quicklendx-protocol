import app from "./app";
import dotenv from "dotenv";

dotenv.config();

const port = process.env.PORT ? Number(process.env.PORT) : 3001;

if (require.main === module) {
  app.listen(port, () => {
    console.log(`Backend server running at http://localhost:${port}`);
  });
}

export default app;
