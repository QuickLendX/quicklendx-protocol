module.exports = {
  preset: "ts-jest",
  testEnvironment: "node",
  testMatch: ["**/*.test.ts"],
  coverageThreshold: {
    global: {
      branches: 55,
      functions: 60,
      lines: 58,
      statements: 58,
    },
  },
  collectCoverageFrom: [
    "src/lib/migrations/**/*.ts",
    "!src/lib/migrations/cli.ts",
    "src/lib/database.ts",
    "src/lib/logging/policy.ts",
    "src/lib/requestContext.ts",
    "src/middleware/request-logger.ts",
    "src/middleware/access-log.ts",
    "src/services/eventProcessor.ts",
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
