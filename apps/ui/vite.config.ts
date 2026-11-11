import { defineConfig } from "vite";
import solid from "vite-plugin-solid";

// Two entry points from one codebase: the app window and the panel docked in a session window.
// Both are served by the daemon; the relative base keeps them independent of the mount path.
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
