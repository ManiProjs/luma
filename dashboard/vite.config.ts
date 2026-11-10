import { defineConfig } from "vite";
import tailwindcss from "@tailwindcss/vite";

const coreUrl = process.env.LUMA_CORE_URL ?? "http://127.0.0.1:3000";
const websocketUrl = coreUrl.replace(/^http/, "ws");

export default defineConfig({
  plugins: [tailwindcss()],
  base: "/",
  esbuild: { jsx: "automatic" },
  server: {
    proxy: {
      "/ws": { target: websocketUrl, ws: true },
      "/api": { target: coreUrl },
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    assetsDir: "assets",
    cssCodeSplit: false,
    assetsInlineLimit: 1024 * 1024,
    rollupOptions: {
      output: {
        entryFileNames: "assets/app.js",
        chunkFileNames: "assets/[name].js",
        assetFileNames: (asset) => asset.name?.endsWith(".css")
          ? "assets/style.css"
          : "assets/[name][extname]",
      },
      onwarn(warning, defaultHandler) {
        if (warning.code === "MODULE_LEVEL_DIRECTIVE" && warning.message.includes("use client")) return;
        defaultHandler(warning);
      },
    },
  },
});
