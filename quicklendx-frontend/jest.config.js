/**
 * Jest configuration.
 *
 * Two projects live side by side:
 *
 *  - `node`  — the existing suites under `app/lib/__tests__` and `tests`.
 *              Untouched: same preset, environment, test matchers and module
 *              mapper as before.
 *  - `jsdom` — the new component tests (`.test.tsx`). They need a DOM,
 *              React's client renderer and Testing Library, which the node
 *              environment cannot provide.
 */

/** @type {import('jest').Config} */
module.exports = {
  projects: [
    {
      displayName: "node",
      preset: "ts-jest",
      testEnvironment: "node",
      testMatch: ["**/__tests__/**/*.test.ts", "**/tests/**/*.test.ts"],
      moduleNameMapper: {
        "^@/(.*)$": "<rootDir>/$1",
      },
    },
    {
      displayName: "jsdom",
      preset: "ts-jest",
      testEnvironment: "jsdom",
      testMatch: ["**/*.test.tsx"],
      moduleNameMapper: {
        "^@/(.*)$": "<rootDir>/$1",
      },
      setupFilesAfterEnv: ["<rootDir>/jest.setup.ts"],
      transform: {
        "^.+\\.tsx?$": [
          "ts-jest",
          {
            tsconfig: "<rootDir>/tsconfig.test.json",
          },
        ],
      },
    },
  ],
};
