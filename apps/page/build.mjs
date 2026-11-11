import { build } from "esbuild";
import { mkdir, rm } from "node:fs/promises";

// One self-contained script. The app (crates/zuruf-browser) embeds it and injects it into an
// isolated world of every frame; nothing is loaded from disk or the network at run time.
await rm("dist", { recursive: true, force: true });
await mkdir("dist", { recursive: true });
await build({
  entryPoints: ["src/agent.ts"],
  outfile: "dist/agent.js",
  bundle: true,
  format: "iife",
  target: "chrome120",
  minify: true,
  loader: { ".css": "text" },
  // Injecting twice into the same world must not register a second set of listeners.
  banner: {
    js: "if(!globalThis.__zurufAgent){globalThis.__zurufAgent=true;",
  },
  footer: { js: "}" },
});
