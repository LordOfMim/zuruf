import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Two entry points from one codebase: the app window and the side panel.
// Relative base so the panel bundle also works from inside the extension.
export default defineConfig({
  base: "./",
  plugins: [solid()],
  build: {
    target: "es2022",
    rollupOptions: {
      input: {
        index: "index.html",
        panel: "panel.html",
      },
    },
  },
  server: { host: "127.0.0.1", port: 47610, strictPort: true },
});
