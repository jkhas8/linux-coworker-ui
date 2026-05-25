import { defineConfig } from "vitest/config";
import solid from "vite-plugin-solid";

export default defineConfig({
  plugins: [solid()],
  test: {
    environment: "jsdom",
    globals: true,
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    coverage: {
      provider: "v8",
      reporter: ["text", "lcov"],
      reportsDirectory: "./coverage",
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/**/*.test.{ts,tsx}",
        "src/vite-env.d.ts",
        "src/index.tsx",
        // App shell: layout + Tauri-event wiring + signal orchestration.
        // Same category as src-tauri/src/lib.rs — needs an integration
        // harness, not unit tests. Components rendered by App are tested
        // individually under src/components/.
        "src/App.tsx",
        // Pure `invoke<T>()` wrappers — testing them = testing Tauri.
        "src/workspaces.ts",
      ],
    },
  },
});
