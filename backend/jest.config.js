module.exports = {
  preset: "ts-jest",
  testEnvironment: "node",
  testMatch: ["**/*.test.ts"],
  coverageThreshold: {
    global: {
      branches: 82,
      functions: 82,
      lines: 88,
      statements: 87,
    },
  },
  collectCoverageFrom: [
    "src/services/webhook/**/*.ts",
    "!src/services/webhook/index.ts",
  ],
};
