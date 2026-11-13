// Real packaged daemon + real Chromium, using only temporary state and local fixtures.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "../../apps/page/node_modules/playwright-core/index.mjs";

const prefixArgument = process.argv.indexOf("--prefix");
const stages = { win32: "windows-x64", darwin: "macos-arm64/Zuruf.app", linux: "linux-x64" };
// On macOS the prefix is the app bundle; the programs live in Contents/MacOS.
const prefix = resolve(prefixArgument >= 0 ? process.argv[prefixArgument + 1] : `.release-stage/${stages[process.platform] ?? stages.linux}`);
const cli = process.platform === "darwin"
  ? join(prefix, "Contents/MacOS/zuruf")
  : join(prefix, `bin/zuruf${process.platform === "win32" ? ".exe" : ""}`);
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
let ownerBrowser;
let lastSession;
let log = "";
const viaLauncher = process.platform === "win32";
function alive(pid) {
  try { process.kill(pid, 0); return true; } catch { return false; }
}
async function daemonLog() {
  const contents = log + await readFile(join(env.ZURUF_STATE_DIR, "daemon.log"), "utf8").catch(() => "");
  return contents.replace(/\u001b\[[0-9;]*m/g, "");
}
async function appWindows() {
  const plain = await daemonLog();
  return [...plain.matchAll(/app window started pid=(\d+)/g)].map((match) => Number(match[1]));
}
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
  daemon = spawn(viaLauncher ? join(prefix, "Zuruf.exe") : cli, viaLauncher ? [] : ["daemon"], { env, windowsHide: true, stdio: ["ignore", "ignore", "pipe"] });
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
  // This inert profile only unlocks the real session-management UI. Never dispatch it.
  await api("POST", "/profiles", { name: "UI fixture", providerInstanceId: "fixture-not-connected", model: "fixture", options: {}, workspaceStrategy: "shared_root" });
  const project = await api("POST", "/projects", { name: "Release fixture", repositoryPath: directory, baseUrl: url });
  const session = await api("POST", "/sessions", { projectId: project.id, name: "Release smoke", mode: "shared", agentAccess: true });
  const current = async () => (lastSession = (await api("GET", "/state")).sessions.find((entry) => entry.id === session.id));
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
  for (let restart = 0; restart < 3; restart++) {
    assert.equal((await api("POST", `/sessions/${session.id}/restart`, {})).state, "running");
    await until(`the restored fixture after restart ${restart + 1}`, async () => (await current()).tabs.some((tab) => tab.scriptable && tab.url === url));
    page = await connect();
    assert.equal(await page.evaluate(() => localStorage.getItem("release-check")), "persisted");
    await until("the restored width", async () => await page.evaluate(() => innerWidth) === 390);
    await browser.close(); browser = undefined;
  }
  for (let cycle = 0; cycle < 3; cycle++) {
    const windowsBeforeStop = (await appWindows()).length;
    // The owner inspects the native panel; the agent connection above must never see it.
    const nativeLog = await readFile(join(env.ZURUF_DATA_DIR, "sessions", session.id, "shell.log"), "utf8");
    const nativePort = [...(await daemonLog()).matchAll(/browser session started session_id=([\w-]+) port=(\d+)/g)]
      .filter((match) => match[1] === session.id).at(-1)?.[2];
    const endpoint = nativeLog.match(/DevTools listening on (ws:\/\/[^\s]+)/)?.[1]
      ?? (nativePort ? `http://127.0.0.1:${nativePort}` : undefined);
    assert(endpoint, "the native test browser must publish its diagnostic endpoint");
    ownerBrowser = await chromium.connectOverCDP(endpoint);
    const panel = await until("the actual docked app", () => ownerBrowser.contexts()[0].pages().find((page) => page.url().startsWith(`${base}/panel.html`)));
    await panel.getByRole("button", { name: "Sitzungen", exact: true }).click();
    await panel.getByRole("button", { name: "Stoppen", exact: true }).click({ noWaitAfter: true }).catch((error) => {
      if (!panel.isClosed()) throw error;
    });
    await until("the session stopped by its own panel", async () => (await current()).state === "stopped");
    const appPid = await until("the standalone app after stopping the last session", async () =>
      (await appWindows()).slice(windowsBeforeStop).find(alive));
    await ownerBrowser.close().catch(() => {}); ownerBrowser = undefined;
    await sleep(600);
    assert.equal((await current()).state, "stopped");
    assert(alive(appPid), "stopping a session must keep the application open");
    await api("POST", `/sessions/${session.id}/start`, {});
    await until("the standalone app to dock into the new session", () => !alive(appPid));
    await until("the redocked fixture", async () => (await current()).tabs.some((tab) => tab.scriptable && tab.url === url));
    console.log(`PASS native panel Stop and redock cycle ${cycle + 1}`);
  }
  // Updates use this CLI while windows are still open. It must wait for browser cleanup.
  const stop = spawn(cli, ["stop"], { env, windowsHide: true, stdio: "ignore" });
  const stopped = new Promise((done, reject) => {
    stop.once("error", reject);
    stop.once("exit", (code) => code === 0 ? done() : reject(new Error(`CLI stop exited ${code}`)));
  });
  await stopped;
  await assert.rejects(fetch(base, { signal: AbortSignal.timeout(1000) }), "CLI stop must wait for shutdown");
  console.log("PASS packaged native browser: start, click, screenshot, filtered CDP, width, profile recovery, session stop keeps app open, redocking and update shutdown");
} catch (error) {
  const redact = (text) => text.replace(/([#?](?:code|token)=)[^\s"<>]+/g, "$1<redacted>").replace(/Bearer\s+\S+/g, "Bearer <redacted>");
  if (lastSession) {
    const tabs = lastSession.tabs.map((tab) => {
      let safeUrl = "<invalid>";
      try {
        const url = new URL(tab.url);
        url.username = ""; url.password = ""; url.search = ""; url.hash = "";
        safeUrl = url.href;
      } catch {}
      return { url: safeUrl, scriptable: tab.scriptable, active: tab.active };
    });
    console.error("Last observed session:", JSON.stringify({ state: lastSession.state, tabs }));
  }
  const browserLogs = [join(env.ZURUF_DATA_DIR, "app-window/shell.log")];
  if (lastSession) browserLogs.push(join(env.ZURUF_DATA_DIR, "sessions", lastSession.id, "shell.log"));
  for (const path of browserLogs) {
    const contents = await readFile(path, "utf8").catch(() => "");
    if (contents) console.error("Native browser log:", redact(contents.split("\n").slice(-12).join("\n")));
  }
  console.error(redact(log.split("\n").slice(-12).join("\n")));
  throw error;
} finally {
  await ownerBrowser?.close().catch(() => {});
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
