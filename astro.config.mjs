// @ts-check
import { defineConfig } from "astro/config";

import tailwindcss from "@tailwindcss/vite";

// https://astro.build/config
export default defineConfig({
  site: "https://maniprojs.github.io",
  base: "/luma",
  vite: {
    plugins: [tailwindcss()],
  },
});
