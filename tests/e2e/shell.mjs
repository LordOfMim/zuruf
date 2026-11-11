// End-to-end check of the integrated app: the real daemon, its own browser window, review
// hotkeys and an agent driving the page with stock Playwright through the daemon's proxy.
// Needs no T3 and no speech backend.
//
//   node tests/e2e/shell.mjs
//
// Environment: a display (DISPLAY or WAYLAND_DISPLAY), the debug build in the cargo target
// directory (or ZURUF_E2E_BINARY and ZURUF_SHELL), and optionally ZURUF_SHELL_ARGS, for example
// "--no-sandbox --disable-gpu" inside a container.
import { chromium } from "../../apps/page/node_modules/playwright-core/index.mjs";
import { execFile, spawn } from "node:child_process";
import { mkdtemp, readdir, readFile, readlink, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import { basename, join, resolve } from "node:path";
import { promisify } from "node:util";
import assert from "node:assert/strict";

const exec = promisify(execFile);
const root = resolve(import.meta.dirname, "../..");
const target = process.env.CARGO_TARGET_DIR ?? join(root, "target");
const binary = process.env.ZURUF_E2E_BINARY ?? join(target, "debug/zuruf");
const directory = await mkdtemp(join(tmpdir(), "zuruf-e2e-"));
const port = 47641;
const base = `http://127.0.0.1:${port}`;
const env = {
  ...process.env,
  ZURUF_PORT: String(port),
  ZURUF_DATA_DIR: join(directory, "data"),
  ZURUF_STATE_DIR: join(directory, "state"),
  ZURUF_CONFIG_DIR: join(directory, "config"),
  ZURUF_SHELL: process.env.ZURUF_SHELL ?? join(target, "debug/zuruf-shell"),
};
delete env.ZURUF_FAKE_ADAPTERS;

const sleep = (ms) => new Promise((done) => setTimeout(done, ms));
async function until(what, probe, limit = 20000) {
  const end = Date.now() + limit;
  while (Date.now() < end) {
    const result = await probe();
    if (result) return result;
    await sleep(50);
  }
  throw new Error(`timed out waiting for ${what}`);
}
const passed = (name) => console.log(`PASS ${name}`);

// The pages under review.
const fixtures = createServer(async (request, response) => {
  const name = new URL(request.url, "http://x").pathname.slice(1) || "index.html";
  try {
    const body = await readFile(join(root, "tests/browser/fixture", name.replace(/\.\./g, "")));
    response.writeHead(200, { "content-type": "text/html; charset=utf-8" }).end(body);
  } catch {
    response.writeHead(404).end();
  }
});
await new Promise((done) => fixtures.listen(0, "127.0.0.1", done));
const fixture = `http://127.0.0.1:${fixtures.address().port}`;

let token;
async function api(method, path, body) {
  const response = await fetch(`${base}/api/v1${path}`, {
    method,
    headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await response.json();
  assert(response.ok, `${method} ${path}: ${JSON.stringify(data)}`);
  return data;
}
const state = () => api("GET", "/state");
async function sessionPids(session) {
  const profile = `--zuruf-profile=${join(env.ZURUF_DATA_DIR, "sessions", session.id, "profile")}`;
  const pids = await Promise.all((await readdir("/proc")).filter((pid) => /^\d+$/.test(pid)).map(async (pid) => {
    try {
      const args = (await readFile(`/proc/${pid}/cmdline`, "utf8")).split("\0");
      return args.includes(profile) && !args.some((arg) => arg.startsWith("--type="))
        && basename(await readlink(`/proc/${pid}/exe`)) === "zuruf-shell" ? Number(pid) : null;
    } catch { return null; }
  }));
  return pids.filter(Boolean);
}

let daemon;
let browser;
let log = "";
try {
  daemon = spawn(binary, ["daemon"], { cwd: root, env, stdio: ["ignore", "ignore", "pipe"] });
  daemon.stderr.on("data", (chunk) => (log += chunk));
  await until("the daemon", async () => {
    if (daemon.exitCode !== null) throw new Error(`daemon exited:\n${log}`);
    try {
      token = (await readFile(join(env.ZURUF_STATE_DIR, "token"), "utf8")).trim();
      return (await fetch(`${base}/api/v1/state`, { headers: { authorization: `Bearer ${token}` } })).ok;
    } catch {
      return false;
    }
  });

  // Without a session the app has a window of its own.
  assert.equal((await api("POST", "/app/open", { view: "tasks" })).window, "app");
  passed("app opens in its own window");

  const project = await api("POST", "/projects", {
    name: "E2E",
    repositoryPath: directory,
    baseUrl: `${fixture}/index.html`,
  });
  const session = await api("POST", "/sessions", {
    projectId: project.id,
    name: "E2E",
    mode: "shared",
    agentAccess: true,
  });
  const started = await api("POST", `/sessions/${session.id}/start`, {});
  assert.equal(started.state, "running");
  await until("the page in the session window", async () =>
    (await state()).sessions
      .find((entry) => entry.id === session.id)
      ?.tabs.some((tab) => tab.url.startsWith(fixture) && tab.scriptable),
  );
  passed("session starts in the integrated browser");

  // One window by default: the app is now docked in the session's window.
  assert.equal((await api("POST", "/app/open", { view: "review" })).window, "session");
  passed("app docks into the session window");

  // An agent connects the way Playwright MCP does: CDP through the daemon's proxy.
  const access = await api("POST", `/sessions/${session.id}/agent-access`, { agentId: "e2e" });
  await api("POST", `/sessions/${session.id}/control`, {
    action: "grant",
    agentId: "e2e",
    leaseSeconds: 300,
  });
  browser = await chromium.connectOverCDP(`ws://127.0.0.1:${port}/cdp/${session.id}`, {
    headers: { Authorization: `Bearer ${access.token}` },
  });
  const context = browser.contexts()[0];
  const page = await until("the agent's view of the page", () =>
    context.pages().find((candidate) => candidate.url().startsWith(fixture)),
  );
  assert(
    context.pages().every((candidate) => !candidate.url().startsWith(base)),
    "the app's own panel must stay invisible to agents",
  );
  passed("Playwright connects through the proxy and sees only the page");

  await page.getByTestId("counter").click();
  assert.equal(await page.getByTestId("counter").getAttribute("data-count"), "1");
  await page.locator("#text").fill("Hallo");
  assert.equal(await page.locator("#text").inputValue(), "Hallo");
  assert.equal(await page.evaluate(() => document.title), "Zuruf Browser-Spike");
  assert((await page.screenshot()).length > 1000);
  passed("agent clicks, types, evaluates and takes screenshots");

  const second = await context.newPage();
  await second.goto(`${fixture}/frame.html`);
  await until("the agent's tab in the session window", async () =>
    (await state()).sessions
      .find((entry) => entry.id === session.id)
      ?.tabs.some((tab) => tab.url.endsWith("/frame.html")),
  );
  await second.close();
  await until("the agent's tab to close", async () =>
    (await state()).sessions
      .find((entry) => entry.id === session.id)
      ?.tabs.every((tab) => !tab.url.endsWith("/frame.html")),
  );
  passed("agent opens and closes a tab inside the window");

  // Review hotkeys: keys pressed in the page become annotations with screenshots.
  await api("POST", "/review-mode", { on: true });
  await page.bringToFront();
  await page.locator("h1").click();
  const capture = async (name, act) => {
    const before = (await state()).annotations.length;
    await act();
    const annotation = await until(`${name} annotation`, async () => {
      const annotations = (await state()).annotations;
      const latest = annotations.at(-1);
      const complete = latest?.assets.some((asset) => asset.kind === "screenshot");
      return annotations.length > before && complete ? latest : null;
    });
    const asset = annotation.assets.find((entry) => entry.kind === "screenshot");
    const image = await fetch(`${base}/api/v1/assets/${asset.id}`, {
      headers: { authorization: `Bearer ${token}` },
    });
    const bytes = new Uint8Array(await image.arrayBuffer());
    assert(image.ok, `asset ${asset.id}: ${image.status} ${new TextDecoder().decode(bytes.slice(0, 200))}`);
    // Every stored screenshot is a PNG.
    assert.deepEqual([...bytes.slice(1, 4)], [0x50, 0x4e, 0x47], `asset ${JSON.stringify(asset)}`);
    return annotation;
  };
  const viewport = await capture("viewport", async () => {
    await page.mouse.move(450, 380);
    await page.keyboard.press("w");
  });
  assert.equal(viewport.type, "viewport");
  // Element and region are chosen first: the key starts a selection, Escape leaves it again,
  // and the click that chooses never reaches the page.
  const counter = page.getByTestId("counter");
  const box = await counter.boundingBox();
  const centre = [box.x + box.width / 2, box.y + box.height / 2];
  const annotationsBefore = (await state()).annotations.length;
  await page.keyboard.press("d");
  await page.keyboard.press("Escape");
  await page.mouse.click(...centre);
  assert.equal(await counter.getAttribute("data-count"), "2", "after Escape the page gets its clicks back");
  await sleep(300);
  assert.equal((await state()).annotations.length, annotationsBefore, "Escape captures nothing");
  const element = await capture("element", async () => {
    await page.keyboard.press("d");
    await page.mouse.click(...centre);
  });
  assert.equal(element.dom?.tag, "button");
  assert.equal(element.dom?.role, "button");
  assert.equal(await counter.getAttribute("data-count"), "2", "the choosing click stays with Zuruf");
  const region = await capture("region", async () => {
    await page.keyboard.press("s");
    await page.mouse.move(420, 370);
    await page.mouse.down();
    await page.mouse.move(520, 430);
    await page.mouse.up();
  });
  assert.equal(region.type, "region");
  passed("viewport is captured at once; element and region are chosen first");

  // What the page reports on its own is attached to the capture and reaches the agent.
  await page.evaluate(() => {
    console.error("Speichern fehlgeschlagen", 500);
    void fetch("/gibt-es-nicht.json");
  });
  await sleep(500);
  const withReports = await capture("viewport with page reports", async () => {
    await page.locator("h1").click();
    await page.keyboard.press("w");
  });
  const reports = withReports.diagnostics ?? [];
  assert(
    reports.some((entry) => entry.kind === "console" && entry.text === "Speichern fehlgeschlagen 500"),
    JSON.stringify(reports),
  );
  assert(
    reports.some((entry) => entry.kind === "network" && entry.url?.endsWith("/gibt-es-nicht.json")),
    JSON.stringify(reports),
  );
  passed("console errors and failed requests travel with the capture");

  // A fixed page width: the page lays itself out for it and captures record it.
  await api("POST", `/sessions/${session.id}/page-width`, { width: 390 });
  await until("the narrow viewport", async () => (await page.evaluate(() => innerWidth)) === 390);
  const narrow = await capture("viewport at 390", async () => {
    await page.locator("h1").click();
    await page.keyboard.press("w");
  });
  assert.equal(narrow.viewport.width, 390);
  await api("POST", `/sessions/${session.id}/page-width`, { width: null });
  await until("the full width again", async () => (await page.evaluate(() => innerWidth)) > 390);
  passed("pages can be reviewed at a fixed width");

  // Typing into a field must stay typing.
  const before = (await state()).annotations.length;
  await page.locator("#text").fill("");
  await page.locator("#text").pressSequentially("wsd");
  assert.equal(await page.locator("#text").inputValue(), "wsd");
  await sleep(400);
  assert.equal((await state()).annotations.length, before);
  passed("hotkeys stay out of text fields");

  // The human always wins.
  await api("POST", `/sessions/${session.id}/control`, { action: "takeover" });
  await assert.rejects(page.evaluate(() => 1 + 1));
  passed("takeover locks the agent out");

  await browser.close();
  browser = undefined;
  const stopped = await api("POST", `/sessions/${session.id}/stop`, {});
  assert.equal(stopped.state, "stopped");
  passed("session stops");

  // A browser that dies comes back without another start request.
  const sessionState = async () =>
    (await state()).sessions.find((entry) => entry.id === session.id)?.state;
  assert.equal((await api("POST", `/sessions/${session.id}/start`, {})).state, "running");
  const oldTabs = (await state()).sessions.find((entry) => entry.id === session.id).tabs.map((tab) => tab.id);
  const beforeCrash = await sessionPids(session);
  assert.equal(beforeCrash.length, 1);
  process.kill(beforeCrash[0], "SIGKILL");
  await until("the page after the restart", async () =>
    (await state()).sessions
      .find((entry) => entry.id === session.id)
      ?.tabs.some((tab) => !oldTabs.includes(tab.id) && tab.url.startsWith(fixture) && tab.scriptable)
      && (await sessionState()) === "running",
  );
  passed("a crashed browser restarts automatically");

  // Losing the window also recovers; only the explicit stop command ends the session.
  let ownWindow;
  const ownPids = await sessionPids(session);
  assert.equal(ownPids.length, 1);
  const ownPid = ownPids[0];
  if (process.env.HYPRLAND_INSTANCE_SIGNATURE) {
    const { stdout } = await exec("hyprctl", ["clients", "-j"]);
    const windows = JSON.parse(stdout).filter(
      (window) => window.class === "zuruf" && window.pid === ownPid,
    );
    assert.equal(windows.length, 1, "close only the test's session window");
    ownWindow = windows[0].address;
    assert.match(ownWindow, /^0x[0-9a-f]+$/i);
  }
  const closeRequest = process.env.SWAYSOCK
    ? ["swaymsg", [`[app_id=zuruf pid=${ownPid}]`, "kill"]]
    : process.env.HYPRLAND_INSTANCE_SIGNATURE
      ? ["hyprctl", ["dispatch", `hl.dsp.window.close({ window = "address:${ownWindow}" })`]]
      : null;
  if (closeRequest) {
    await exec(closeRequest[0], closeRequest[1]).catch(async (error) => {
      if (closeRequest[0] !== "hyprctl") throw error;
      await exec("hyprctl", ["dispatch", "closewindow", `address:${ownWindow}`]);
    });
    await until("the closed window to recover", async () => {
      if ((await sessionState()) !== "running") return false;
      const pids = await sessionPids(session);
      return pids.length === 1 && pids[0] !== ownPid;
    });
    passed("closing the window restores the running session");
  } else {
    console.log("SKIP closing the window (needs sway or Hyprland)");
  }
  assert.equal((await api("POST", `/sessions/${session.id}/stop`, {})).state, "stopped");
  await sleep(800);
  assert.equal(await sessionState(), "stopped", "a deliberate stop cancels recovery");
  passed("an explicit stop stays stopped");
} catch (error) {
  console.error(log.split("\n").slice(-15).join("\n").replace(/([#?](?:code|token)=)[^\s"<>]+/g, "$1<redacted>"));
  throw error;
} finally {
  await browser?.close().catch(() => {});
  if (daemon && daemon.exitCode === null) {
    await fetch(`${base}/api/v1/shutdown`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: "{}",
    }).catch(() => {});
    await Promise.race([new Promise((done) => daemon.once("exit", done)), sleep(8000)]);
    daemon.kill();
  }
  fixtures.close();
  await rm(directory, { recursive: true, force: true });
}
console.log("All integrated app checks passed.");
