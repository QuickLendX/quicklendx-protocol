module.exports = {
  preset: "ts-jest",
  testEnvironment: "node",
  testMatch: ["**/*.test.ts"],
  coverageThreshold: {
    global: {
      branches: 70,
      functions: 90,
      lines: 90,
      statements: 90,
    },
  },
  collectCoverageFrom: [
    "src/**/*.ts",
    "!src/index.ts",
    "!src/controllers/v1/disputes.ts",
    "!src/routes/v1/test-errors.ts",
  ],
};
