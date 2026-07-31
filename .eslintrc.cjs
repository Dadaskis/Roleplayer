/* eslint-env node */
module.exports = {
  root: true,
  parser: "@typescript-eslint/parser",
  plugins: ["@typescript-eslint", "react-hooks"],
  extends: [
    "eslint:recommended",
    "plugin:@typescript-eslint/recommended",
  ],
  ignorePatterns: ["dist", "node_modules", "src-tauri"],
  parserOptions: {
    ecmaVersion: "latest",
    sourceType: "module",
    ecmaFeatures: { jsx: true },
  },
  settings: {
    react: { version: "detect" },
  },
  rules: {
    // This is the actual enforcement of module boundaries on the TS side:
    // a module may import its own internals and the shared `core`, but never
    // another module's internal files. Only the public `index.ts` surface is
    // allowed cross-module.
    "no-restricted-imports": [
      "error",
      {
        patterns: [
          {
            group: ["../*/components/*", "../*/screens/*", "../*/store/*", "../*/api/*", "../*/types/*"],
            message:
              "Cross-module imports may only use the module's public index.ts surface (AGENTS.md 5.6).",
          },
        ],
      },
    ],
    "@typescript-eslint/no-explicit-any": "error",
    "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
    "react-hooks/rules-of-hooks": "error",
    "react-hooks/exhaustive-deps": "warn",
  },
}
