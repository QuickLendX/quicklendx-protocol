import fs from "fs";
import path from "path";
import { configSchema } from "../config";

function parseEnvExampleKeys(content: string): Set<string> {
  const keys = new Set<string>();
  for (const line of content.split(/\r?\n/)) {
    const match = line.match(/^\s*#?\s*([A-Z][A-Z0-9_]*)=/);
    if (match) {
      keys.add(match[1]);
    }
  }
  return keys;
}

describe(".env.example coverage", () => {
  const envExamplePath = path.resolve(__dirname, "../../.env.example");
  const envExampleContent = fs.readFileSync(envExamplePath, "utf8");
  const documentedKeys = parseEnvExampleKeys(envExampleContent);

  it("documents every environment variable defined in the config.ts zod schema", () => {
    const schemaKeys = Object.keys(configSchema.shape);
    const missingKeys = schemaKeys.filter((key) => !documentedKeys.has(key));

    expect(missingKeys).toEqual([]);
  });

  it("documents ALLOWED_ORIGINS used by config/cors.ts", () => {
    expect(documentedKeys.has("ALLOWED_ORIGINS")).toBe(true);
  });
});
