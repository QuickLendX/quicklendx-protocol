import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    globals: true,
    environment: "node",
    coverage: {
      provider: "v8",
      reporter: ["text", "text-summary", "lcov"],
      include: ["src/**/*.ts"],
      exclude: [
        "src/**/__tests__/**",
        "src/**/__fakes__/**",
        "**/*.test.ts",
        "src/models/**", // type-only module (interfaces/types, no runtime)
      ],
    },
    include: ["src/**/*.test.ts", "src/**/*.spec.ts"],
    setupFiles: [],
  },
});
