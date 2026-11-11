// Fault injection against an isolated real daemon and CEF shell. No T3 or microphone.
import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { mkdtemp, readdir, readFile, readlink, rm, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { promisify } from "node:util";
import { chromium } from "../../apps/page/node_modules/playwright-core/index.mjs";

const root = resolve(import.meta.dirname, "../..");
const exec = promisify(execFile);
const target = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const binary = process.env.ZURUF_E2E_BINARY ?? join(target, "debug/zuruf");
const directory = await mkdtemp(join(tmpdir(), "zuruf-recovery-"));
const port = 47644;
const base = `http://127.0.0.1:${port}`;
const env = {
  ...process.env,
  ZURUF_PORT: String(port),
  ZURUF_DATA_DIR: join(directory, "data"),
  ZURUF_STATE_DIR: join(directory, "state"),
  ZURUF_CONFIG_DIR: join(directory, "config"),
  ZURUF_SHELL: join(directory, "launcher"),
  ZURUF_TEST_REAL_SHELL: process.env.ZURUF_SHELL ?? join(target, "debug/zuruf-shell"),
  ZURUF_TEST_DIR: directory,
  RUST_LOG: "zuruf=info,zuruf_browser::session=debug",
};
delete env.ZURUF_FAKE_ADAPTERS;
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
let session;
let log = "";
let connection;
let observation;
const ownedPids = new Set();
const passed = (name) => console.log(`PASS ${name}`);
async function api(method, path, body) {
  const response = await fetch(`${base}/api/v1${path}`, {
    method,
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(25000),
  });
  const data = await response.json();
  assert(response.ok, `${method} ${path}: ${JSON.stringify(data)}`);
  return data;
}
const current = async () => (await api("GET", "/state")).sessions.find((s) => s.id === session.id);
const act = (action) => api("POST", `/sessions/${session.id}/${action}`, {});
const launches = async () => (await readFile(join(directory, "launches"), "utf8").catch(() => "")).trim().split("\n").filter(Boolean).length;
async function shells() {
  if (!session) return [];
  const profile = `--zuruf-profile=${join(env.ZURUF_DATA_DIR, "sessions", session.id, "profile")}`;
  const matches = await Promise.all((await readdir("/proc")).filter((pid) => /^\d+$/.test(pid)).map(async (pid) => {
    try {
      const args = (await readFile(`/proc/${pid}/cmdline`, "utf8")).split("\0");
      if (!args.includes(profile) || args.some((arg) => arg.startsWith("--type="))) return null;
      if (basename(await readlink(`/proc/${pid}/exe`)) !== "zuruf-shell") return null;
      const entry = { pid: Number(pid), port: Number(args.find((arg) => arg.startsWith("--zuruf-port=")).split("=")[1]) };
      ownedPids.add(entry.pid);
      return entry;
    } catch { return null; }
  }));
  return matches.filter(Boolean);
}
async function ready(previousPid) {
  return until("a recovered, scriptable session with one new shell", async () => {
    const s = await current();
    const processes = await shells();
    return s.state === "running" && s.tabs.length > 0 && s.tabs.every((tab) => tab.scriptable && [first, second].includes(tab.url))
      && processes.length === 1 && processes[0].pid !== previousPid ? processes[0] : null;
  });
}
async function startDaemon() {
  daemon = spawn(binary, ["daemon"], { cwd: root, env, stdio: ["ignore", "ignore", "pipe"] });
  daemon.stderr.on("data", (chunk) => (log += chunk));
  await until("the isolated daemon", async () => {
    if (daemon.exitCode !== null) throw new Error("daemon exited before listening");
    try {
      token = (await readFile(join(env.ZURUF_STATE_DIR, "token"), "utf8")).trim();
      return (await fetch(`${base}/api/v1/state`, {
        headers: { authorization: `Bearer ${token}` }, signal: AbortSignal.timeout(1000),
      })).ok;
    } catch { return false; }
  });
}
async function rawBrowser(shell) {
  return chromium.connectOverCDP(`http://127.0.0.1:${shell.port}`);
}
const fixtures = createServer((request, response) => {
  const page = new URL(request.url, "http://fixture").pathname;
  response.writeHead(200, { "content-type": "text/html" }).end(`<title>${page}</title><h1>Recovery fixture</h1>`);
});
await new Promise((done) => fixtures.listen(0, "127.0.0.1", done));
const first = `http://127.0.0.1:${fixtures.address().port}/first`;
const second = `http://127.0.0.1:${fixtures.address().port}/second`;
try {
  await writeFile(env.ZURUF_SHELL, `#!/usr/bin/env bash
printf 'launch\\n' >> "$ZURUF_TEST_DIR/launches"
if [[ -f "$ZURUF_TEST_DIR/fail" ]]; then exit 42; fi
while [[ -f "$ZURUF_TEST_DIR/delay" ]]; do sleep 0.1; done
exec "$ZURUF_TEST_REAL_SHELL" "$@"
`, { mode: 0o700 });
  await startDaemon();
  const project = await api("POST", "/projects", { name: "Recovery", repositoryPath: directory, baseUrl: first });
  session = await api("POST", "/sessions", { projectId: project.id, name: "Recovery", mode: "shared", agentAccess: true });

  await writeFile(join(directory, "fail"), "");
  assert.equal((await act("start")).state, "starting");
  await Promise.all(Array.from({ length: 5 }, () => act("start")));
  await until("automatic launch retries", async () => (await launches()) >= 3);
  assert((await launches()) < 8, "repeated Start must not create concurrent retry loops");
  await act("stop");
  const stoppedAt = await launches();
  await sleep(1100);
  assert.equal(await launches(), stoppedAt);
  assert.equal((await current()).state, "stopped");
  passed("failed launches retry quietly; repeated Start is idempotent; Stop cancels retries");

  await rm(join(directory, "fail"));
  await writeFile(join(directory, "delay"), "");
  const starting = act("start");
  await until("an in-flight delayed launch", async () => (await launches()) > stoppedAt);
  const beforeStop = Date.now();
  await act("stop");
  await starting;
  assert(Date.now() - beforeStop < 3000, "Stop must cancel startup instead of waiting for its timeout");
  await rm(join(directory, "delay"));
  await sleep(600);
  assert.equal((await shells()).length, 0);
  assert.equal((await current()).state, "stopped");
  passed("Stop cancels an in-flight launch without resurrection");

  await act("start");
  let live = await ready();
  await api("POST", `/sessions/${session.id}/tabs`, { url: second });
  await api("POST", `/sessions/${session.id}/page-width`, { width: 390 });
  connection = await rawBrowser(live);
  const page = await until("the second active page", () => connection.contexts()[0].pages().find((page) => page.url() === second));
  await page.bringToFront();
  await page.evaluate(() => localStorage.setItem("recovery-check", "retained"));
  await page.context().addCookies([{ name: "recovery-session", value: "retained", url: first }]);
  await until("both tabs and width to be recorded", async () => {
    const s = await current();
    observation = { tabs: s.tabs.map((tab) => ({ url: tab.url, active: tab.active })), width: await page.evaluate(() => innerWidth) };
    return s.tabs.length === 2 && s.tabs.some((tab) => tab.url === second)
      && observation.width === 390;
  });
  // Commit the test profile with a graceful restart before injecting SIGKILL;
  // recovery guarantees durable profile data, not unflushed renderer memory.
  await connection.close(); connection = undefined;
  await act("restart");
  live = await ready(live.pid);
  const oldTabs = (await current()).tabs.map((tab) => tab.id);
  const access = await api("POST", `/sessions/${session.id}/agent-access`, { agentId: "recovery-test" });
  await api("POST", `/sessions/${session.id}/control`, { action: "grant", agentId: "recovery-test", leaseSeconds: 60 });
  process.kill(live.pid, "SIGKILL");
  live = await ready(live.pid);
  const restored = await current();
  assert.equal(restored.control.owner, "none");
  assert(restored.tabs.every((tab) => !oldTabs.includes(tab.id)));
  assert.deepEqual(restored.tabs.map((tab) => tab.url).sort(), [first, second].sort());
  const refused = await fetch(`${base}/cdp/${session.id}`, { headers: { authorization: `Bearer ${access.token}` } });
  assert.equal(refused.status, 401, "the old agent credential must be revoked");
  connection = await rawBrowser(live);
  const visibleFixturePage = async () => {
    for (const page of connection.contexts()[0].pages()) {
      if ([first, second].includes(page.url()) && await page.evaluate(() => innerWidth) === 390) return page;
    }
    return null;
  };
  // Native tab layout sizes the visible view; hidden tabs resize when selected.
  const restoredPage = await until("the saved width on the visible restored page", visibleFixturePage);
  assert.equal(await restoredPage.evaluate(() => localStorage.getItem("recovery-check")), "retained");
  assert((await restoredPage.context().cookies(first)).some((cookie) => cookie.name === "recovery-session" && cookie.value === "retained"));
  await connection.close(); connection = undefined;
  passed("SIGKILL restores tabs, width and profile, and revokes the old agent lease");

  if (process.env.HYPRLAND_INSTANCE_SIGNATURE) {
    const { stdout } = await exec("hyprctl", ["clients", "-j"]);
    const window = JSON.parse(stdout).find((window) => window.pid === live.pid && window.class === "zuruf");
    assert(window, "close only our session window");
    assert.match(window.address, /^0x[0-9a-f]+$/i);
    await exec("hyprctl", ["dispatch", `hl.dsp.window.close({window="address:${window.address}"})`])
      .catch(() => exec("hyprctl", ["dispatch", "closewindow", `address:${window.address}`]));
    live = await ready(live.pid);
    await until("all tabs after native window close", async () => (await current()).tabs.length === 2);
    assert.deepEqual((await current()).tabs.map((tab) => tab.url).sort(), [first, second].sort());
    passed("native window close restores every saved tab");
  }

  connection = await rawBrowser(live);
  const cdp = await connection.newBrowserCDPSession();
  const { processInfo } = await cdp.send("SystemInfo.getProcessInfo");
  const rendererPids = processInfo.filter((process) => process.type === "renderer").map((process) => Number(process.id));
  assert(rendererPids.length, "the test browser has renderer processes");
  // Chromium can list an unused spare renderer first. Kill all of this test's
  // renderers so a real page/panel process is definitely interrupted.
  for (const rendererPid of rendererPids) {
    assert.notEqual(rendererPid, live.pid);
    assert.equal(basename(await readlink(`/proc/${rendererPid}/exe`)), "zuruf-shell");
    let ancestor = rendererPid;
    const ancestors = new Set();
    while (ancestor > 1 && ancestor !== live.pid && !ancestors.has(ancestor)) {
      ancestors.add(ancestor);
      const status = await readFile(`/proc/${ancestor}/status`, "utf8");
      ancestor = Number(status.match(/^PPid:\s*(\d+)/m)?.[1] ?? 0);
    }
    assert.equal(ancestor, live.pid, "kill only a renderer descended from our browser");
  }
  for (const pid of rendererPids) { try { process.kill(pid, "SIGKILL"); } catch (error) { if (error.code !== "ESRCH") throw error; } }
  live = await ready(live.pid);
  await connection.close(); connection = undefined;
  passed("a crashed page renderer also recovers automatically");

  await api("POST", `/sessions/${session.id}/control`, { action: "takeover" });
  process.kill(live.pid, "SIGSTOP");
  live = await ready(live.pid);
  assert.equal((await current()).control.owner, "human");
  passed("an unresponsive browser is replaced and human takeover is retained");

  // Kill the controller too: the same private state must recover at next startup.
  const oldDaemon = daemon;
  oldDaemon.kill("SIGKILL");
  await new Promise((done) => oldDaemon.once("exit", done));
  await startDaemon();
  live = await ready(live.pid);
  connection = await rawBrowser(live);
  const panel = await until("the restored app panel", () => connection.contexts()[0].pages().find((page) => page.url().startsWith(`${base}/panel.html`)));
  await panel.getByRole("button", { name: "Sitzungen", exact: true }).waitFor({ timeout: 15000 });
  assert.equal((await current()).tabs.length, 2);
  await until("the width after daemon restart", visibleFixturePage);
  await connection.close(); connection = undefined;
  passed("daemon restart restores the session and a usable authenticated panel");

  // Stop is ordered after the restart request has actually begun.
  const count = await launches();
  const restarting = act("restart");
  await until("the forced restart launch", async () => await launches() > count);
  await act("stop");
  await restarting;
  await sleep(800);
  assert.equal((await current()).state, "stopped");
  assert.equal((await shells()).length, 0);
  const finalCount = await launches();
  await sleep(1100);
  assert.equal(await launches(), finalCount);
  passed("a later Stop wins over an overlapping Restart and stays stopped");
} catch (error) {
  console.error(observation);
  if (session) {
    console.error("fault state", await current().catch(() => "unavailable"));
    const shellLog = join(env.ZURUF_DATA_DIR, "sessions", session.id, "shell.log");
    console.error((await readFile(shellLog, "utf8").catch(() => "")).split("\n").slice(-10).join("\n"));
  }
  // Logs contain only synthetic data; redact the panel's one-use code regardless.
  console.error(log.split("\n").slice(-15).join("\n").replace(/([#?](?:code|token)=)[^\s"<>]+/g, "$1<redacted>"));
  throw error;
} finally {
  await connection?.close().catch(() => {});
  if (daemon && daemon.exitCode === null && daemon.signalCode === null) {
    await api("POST", "/shutdown", {}).catch(() => {});
    await Promise.race([new Promise((done) => daemon.once("exit", done)), sleep(6000)]);
    if (daemon.exitCode === null && daemon.signalCode === null) daemon.kill("SIGKILL");
  }
  for (const shell of await shells()) ownedPids.add(shell.pid);
  for (const pid of ownedPids) { try { process.kill(pid, "SIGKILL"); } catch {} }
  fixtures.close();
  await rm(directory, { recursive: true, force: true });
}
