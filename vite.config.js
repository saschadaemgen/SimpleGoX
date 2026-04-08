import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

export default defineConfig({
    root: "src",
    plugins: [svelte()],
    server: {
        port: 1420,
        strictPort: true,
    },
    build: {
        outDir: "../dist",
    },
});
