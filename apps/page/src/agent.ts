import type {
  Binding,
  DomCapture,
  LocatorCandidate,
  Rect,
} from "../../../packages/protocol/src/index";
import type { Incoming, Outgoing, Point, Reply } from "./protocol";
import styles from "./overlay.css";

// The app injects this file into an isolated world of every frame, before the page's own
// scripts run. It never touches the page's globals, and the page cannot call into it.
let review = false;
let bindings: Binding[] = [];
let cursor: Point = { x: 0, y: 0 };
let region = false;
let origin: Point | undefined;
let host: HTMLElement | undefined;
let overlay: HTMLElement | undefined;
let outline: HTMLElement | undefined;
let status: HTMLElement | undefined;
let confirmation: HTMLElement | undefined;
let hidden = false;
let recording = false;
let owner = "none";
/** Pills speak for the whole tab, so embedded frames stay silent. */
const isTop = window === window.top;
function renderStatus(): void {
  if (status) {
    status.hidden = !review || !isTop;
    // One word, one dot: the page stays the subject. State is carried by attributes for overlay.css.
    status.dataset.recording = String(recording);
    status.dataset.owner = owner;
    status.textContent =
      owner === "agent" ? "Agent steuert" : recording ? "Aufnahme" : "Review";
  }
}
function mount(): void {
  if (host || !document.documentElement) return;
  host = document.createElement("zuruf-overlay");
  const root = host.attachShadow({ mode: "closed" });
  // A constructed sheet is not subject to the page's Content Security Policy.
  const sheet = new CSSStyleSheet();
  sheet.replaceSync(styles);
  root.adoptedStyleSheets = [sheet];
  overlay = document.createElement("div");
  overlay.className = "overlay";
  overlay.hidden = hidden;
  outline = document.createElement("div");
  outline.className = "outline";
  outline.hidden = true;
  status = document.createElement("div");
  status.className = "status";
  status.hidden = !review || !isTop;
  status.textContent = "Review";
  confirmation = document.createElement("div");
  confirmation.className = "confirmation";
  confirmation.hidden = true;
  overlay.append(outline, status, confirmation);
  root.append(overlay);
  document.documentElement.append(host);
}
// Messages wait here until the app's binding exists in this world.
const outbox: string[] = [];
function flush(): void {
  const post = globalThis.__zurufSend;
  if (typeof post !== "function") {
    if (outbox.length > 0) setTimeout(flush, 15);
    return;
  }
  for (const json of outbox.splice(0)) post(json);
}
function send(message: Outgoing): void {
  // Pointer positions are only worth their latest value.
  if (message.type === "pointer" && outbox.length > 64) return;
  outbox.push(JSON.stringify(message));
  flush();
}
const guardSelector =
  'input,textarea,select,[role="textbox"],[role="combobox"],.monaco-editor,.cm-editor,.CodeMirror,[data-zuruf-ignore]';
function guarded(event: KeyboardEvent): boolean {
  if (
    event
      .composedPath()
      .some(
        (node) =>
          node instanceof HTMLElement &&
          (node.matches(guardSelector) || node.isContentEditable),
      )
  )
    return true;
  let focused: Element | null = document.activeElement;
  while (focused?.shadowRoot?.activeElement)
    focused = focused.shadowRoot.activeElement;
  return (
    focused instanceof HTMLElement &&
    (focused.matches(guardSelector) ||
      focused.isContentEditable ||
      (focused.localName.includes("-") && !focused.shadowRoot))
  );
}
function topOffset(): { point: Point; path: string[] } {
  let current: Window = window;
  let x = 0,
    y = 0;
  const path: string[] = [];
  try {
    while (current !== current.top) {
      const frame = current.frameElement;
      if (!(frame instanceof HTMLElement))
        return { point: { x, y }, path: ["unresolved:cross-origin", ...path] };
      const r = frame.getBoundingClientRect();
      x += r.x + frame.clientLeft;
      y += r.y + frame.clientTop;
      path.unshift(selector(frame));
      current = current.parent;
    }
  } catch {
    return { point: { x, y }, path: ["unresolved:cross-origin", ...path] };
  }
  return { point: { x, y }, path };
}
function topPoint(point: Point): Point {
  const offset = topOffset().point;
  return { x: point.x + offset.x, y: point.y + offset.y };
}
function box(rect: Rect): void {
  mount();
  if (!outline) return;
  outline.hidden = false;
  for (const [key, value] of Object.entries(rect))
    outline.style.setProperty(`--zuruf-${key}`, `${value}px`);
}
function under(): Element | null {
  let element = document.elementFromPoint(cursor.x, cursor.y);
  if (element === host) return null;
  while (element?.shadowRoot) {
    const next = element.shadowRoot.elementFromPoint(cursor.x, cursor.y);
    if (!next || next === element) break;
    element = next;
  }
  return element;
}
function selector(element: Element): string {
  if (element.id) return "#" + CSS.escape(element.id);
  const parts: string[] = [];
  let node: Element | null = element;
  while (node) {
    const siblings = Array.from(node.parentElement?.children ?? []).filter(
      (n) => n.localName === node?.localName,
    );
    parts.unshift(
      `${node.localName}:nth-of-type(${Math.max(1, siblings.indexOf(node) + 1)})`,
    );
    node = node.parentElement;
  }
  return parts.join(" > ");
}
function domCapture(): DomCapture | null {
  const element = under();
  if (!element) return null;
  const sensitive =
    element.matches(guardSelector) ||
    element.closest("[contenteditable]") !== null;
  const text = sensitive
    ? ""
    : (element instanceof HTMLElement
        ? element.innerText
        : (element.textContent ?? "")
      )
        .trim()
        .slice(0, 500);
  const attributes: Record<string, string> = {};
  for (const attribute of element.attributes) {
    if (
      /^(id|class|role|type|data-testid|aria-label|aria-labelledby|aria-describedby)$/.test(
        attribute.name,
      )
    )
      attributes[attribute.name] = attribute.value.slice(0, 200);
  }
  const locators: LocatorCandidate[] = [];
  const add = (
    kind: LocatorCandidate["kind"],
    value: string,
    playwright: string,
    stable: boolean,
    reason: string,
  ): void => {
    locators.push({ kind, value, playwright, stable, reason });
  };
  const testid = element.getAttribute("data-testid");
  const root = element.getRootNode() as Document | ShadowRoot;
  if (testid) {
    const count = root.querySelectorAll(
      `[data-testid=${JSON.stringify(testid)}]`,
    ).length;
    add(
      "testid",
      testid,
      `getByTestId(${JSON.stringify(testid)})`,
      count === 1,
      count === 1 ? "Explicit unique test contract" : "Duplicate test ID",
    );
  }
  // Role/name are added by Rust after CDP Accessibility validation.
  if (
    text &&
    Array.from(root.querySelectorAll("*")).filter(
      (e) => e.textContent?.trim() === text,
    ).length === 1
  )
    add(
      "text",
      text,
      `getByText(${JSON.stringify(text)}, { exact: true })`,
      false,
      "Currently unique text; may change",
    );
  const css = selector(element);
  add(
    "css",
    css,
    `locator(${JSON.stringify(css)})`,
    false,
    "ID or structural position may change",
  );
  const shadowPath: string[] = [];
  let node: Element = element;
  while (node.getRootNode() instanceof ShadowRoot) {
    node = (node.getRootNode() as ShadowRoot).host;
    shadowPath.unshift(selector(node));
  }
  if (shadowPath.length === 0) {
    const xpath =
      "/" +
      css
        .split(" > ")
        .map((s) => s.replace(/:nth-of-type\((\d+)\)/, "[$1]"))
        .join("/");
    if (!element.id)
      add(
        "xpath",
        xpath,
        `locator(${JSON.stringify("xpath=" + xpath)})`,
        false,
        "Positional fallback",
      );
  }
  const local = element.getBoundingClientRect();
  const { point, path } = topOffset();
  return {
    tag: element.localName,
    attributes,
    text,
    locators,
    boundingBox: {
      x: local.x + point.x,
      y: local.y + point.y,
      width: local.width,
      height: local.height,
    },
    framePath: path,
    shadowPath,
    parent: element.parentElement?.localName,
    children: Array.from(element.children)
      .slice(0, 8)
      .map((e) => e.localName),
  };
}
function cancel(): void {
  region = false;
  origin = undefined;
  if (outline) outline.hidden = true;
}
function configure(payload: { review: boolean; bindings: Binding[] }): void {
  review = payload.review;
  bindings = payload.bindings;
  mount();
  renderStatus();
  if (!review) cancel();
}
function receive(message: Incoming): Reply {
  if (message.type === "configure") configure(message.payload);
  if (message.type === "overlay.hide") {
    hidden = true;
    if (overlay) overlay.hidden = true;
    return { hidden: true };
  }
  if (message.type === "overlay.show") {
    hidden = false;
    if (overlay) overlay.hidden = false;
  }
  if (message.type === "dom.request")
    return { dom: domCapture(), cursor: topPoint(cursor) };
  if (message.type === "capture.failed" && isTop) {
    mount();
    if (confirmation) {
      const failed = confirmation;
      failed.hidden = false;
      failed.dataset.kind = "error";
      failed.textContent = message.payload.reason;
      setTimeout(() => {
        if (failed.dataset.kind === "error") failed.hidden = true;
      }, 3200);
    }
  }
  if (message.type === "capture.saved" && isTop) {
    mount();
    if (confirmation) {
      confirmation.hidden = false;
      confirmation.dataset.kind = "saved";
      confirmation.textContent = "Gespeichert";
      // A second frame callback follows a rendering opportunity for the confirmation.
      requestAnimationFrame(() =>
        requestAnimationFrame(() => {
          window.dispatchEvent(
            new CustomEvent("zuruf:capture-confirmed", {
              detail: performance.timeOrigin + performance.now(),
            }),
          );
        }),
      );
      setTimeout(() => {
        if (confirmation?.dataset.kind === "saved") confirmation.hidden = true;
      }, 900);
    }
  }
  if (message.type === "status") {
    recording = message.payload.recording;
    mount();
    renderStatus();
  }
  if (message.type === "control.changed") {
    owner = message.payload.owner;
    mount();
    renderStatus();
  }
  return { ok: true };
}
// The app hears about the pointer at most every 32 ms, but always about where it came to rest.
const pointerInterval = 32;
let lastPointer = 0;
let pointerTimer: ReturnType<typeof setTimeout> | undefined;
function reportPointer(): void {
  pointerTimer = undefined;
  lastPointer = performance.now();
  send({ type: "pointer", payload: { cursor: topPoint(cursor) } });
}
window.addEventListener(
  "pointermove",
  (event) => {
    if (!event.isTrusted) return;
    cursor = { x: event.clientX, y: event.clientY };
    const wait = pointerInterval - (performance.now() - lastPointer);
    if (wait <= 0) reportPointer();
    else pointerTimer ??= setTimeout(reportPointer, wait);
    if (!review) return;
    if (origin)
      box({
        x: Math.min(cursor.x, origin.x),
        y: Math.min(cursor.y, origin.y),
        width: Math.abs(cursor.x - origin.x),
        height: Math.abs(cursor.y - origin.y),
      });
    else {
      const element = under();
      if (element) box(element.getBoundingClientRect());
    }
  },
  true,
);
window.addEventListener(
  "pointerdown",
  (event) => {
    if (!event.isTrusted || !region) return;
    origin = { x: event.clientX, y: event.clientY };
    event.preventDefault();
    event.stopImmediatePropagation();
  },
  true,
);
window.addEventListener(
  "pointerup",
  (event) => {
    if (!event.isTrusted || !origin) return;
    cursor = { x: event.clientX, y: event.clientY };
    const start = topPoint(origin),
      end = topPoint(cursor);
    const rect = {
      x: Math.min(start.x, end.x),
      y: Math.min(start.y, end.y),
      width: Math.abs(end.x - start.x),
      height: Math.abs(end.y - start.y),
    };
    cancel();
    if (rect.width > 0 && rect.height > 0)
      send({
        type: "capture.request",
        payload: { kind: "region", cursor: end, rect, dom: null },
      });
    event.preventDefault();
    event.stopImmediatePropagation();
  },
  true,
);
window.addEventListener(
  "click",
  (event) => {
    if (region) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  },
  true,
);
function matches(
  event: KeyboardEvent,
  binding: Binding,
  withAlt: boolean,
): boolean {
  const chord = binding.chord;
  if (binding.scope !== "page" || !chord || chord.code !== event.code)
    return false;
  const strong = chord.mods.some((mod) => mod !== "shift");
  if (withAlt && strong) return false;
  return (
    event.ctrlKey === chord.mods.includes("ctrl") &&
    event.altKey === (withAlt || chord.mods.includes("alt")) &&
    event.shiftKey === chord.mods.includes("shift") &&
    event.metaKey === chord.mods.includes("super")
  );
}
/**
 * The binding a key press means. A chord with Ctrl, Alt or Super is ours everywhere. A bare key
 * is ours only outside text fields; held with Alt it reaches the same action inside them too.
 */
function bindingFor(event: KeyboardEvent): Binding | undefined {
  const exact = bindings.find((b) => matches(event, b, false));
  if (exact) {
    const strong = event.ctrlKey || event.altKey || event.metaKey;
    return strong || !guarded(event) ? exact : undefined;
  }
  return event.altKey
    ? bindings.find((b) => matches(event, b, true))
    : undefined;
}
// Keys we took: the page must not see their repeats, keypress or keyup either.
const taken = new Set<string>();
function swallow(event: KeyboardEvent): void {
  event.preventDefault();
  event.stopImmediatePropagation();
}
window.addEventListener(
  "keyup",
  (event) => {
    if (event.isTrusted && taken.delete(event.code)) swallow(event);
  },
  true,
);
window.addEventListener(
  "keypress",
  (event) => {
    if (event.isTrusted && taken.has(event.code)) swallow(event);
  },
  true,
);
window.addEventListener("blur", () => taken.clear());
window.addEventListener(
  "keydown",
  (event) => {
    if (!event.isTrusted) return;
    if (taken.has(event.code)) {
      swallow(event);
      return;
    }
    if (!review || event.repeat || event.isComposing || event.keyCode === 229)
      return;
    const binding = bindingFor(event);
    if (!binding) return;
    taken.add(event.code);
    event.preventDefault();
    event.stopImmediatePropagation();
    const action = binding.action;
    if (
      action === "capture.viewport" ||
      action === "capture.dom" ||
      action === "capture.region"
    ) {
      window.dispatchEvent(
        new CustomEvent("zuruf:capture-start", {
          detail: performance.timeOrigin + event.timeStamp,
        }),
      );
    }
    if (action === "action.cancel") {
      cancel();
      return;
    }
    if (action === "capture.region") {
      // The capture is requested once a region has been dragged.
      region = true;
      return;
    }
    // Captures travel as a request with their geometry; everything else is a plain action.
    if (action !== "capture.viewport" && action !== "capture.dom")
      send({ type: "action", payload: { action } });
    if (action === "capture.viewport" || action === "capture.dom")
      send({
        type: "capture.request",
        payload: {
          kind: action === "capture.dom" ? "dom" : "viewport",
          cursor: topPoint(cursor),
          rect: null,
          dom: action === "capture.dom" ? domCapture() : null,
        },
      });
  },
  true,
);
/** The browser does not announce title changes of a tab, so its top document does. */
function watchTitle(): void {
  let reported: string | undefined;
  const report = (): void => {
    if (document.title === reported) return;
    reported = document.title;
    send({ type: "title", payload: { title: reported } });
  };
  const observe = (): void => {
    report();
    // The title lives in the head; watching only that keeps busy pages cheap.
    if (document.head)
      new MutationObserver(report).observe(document.head, {
        childList: true,
        characterData: true,
        subtree: true,
      });
  };
  if (document.readyState === "loading")
    document.addEventListener("DOMContentLoaded", observe, { once: true });
  else observe();
  window.addEventListener("load", report, { once: true });
}
function start(): void {
  globalThis.__zurufReceive = receive;
  if (document.documentElement) mount();
  else
    new MutationObserver((_, observer) => {
      if (document.documentElement) {
        mount();
        observer.disconnect();
      }
    }).observe(document, { childList: true });
  const visible = (): boolean => document.visibilityState === "visible";
  if (window === window.top) {
    document.addEventListener("visibilitychange", () => {
      send({ type: "visibility", payload: { visible: visible() } });
    });
    watchTitle();
  }
  // The app answers with the current configuration, recording state and control owner.
  send({
    type: "ready",
    payload: { top: window === window.top, visible: visible() },
  });
}
start();
