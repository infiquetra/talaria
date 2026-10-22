import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // docs/ holds reference slices with their own runners (bun test, cargo test).
    exclude: ["node_modules/**", "dist/**", ".git/**", "docs/**"],
  },
});
