import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import tsconfigPaths from "vite-tsconfig-paths";

// Vite picks up `dev` / `build` from the package.json scripts. Tauri runs
// `pnpm dev` (devUrl = http://localhost:5173) before the desktop window
// opens.

export default defineConfig({
  plugins: [react(), tailwindcss(), tsconfigPaths()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: "127.0.0.1",
    // Tauri expects the dev server to stay on a fixed port.
    hmr: {
      protocol: "ws",
      host: "127.0.0.1",
      port: 5174,
    },
  },
  build: {
    target: "esnext",
    sourcemap: true,
    outDir: "dist",
    emptyOutDir: true,
    chunkSizeWarningLimit: 1200,
  },
  // Vitest configuration. Kept inline so we don't ship a second config file.
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["src/setup-tests.ts"],
    css: false,
    include: ["src/**/*.test.{ts,tsx}"],
    exclude: ["node_modules", "dist", "src-tauri", "target"],
  },
});
