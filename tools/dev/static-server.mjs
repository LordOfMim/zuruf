// Minimal static file server for manual checks and browser tests:
//   node static-server.mjs <directory> <port>
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import { extname, join, posix } from "node:path";

const [root, port] = process.argv.slice(2);
const types = {
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript",
  ".css": "text/css",
  ".svg": "image/svg+xml",
  ".png": "image/png",
  ".json": "application/json",
};

createServer(async (request, response) => {
  // Normalising first keeps ".." from leaving the served directory.
  const requested = posix.normalize(new URL(request.url, "http://x").pathname);
  const path = requested === "/" ? "index.html" : requested.slice(1);
  try {
    const body = await readFile(join(root, path));
    response.writeHead(200, {
      "content-type": types[extname(path)] ?? "text/html; charset=utf-8",
    });
    response.end(body);
  } catch {
    response.writeHead(404).end("not found");
  }
}).listen(Number(port), "127.0.0.1");
