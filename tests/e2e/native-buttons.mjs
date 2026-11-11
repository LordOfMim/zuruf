// Exercise the real CEF mouse-release callback that used to destroy its own button.
// The stdin test command exists only in debug builds; it clicks the native view.
import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { promisify } from "node:util";

const exec = promisify(execFile);
const root = resolve(import.meta.dirname, "../..");
const target = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const directory = await mkdtemp(join(tmpdir(), "zuruf-native-buttons-"));
const socket = createServer();
await new Promise((done) => socket.listen(0, "127.0.0.1", done));
const port = socket.address().port;
await new Promise((done) => socket.close(done));
const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
let shell;
let log = "";
async function until(name, probe) {
  const deadline = Date.now() + 20000;
  while (Date.now() < deadline) {
    if (shell.exitCode !== null || shell.signalCode !== null) {
      throw new Error(`shell exited while waiting for ${name}: ${shell.exitCode ?? shell.signalCode}`);
    }
    const result = await probe();
    if (result) return result;
    await sleep(75);
  }
  throw new Error(`timed out waiting for ${name}`);
}
const command = (body) => shell.stdin.write(JSON.stringify(body) + "\n");
async function closeNativeTab() {
  if (process.env.HYPRLAND_INSTANCE_SIGNATURE) {
    const window = await until("our native window", async () => {
      const { stdout } = await exec("hyprctl", ["clients", "-j"]);
      return JSON.parse(stdout).find((window) => window.pid === shell.pid);
    });
    assert.match(window.address, /^0x[0-9a-f]+$/i);
    await exec("hyprctl", ["dispatch", `hl.dsp.focus({window="address:${window.address}"})`])
      .catch(() => exec("hyprctl", ["dispatch", "focuswindow", `address:${window.address}`]));
  }
  command({ command: "focus" });
  await sleep(200);
  command({ command: "test-close-tab" });
}
async function targets() {
  try {
    const response = await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(1000) });
    return (await response.json()).filter((target) => target.type === "page");
  } catch {
    return [];
  }
}
try {
  shell = spawn(process.env.ZURUF_SHELL ?? join(target, "debug/zuruf-shell"), [
    `--zuruf-profile=${join(directory, "profile")}`,
    `--zuruf-port=${port}`,
    "--zuruf-url=data:text/html,<title>First</title>",
    "--zuruf-url=data:text/html,<title>Second</title>",
    ...(process.env.ZURUF_SHELL_ARGS?.split(/\s+/).filter(Boolean) ?? []),
  ], { stdio: ["pipe", "ignore", "pipe"] });
  shell.stderr.on("data", (chunk) => (log += chunk));
  await until("both initial tabs", async () => (await targets()).length === 2);
  await closeNativeTab();
  await until("the native close to remove a tab", async () => (await targets()).length === 1);
  console.log("PASS native tab close keeps the shell alive");

  const old = (await targets())[0].id;
  await closeNativeTab();
  await until("the last tab to be replaced with a blank tab", async () => {
    const tabs = await targets();
    return tabs.length === 1 && tabs[0].id !== old && tabs[0].url === "about:blank";
  });
  console.log("PASS closing the final native tab creates a live blank tab");
  for (let index = 0; index < 8; index++) {
    const url = `data:text/html,<title>Repeat ${index}</title>`;
    command({ command: "open", url });
    await until("another loaded tab", async () => (await targets()).some((tab) => tab.title === `Repeat ${index}`));
    await closeNativeTab();
    await until("another native close", async () => {
      const tabs = await targets();
      return tabs.length === 1 && tabs[0].url === "about:blank";
    });
  }
  console.log("PASS repeated native close callbacks stay usable");
} catch (error) {
  console.error((await targets()).map((tab) => ({ title: tab.title, url: tab.url })));
  console.error(log.split("\n").slice(-10).join("\n"));
  throw error;
} finally {
  if (shell && shell.exitCode === null && shell.signalCode === null) {
    command({ command: "quit" });
    await Promise.race([new Promise((done) => shell.once("exit", done)), sleep(5000)]);
    if (shell.exitCode === null && shell.signalCode === null) shell.kill("SIGKILL");
  }
  await rm(directory, { recursive: true, force: true });
}
