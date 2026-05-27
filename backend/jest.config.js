module.exports = {
  preset: "ts-jest",
  testEnvironment: "node",
  testMatch: ["**/*.test.ts"],
  coverageThreshold: {
    global: {
      branches: 50,
      functions: 50,
      lines: 50,
      statements: 50,
    },
  },
  collectCoverageFrom: [
    "src/db/**/*.ts",
    "src/services/webhook/**/*.ts",
    "!src/services/webhook/index.ts",
    "src/lib/migrations/**/*.ts",
    "!src/lib/migrations/cli.ts",
    "src/lib/database.ts",
    "src/lib/logging/policy.ts",
    "src/middleware/request-logger.ts",
    "src/tests/spec-loader.ts",
    "src/tests/openapi-contract.test.ts",
  ],
  moduleNameMapper: {
    "^@/(.*)$": "<rootDir>/src/$1",
    // Mock pg (not installed) so contract tests can import app.ts cleanly.
    "^pg$": "<rootDir>/src/__mocks__/pg.ts",
  },
  testPathIgnorePatterns: [
    "src/node_modules/",
    "node_modules/",
    "src/migrations/*",
  ],
};
