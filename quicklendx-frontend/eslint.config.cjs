const { FlatCompat } = require("@eslint/eslintrc");
const path = require("path");

const compat = new FlatCompat({
  baseDirectory: __dirname,
});

/** @type {import("eslint").Linter.Config[]} */
const config = [
  ...compat.extends("next/core-web-vitals"),
  {
    files: ["app/components/**/*.{js,jsx,ts,tsx}", "app/lib/api-client.ts"],
    rules: {
      "no-restricted-imports": [
        "error",
        {
          paths: [
            {
              name: "crypto",
              message: "Use the browser-safe app/lib/id helper.",
            },
            {
              name: "fs",
              message: "Node built-ins are not available in client code.",
            },
            {
              name: "path",
              message: "Node built-ins are not available in client code.",
            },
          ],
          patterns: [
            {
              group: ["node:*"],
              message: "Node built-ins are not available in client code.",
            },
          ],
        },
      ],
    },
  },
];

module.exports = config;
