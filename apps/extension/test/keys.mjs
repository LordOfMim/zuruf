import { chromium } from "playwright-core";
import { readFileSync } from "node:fs";
const content = readFileSync(process.argv[2], "utf8");
const browser = await chromium.launch({
  executablePath: "/usr/bin/chromium",
  headless: true,
});
const page = await browser.newPage();
await page.addInitScript(() => {
  window.__sent = [];
  const bind = (action, code, mods = []) => ({
    action,
    scope: "page",
    chord: { mods, code },
  });
  window.chrome = {
    runtime: {
      id: "x",
      getURL: (p) => "data:text/css,",
      onMessage: { addListener() {} },
      sendMessage: async (m) => {
        window.__sent.push(m);
        if (m.type === "ready")
          return {
            payload: {
              review: true,
              bindings: [
                bind("capture.viewport", "KeyW"),
                bind("speech.toggle", "KeyR"),
                bind("batch.dispatch", "KeyN", ["shift"]),
                bind("panel.toggle", "KeyP", ["alt"]),
              ],
            },
          };
        return { ok: true };
      },
    },
  };
});
await page.addInitScript(content);
await page.route("http://zuruf.test/", (r) =>
  r.fulfill({
    contentType: "text/html",
    body: `<input id=i><div id=app tabindex=0>app</div><script>
  window.__seen=[]; for (const t of ["keydown","keypress","keyup"]) window.addEventListener(t,e=>{window.__seen.push(t+":"+e.code);},true);
</script>`,
  }),
);
await page.goto("http://zuruf.test/");
page.on("pageerror", (e) => console.log("pageerror", e.message));
await page.waitForTimeout(200);
const actions = async () =>
  await page.evaluate(() =>
    window.__sent
      .filter((m) => m.type === "action")
      .map((m) => m.payload.action),
  );
const seen = async () =>
  await page.evaluate(() => window.__seen.filter((s) => !/Alt|Shift/.test(s)));
const reset = () =>
  page.evaluate(() => {
    window.__sent = [];
    window.__seen = [];
  });
const out = [];
const check = (name, ok, detail) => {
  out.push(
    `${ok ? "ok  " : "FAIL"} ${name}${ok ? "" : " " + JSON.stringify(detail)}`,
  );
};
await page.click("#app");
await reset();
await page.keyboard.press("r");
check(
  "solo R outside a field triggers, page sees nothing",
  (await actions()).join() === "speech.toggle" && (await seen()).length === 0,
  [await actions(), await seen()],
);
await page.click("#i");
await reset();
await page.keyboard.press("r");
check(
  "solo R in a field types",
  (await actions()).length === 0 && (await page.inputValue("#i")) === "r",
  [await actions(), await page.inputValue("#i")],
);
await reset();
await page.keyboard.press("Alt+r");
check(
  "Alt+R in a field triggers, field unchanged, page sees nothing",
  (await actions()).join() === "speech.toggle" &&
    (await page.inputValue("#i")) === "r" &&
    (await seen()).length === 0,
  [await actions(), await page.inputValue("#i"), await seen()],
);
await reset();
await page.keyboard.press("Alt+Shift+N");
check(
  "Alt+Shift+N in a field dispatches",
  (await actions()).join() === "batch.dispatch",
  await actions(),
);
await reset();
await page.keyboard.press("Alt+p");
check(
  "explicit Alt+P works in a field",
  (await actions()).join() === "panel.toggle",
  await actions(),
);
await reset();
await page.keyboard.press("Alt+x");
check(
  "unbound Alt+X is left to the page",
  (await actions()).length === 0 && (await seen()).includes("keydown:KeyX"),
  [await actions(), await seen()],
);
console.log(out.join("\n"));
await browser.close();
process.exit(out.some((l) => l.startsWith("FAIL")) ? 1 : 0);
