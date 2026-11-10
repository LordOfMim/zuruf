import { build } from "esbuild";
import { cp, mkdir } from "node:fs/promises";
await mkdir("dist", { recursive: true });
await cp("static", "dist", { recursive: true });
await build({
  entryPoints: ["src/worker.ts", "src/content.ts", "src/panel.ts"],
  outdir: "dist",
  bundle: true,
  format: "iife",
  target: "chrome120",
  minify: true,
});
