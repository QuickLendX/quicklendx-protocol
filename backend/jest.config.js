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
    "src/services/webhook/**/*.ts",
    "!src/services/webhook/index.ts",
    "src/lib/migrations/**/*.ts",
    "!src/lib/migrations/cli.ts",
    "src/lib/database.ts",
    "src/lib/logging/policy.ts",
    "src/middleware/request-logger.ts",
  ],
  moduleNameMapper: {
    "^@/(.*)$": "<rootDir>/src/$1",
  },
  testPathIgnorePatterns: [
    "src/node_modules/",
    "node_modules/",
    "src/migrations/*",
  ],
};
