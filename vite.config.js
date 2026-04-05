import { defineConfig } from "vite";

export default defineConfig({
  root: "src",
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "../dist",
  },
});