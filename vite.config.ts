import { defineConfig } from "vite";
import { resolve } from "node:path";

export default defineConfig({
  root: "ui",
  publicDir: resolve(__dirname, "ui/public"),
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    rollupOptions: {
      input: {
        sprite: resolve(__dirname, "ui/sprite/index.html"),
        settings: resolve(__dirname, "ui/settings/index.html"),
      },
    },
  },
});
