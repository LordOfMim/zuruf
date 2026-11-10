import { build } from "esbuild";
import { createHash } from "node:crypto";
import { cp, mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
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

// Chromium keeps running a profile's cached service worker for the same script URL, even when
// the files on disk changed. Name the worker after the build so a session browser always runs
// this build's worker, and never an older one next to newer content scripts.
const hash = createHash("sha256");
for (const file of ["dist/worker.js", "dist/content.js"])
  hash.update(await readFile(file));
const digest = hash.digest();
const worker = `worker-${digest.toString("hex").slice(0, 12)}.js`;
await rename("dist/worker.js", `dist/${worker}`);
const manifest = JSON.parse(await readFile("dist/manifest.json", "utf8"));
manifest.version = `0.1.${digest.readUInt16BE(0)}.${digest.readUInt16BE(2)}`;
manifest.background.service_worker = worker;
await writeFile("dist/manifest.json", JSON.stringify(manifest, null, 2) + "\n");
