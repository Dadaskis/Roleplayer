import { defineConfig, type Config } from "vitest/config"

export default defineConfig({
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["src-web/core/test/setup.ts"],
    include: ["src-web/**/*.test.{ts,tsx}"],
  },
} satisfies Config)
