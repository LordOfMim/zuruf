// The real task UI and local API, backed by isolated fake adapters. No provider calls.
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { chromium } from "../../apps/page/node_modules/playwright-core/index.mjs";

const root = resolve(import.meta.dirname, "../..");
const target = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const directory = await mkdtemp(join(tmpdir(), "zuruf-task-settle-"));
const port = 47645;
const debugPort = 47675;
const base = `http://127.0.0.1:${port}`;
const env = {
  ...process.env,
  ZURUF_PORT: String(port),
  ZURUF_DATA_DIR: join(directory, "data"),
  ZURUF_STATE_DIR: join(directory, "state"),
  ZURUF_CONFIG_DIR: join(directory, "config"),
  ZURUF_FAKE_ADAPTERS: "1",
};
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
async function until(name, probe) {
  const end = Date.now() + 15000;
  while (Date.now() < end) {
    const result = await probe();
    if (result) return result;
    await sleep(60);
  }
  throw new Error(`timed out waiting for ${name}`);
}
let daemon;
let shell;
let browser;
let token;
let log = "";
async function api(method, path, body) {
  const response = await fetch(`${base}/api/v1${path}`, {
    method,
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
    signal: AbortSignal.timeout(3000),
  });
  assert(response.ok, `${method} ${path}: ${response.status}`);
  return response.json();
}
const tasks = async () => (await api("GET", "/state")).tasks;
async function startDaemon() {
  daemon = spawn(process.env.ZURUF_E2E_BINARY ?? join(target, "debug/zuruf"), ["daemon", "--seed-demo"], {
    cwd: root, env, stdio: ["ignore", "ignore", "pipe"],
  });
  daemon.stderr.on("data", (chunk) => (log += chunk));
  await until("the isolated daemon", async () => {
    if (daemon.exitCode !== null) throw new Error("test daemon exited");
    try {
      token = (await readFile(join(env.ZURUF_STATE_DIR, "token"), "utf8")).trim();
      return (await tasks()).length === 3;
    } catch { return false; }
  });
}
async function stopDaemon() {
  if (daemon && daemon.exitCode === null && daemon.signalCode === null) {
    const exit = new Promise((done) => daemon.once("exit", done));
    await api("POST", "/shutdown", {}).catch(() => {});
    await Promise.race([exit, sleep(5000)]);
    if (daemon.exitCode === null && daemon.signalCode === null) daemon.kill("SIGKILL");
    await exit;
  }
}
try {
  await startDaemon();
  const original = await tasks();
  const { code } = await api("POST", "/auth/code", {});
  shell = spawn(process.env.ZURUF_SHELL ?? join(target, "debug/zuruf-shell"), [
    `--zuruf-profile=${join(directory, "profile")}`,
    `--zuruf-port=${debugPort}`,
    `--zuruf-panel=${base}/index.html#code=${code}&view=tasks`,
    "--zuruf-app-only",
    ...(process.env.ZURUF_SHELL_ARGS?.split(/\s+/).filter(Boolean) ?? []),
  ], { stdio: ["pipe", "ignore", "pipe"] });
  shell.stderr.on("data", (chunk) => (log += chunk));
  await until("our shell's debugger", async () => {
    try { return (await fetch(`http://127.0.0.1:${debugPort}/json/version`, { signal: AbortSignal.timeout(1000) })).ok; }
    catch { return false; }
  });
  browser = await chromium.connectOverCDP(`http://127.0.0.1:${debugPort}`);
  const page = await until("our task board", () => browser.contexts()[0].pages().find((page) => page.url().startsWith(base)));
  await page.getByRole("heading", { name: "Aufgaben", exact: true }).waitFor();
  const row = (task) => page.locator("li.task").filter({ has: page.locator(".task-title", { hasText: task.title }) });
  const group = (name) => page.locator("section.group").filter({ has: page.getByRole("heading", { name, exact: true }) });
  const open = async (task) => {
    if (await row(task).locator(".task-line").getAttribute("aria-expanded") === "false") await row(task).locator(".task-line").click();
  };
  const waitMarked = (id, value) => until("the saved completion flag", async () => (await tasks()).find((task) => task.id === id)?.markedDone === value);
  await until("one initial attention badge", async () => await page.locator(".rail-count").textContent() === "1");

  for (const task of original) {
    await open(task);
    await row(task).getByRole("button", { name: "Abschließen", exact: true }).click();
    await waitMarked(task.id, true);
    await until("the task in Abgeschlossen", async () => await group("Abgeschlossen").locator("li.task").filter({ hasText: task.title }).count() === 1);
    const saved = (await tasks()).find((entry) => entry.id === task.id);
    assert.equal(saved.status, task.status, "settling must not turn an interruption into success");
    assert.equal(saved.error, task.error);
    if (task.status === "cancelled") assert.match(await row(task).locator(".task-state").textContent(), /Abgebrochen/);
  }
  await until("the attention badge to disappear", async () => await page.locator(".rail-count").count() === 0);
  assert.equal(await group("Braucht dich").count(), 0);
  console.log("PASS successful, cancelled and failed tasks settle without changing their run status; groups and badge update");

  await stopDaemon();
  await startDaemon();
  await page.reload();
  await page.getByRole("heading", { name: "Abgeschlossen", exact: true }).waitFor();
  assert((await tasks()).every((task) => task.markedDone));
  assert.equal(await group("Abgeschlossen").locator("li.task").count(), 3);
  assert.equal(await page.locator(".rail-count").count(), 0);
  console.log("PASS settlement survives daemon restart and page reload");

  const failed = original.find((task) => task.status === "failed");
  await open(failed);
  await row(failed).getByRole("button", { name: "Wieder öffnen", exact: true }).click();
  await waitMarked(failed.id, false);
  await until("the reopened task needing attention", async () => await group("Braucht dich").locator("li.task").count() === 1);
  await until("the attention badge to return", async () => await page.locator(".rail-count").textContent() === "1");
  await row(failed).getByRole("button", { name: "Abschließen", exact: true }).click();
  await waitMarked(failed.id, true);
  await row(failed).getByRole("button", { name: "Erneut senden", exact: true }).click();
  await waitMarked(failed.id, false);
  console.log("PASS reopening restores attention; an explicit retry reopens a settled task automatically");
} catch (error) {
  console.error(log.split("\n").slice(-12).join("\n").replace(/([#?](?:code|token)=)[^\s"<>]+/g, "$1<redacted>"));
  throw error;
} finally {
  await browser?.close().catch(() => {});
  if (shell && shell.exitCode === null && shell.signalCode === null) {
    const exit = new Promise((done) => shell.once("exit", done));
    shell.stdin.end(JSON.stringify({ command: "quit" }) + "\n");
    await Promise.race([exit, sleep(5000)]);
    if (shell.exitCode === null && shell.signalCode === null) shell.kill("SIGKILL");
    await exit;
  }
  await stopDaemon();
  await rm(directory, { recursive: true, force: true });
}
