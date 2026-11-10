import { build } from "esbuild";
import { cp, mkdir, rm } from "node:fs/promises";
await rm("dist", { recursive: true, force: true });
await mkdir("dist", { recursive: true });
await cp("static", "dist", { recursive: true });
await build({
  entryPoints: ["src/worker.ts", "src/content.ts"],
  outdir: "dist",
  bundle: true,
  format: "iife",
  target: "chrome120",
  minify: true,
});

// The panel is the same compiled Solid application as the app window.
await mkdir("dist/panel", { recursive: true });
await cp("../ui/dist/panel.html", "dist/panel/panel.html");
await cp("../ui/dist/assets", "dist/panel/assets", { recursive: true });
