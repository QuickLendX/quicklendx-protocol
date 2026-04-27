module.exports = {
  preset: "ts-jest",
  testEnvironment: "node",
  testMatch: ["**/*.test.ts"],
  coverageThreshold: {
    global: {
      branches: 95,
      functions: 95,
      lines: 95,
      statements: 95,
    },
  },
  collectCoverageFrom: [
    "src/services/webhook/**/*.ts",
    "!src/services/webhook/index.ts",
    "src/lib/migrations/**/*.ts",
    "!src/lib/migrations/cli.ts",
    "src/lib/database.ts",
    "src/middleware/rate-limit.ts",
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
