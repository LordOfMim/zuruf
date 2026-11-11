// Real packaged daemon + real Chromium, using only temporary state and local fixtures.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "../../apps/page/node_modules/playwright-core/index.mjs";

const prefixArgument = process.argv.indexOf("--prefix");
const prefix = resolve(prefixArgument >= 0 ? process.argv[prefixArgument + 1] : `.release-stage/${process.platform === "win32" ? "windows" : "linux"}-x64`);
const directory = await mkdtemp(join(tmpdir(), "zuruf release smoke "));
const listener = createServer();
await new Promise((done) => listener.listen(0, "127.0.0.1", done));
const port = listener.address().port;
await new Promise((done) => listener.close(done));
const base = `http://127.0.0.1:${port}`;
const env = { ...process.env, ZURUF_PORT: String(port), ZURUF_DATA_DIR: join(directory, "data"), ZURUF_STATE_DIR: join(directory, "state"), ZURUF_CONFIG_DIR: join(directory, "config") };
delete env.ZURUF_FAKE_ADAPTERS;
delete env.ZURUF_SHELL;
delete env.T3_URL;
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
async function until(name, probe, timeout = 25000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    const result = await probe();
    if (result) return result;
    await sleep(75);
  }
  throw new Error(`timed out waiting for ${name}`);
}
let token;
let daemon;
let browser;
let log = "";
const viaLauncher = process.platform === "win32";
async function api(method, path, body) {
  const response = await fetch(`${base}/api/v1${path}`, {
    method, headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body), signal: AbortSignal.timeout(25000),
  });
  const result = await response.json();
  assert(response.ok, `${method} ${path}: ${response.status}`);
  return result;
}
const fixture = createServer((request, response) => {
  response.writeHead(200, { "content-type": "text/html; charset=utf-8" }).end('<!doctype html><title>Zuruf Release Smoke</title><h1>Packaged browser</h1><button onclick="this.textContent=\'Clicked\'">Click me</button>');
});
await new Promise((done) => fixture.listen(0, "127.0.0.1", done));
const url = `http://127.0.0.1:${fixture.address().port}/`;
try {
  daemon = spawn(viaLauncher ? join(prefix, "Zuruf.exe") : join(prefix, "bin/zuruf"), viaLauncher ? [] : ["daemon"], { env, windowsHide: true, stdio: ["ignore", "ignore", "pipe"] });
  daemon.stderr.on("data", (chunk) => (log += chunk));
  daemon.on("error", (error) => (log += error.message));
  await until("the packaged daemon", async () => {
    if ((!viaLauncher && daemon.exitCode !== null) || daemon.signalCode !== null || daemon.exitCode > 0) throw new Error("packaged program exited");
    try {
      token = (await readFile(join(env.ZURUF_STATE_DIR, "token"), "utf8")).trim();
      return (await api("GET", "/state")).projects.length === 0;
    } catch { return false; }
  });
  if (viaLauncher) await until("the GUI launcher to finish opening the app", () => daemon.exitCode === 0);
  const project = await api("POST", "/projects", { name: "Release fixture", repositoryPath: directory, baseUrl: url });
  const session = await api("POST", "/sessions", { projectId: project.id, name: "Release smoke", mode: "shared", agentAccess: true });
  const current = async () => (await api("GET", "/state")).sessions.find((entry) => entry.id === session.id);
  const connect = async () => {
    const access = await api("POST", `/sessions/${session.id}/agent-access`, { agentId: "release-smoke" });
    await api("POST", `/sessions/${session.id}/control`, { action: "grant", agentId: "release-smoke", leaseSeconds: 120 });
    browser = await chromium.connectOverCDP(`ws://127.0.0.1:${port}/cdp/${session.id}`, { headers: { Authorization: `Bearer ${access.token}` } });
    return until("the scriptable fixture", () => browser.contexts()[0].pages().find((page) => page.url() === url));
  };
  assert.equal((await api("POST", `/sessions/${session.id}/start`, {})).state, "running");
  await until("the native session", async () => (await current()).tabs.some((tab) => tab.scriptable && tab.url === url));
  let page = await connect();
  await page.getByRole("button", { name: "Click me", exact: true }).click();
  await page.getByRole("button", { name: "Clicked", exact: true }).waitFor();
  await page.evaluate(() => localStorage.setItem("release-check", "persisted"));
  assert((await page.screenshot()).length > 1000);
  assert(browser.contexts()[0].pages().every((page) => !page.url().startsWith(base)), "the panel must remain hidden from the agent");
  await api("POST", `/sessions/${session.id}/page-width`, { width: 390 });
  await until("the real page viewport", async () => await page.evaluate(() => innerWidth) === 390);
  await browser.close(); browser = undefined;
  assert.equal((await api("POST", `/sessions/${session.id}/restart`, {})).state, "running");
  await until("the restored fixture", async () => (await current()).tabs.some((tab) => tab.scriptable && tab.url === url));
  page = await connect();
  assert.equal(await page.evaluate(() => localStorage.getItem("release-check")), "persisted");
  await until("the restored width", async () => await page.evaluate(() => innerWidth) === 390);
  await browser.close(); browser = undefined;
  assert.equal((await api("POST", `/sessions/${session.id}/stop`, {})).state, "stopped");
  await sleep(600);
  assert.equal((await current()).state, "stopped");
  // Updates use this CLI while windows are still open. It must wait for browser cleanup.
  await api("POST", `/sessions/${session.id}/start`, {});
  const stop = spawn(join(prefix, `bin/zuruf${process.platform === "win32" ? ".exe" : ""}`), ["stop"], { env, windowsHide: true, stdio: "ignore" });
  const stopped = new Promise((done, reject) => {
    stop.once("error", reject);
    stop.once("exit", (code) => code === 0 ? done() : reject(new Error(`CLI stop exited ${code}`)));
  });
  await stopped;
  await assert.rejects(fetch(base, { signal: AbortSignal.timeout(1000) }), "CLI stop must wait for shutdown");
  console.log("PASS packaged native browser: start, click, screenshot, filtered CDP, width, profile recovery, deliberate stop and update shutdown");
} catch (error) {
  console.error(log.split("\n").slice(-12).join("\n").replace(/([#?](?:code|token)=)[^\s"<>]+/g, "$1<redacted>"));
  throw error;
} finally {
  await browser?.close().catch(() => {});
  if (token) {
    await api("POST", "/shutdown", {}).catch(() => {});
    for (let attempt = 0; attempt < 80; attempt++) {
      try { await fetch(base, { signal: AbortSignal.timeout(500) }); }
      catch { break; }
      await sleep(100);
    }
  }
  if (daemon && daemon.exitCode === null && daemon.signalCode === null) {
    const exited = new Promise((done) => daemon.once("exit", done));
    await Promise.race([exited, sleep(8000)]);
    if (daemon.exitCode === null && daemon.signalCode === null) daemon.kill("SIGKILL");
    await exited;
  }
  fixture.close();
  await rm(directory, { recursive: true, force: true, maxRetries: 10, retryDelay: 100 });
}
