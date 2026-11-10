import type {
  Binding,
  DomCapture,
  LocatorCandidate,
  Rect,
} from "../../../packages/protocol/src/index";
import type { Message, Point, Envelope } from "./protocol";
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
function renderStatus(): void {
  if (status) {
    status.hidden = !review;
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
  const sheet = document.createElement("link");
  sheet.rel = "stylesheet";
  sheet.href = chrome.runtime.getURL("overlay.css");
  root.append(sheet);
  overlay = document.createElement("div");
  overlay.className = "overlay";
  overlay.hidden = hidden;
  outline = document.createElement("div");
  outline.className = "outline";
  outline.hidden = true;
  status = document.createElement("div");
  status.className = "status";
  status.hidden = !review;
  status.textContent = "Review";
  confirmation = document.createElement("div");
  confirmation.className = "confirmation";
  confirmation.hidden = true;
  overlay.append(outline, status, confirmation);
  root.append(overlay);
  document.documentElement.append(host);
}
function send(message: Message): void {
  void chrome.runtime.sendMessage(message).catch(() => {});
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
chrome.runtime.onMessage.addListener((message: Envelope, _, reply) => {
  if (message.type === "configure") configure(message.payload);
  if (message.type === "overlay.hide") {
    hidden = true;
    if (overlay) overlay.hidden = true;
    reply({ hidden: true });
    return;
  }
  if (message.type === "overlay.show") {
    hidden = false;
    if (overlay) overlay.hidden = false;
  }
  if (message.type === "dom.request") {
    reply({ dom: domCapture(), cursor: topPoint(cursor) });
    return;
  }
  if (message.type === "capture.failed") {
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
  if (message.type === "capture.saved") {
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
  reply({ ok: true });
});
let lastPointer = 0;
window.addEventListener(
  "pointermove",
  (event) => {
    if (!event.isTrusted) return;
    cursor = { x: event.clientX, y: event.clientY };
    if (performance.now() - lastPointer > 32) {
      lastPointer = performance.now();
      send({ type: "pointer", payload: { cursor: topPoint(cursor) } });
    }
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
function matches(event: KeyboardEvent, binding: Binding): boolean {
  const chord = binding.chord;
  if (binding.scope !== "page" || !chord || chord.code !== event.code)
    return false;
  return (
    event.ctrlKey === chord.mods.includes("ctrl") &&
    event.altKey === chord.mods.includes("alt") &&
    event.shiftKey === chord.mods.includes("shift") &&
    event.metaKey === chord.mods.includes("super")
  );
}
window.addEventListener(
  "keydown",
  (event) => {
    if (
      !event.isTrusted ||
      !review ||
      event.repeat ||
      event.isComposing ||
      event.keyCode === 229 ||
      guarded(event)
    )
      return;
    const binding = bindings.find((b) => matches(event, b));
    if (!binding) return;
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
    // This also permits the worker to open the panel in this user gesture.
    send({ type: "action", payload: { action } });
    if (action === "capture.region") {
      region = true;
      return;
    }
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
if (document.documentElement) mount();
else
  new MutationObserver((_, observer) => {
    if (document.documentElement) {
      mount();
      observer.disconnect();
    }
  }).observe(document, { childList: true });
void chrome.runtime
  .sendMessage({ type: "ready" })
  .then(
    (
      reply: { payload: { review: boolean; bindings: Binding[] } } | undefined,
    ) => {
      if (reply) configure(reply.payload);
    },
  )
  .catch(() => {});
