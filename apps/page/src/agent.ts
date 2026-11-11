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
/**
 * What the next click or drag selects. A viewport capture has nothing to choose and is taken at
 * once; an element or a region is picked first.
 */
type Mode = "none" | "element" | "region";
let mode: Mode = "none";
let origin: Point | undefined;
let host: HTMLElement | undefined;
let overlay: HTMLElement | undefined;
let outline: HTMLElement | undefined;
let tag: HTMLElement | undefined;
let shade: HTMLElement | undefined;
let hint: HTMLElement | undefined;
let status: HTMLElement | undefined;
let confirmation: HTMLElement | undefined;
let agentCursor: HTMLElement | undefined;
let agentNote: HTMLElement | undefined;
let agentMark: HTMLElement | undefined;
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
  const part = (name: string): HTMLElement => {
    const element = document.createElement("div");
    element.className = name;
    element.hidden = true;
    return element;
  };
  // The shade covers the page while a region is drawn; it is the only part that takes clicks.
  shade = part("shade");
  outline = part("outline");
  tag = document.createElement("span");
  tag.className = "tag";
  outline.append(tag);
  hint = part("hint");
  agentMark = part("agent-mark");
  agentMark.append(document.createElement("span"));
  agentCursor = part("agent-cursor");
  agentCursor.textContent = "Agent";
  agentNote = part("agent-note");
  status = document.createElement("div");
  status.className = "status";
  status.hidden = !review || !isTop;
  status.textContent = "Review";
  confirmation = document.createElement("div");
  confirmation.className = "confirmation";
  confirmation.hidden = true;
  overlay.append(
    shade,
    outline,
    agentMark,
    agentCursor,
    hint,
    agentNote,
    status,
    confirmation,
  );
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
function place(element: HTMLElement, rect: Rect): void {
  // Named one by one: a DOMRect keeps its values on the prototype, where enumeration finds none.
  element.style.setProperty("--zuruf-x", `${rect.x}px`);
  element.style.setProperty("--zuruf-y", `${rect.y}px`);
  element.style.setProperty("--zuruf-width", `${rect.width}px`);
  element.style.setProperty("--zuruf-height", `${rect.height}px`);
}
/** Draw the selection with a short description of what it holds. */
function box(rect: Rect, label: string): void {
  mount();
  if (!outline || !tag) return;
  outline.hidden = false;
  place(outline, rect);
  tag.textContent = label;
  // Near the top edge the label moves inside, so it stays readable.
  tag.dataset.inside = String(rect.y < 22);
}
function under(): Element | null {
  // Our own shade lies on top while a region is drawn; look through it.
  let element =
    document
      .elementsFromPoint(cursor.x, cursor.y)
      .find((candidate) => candidate !== host) ?? null;
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
const HINTS: Record<Mode, string> = {
  none: "",
  element: "Element wählen – Klick nimmt auf · Esc bricht ab",
  region: "Bereich aufziehen · Esc bricht ab",
};
/**
 * Enter or leave a selection. Every frame of the tab follows, because the key may be pressed in
 * one frame while the pointer is over another.
 */
function setMode(next: Mode, announce: boolean): void {
  mode = next;
  origin = undefined;
  mount();
  if (overlay) {
    overlay.dataset.mode = next;
    overlay.dataset.dragging = "false";
  }
  if (outline) outline.hidden = true;
  // A region is drawn in the tab's own coordinates, so only the top document shades.
  if (shade) shade.hidden = !(next === "region" && isTop);
  if (hint) {
    hint.hidden = next === "none" || !isTop;
    hint.textContent = HINTS[next];
  }
  if (next === "element") highlight();
  if (announce) send({ type: "mode", payload: { mode: next } });
}
// Whether the pointer is over this document; in a tab with frames it is over one of them.
let pointerHere = false;
/** Mark the element under the pointer as the one a click would capture. */
function highlight(): void {
  const element = pointerHere ? under() : null;
  if (!element) {
    if (outline) outline.hidden = true;
    return;
  }
  const rect = element.getBoundingClientRect();
  box(
    rect,
    `${element.localName} · ${Math.round(rect.width)} × ${Math.round(rect.height)}`,
  );
}
function captureElement(): void {
  const dom = domCapture();
  setMode("none", true);
  if (dom)
    send({
      type: "capture.request",
      payload: { kind: "dom", cursor: topPoint(cursor), rect: null, dom },
    });
}
function configure(payload: { review: boolean; bindings: Binding[] }): void {
  review = payload.review;
  bindings = payload.bindings;
  mount();
  renderStatus();
  if (!review && mode !== "none") setMode("none", false);
}

// What an agent shows the person at the browser: where it acts, what it is doing, and what it
// wants them to look at. Only the tab's top document draws these.
let agentCursorTimer: ReturnType<typeof setTimeout> | undefined;
function showAgentPointer(point: Point, pressed: boolean): void {
  mount();
  if (!agentCursor || !overlay) return;
  agentCursor.hidden = false;
  agentCursor.style.setProperty("--zuruf-x", `${point.x}px`);
  agentCursor.style.setProperty("--zuruf-y", `${point.y}px`);
  if (pressed) {
    const ripple = document.createElement("div");
    ripple.className = "agent-ripple";
    ripple.style.setProperty("--zuruf-x", `${point.x}px`);
    ripple.style.setProperty("--zuruf-y", `${point.y}px`);
    overlay.append(ripple);
    setTimeout(() => ripple.remove(), 600);
  }
  clearTimeout(agentCursorTimer);
  agentCursorTimer = setTimeout(() => {
    if (agentCursor) agentCursor.hidden = true;
  }, 4000);
}
let agentNoteTimer: ReturnType<typeof setTimeout> | undefined;
function showAgentNote(text: string): void {
  mount();
  if (!agentNote) return;
  agentNote.hidden = false;
  agentNote.textContent = text;
  clearTimeout(agentNoteTimer);
  agentNoteTimer = setTimeout(() => {
    if (agentNote) agentNote.hidden = true;
  }, 12000);
}
let marked: Element | undefined;
let markTimer: ReturnType<typeof setTimeout> | undefined;
function clearAgentMark(): void {
  marked = undefined;
  clearTimeout(markTimer);
  if (agentMark) agentMark.hidden = true;
}
/** Keep the mark on its element while the page scrolls or reflows. */
function followMark(): void {
  if (!marked || !agentMark) return;
  if (!marked.isConnected) {
    clearAgentMark();
    return;
  }
  place(agentMark, marked.getBoundingClientRect());
  requestAnimationFrame(followMark);
}
function showAgentMark(selectorText: string, text: string): boolean {
  mount();
  let element: Element | null = null;
  try {
    element = document.querySelector(selectorText);
  } catch {
    return false;
  }
  if (!element || !agentMark) return false;
  element.scrollIntoView({ block: "center", inline: "nearest" });
  marked = element;
  agentMark.hidden = false;
  const caption = agentMark.firstElementChild;
  if (caption) caption.textContent = text;
  followMark();
  clearTimeout(markTimer);
  markTimer = setTimeout(clearAgentMark, 30000);
  return true;
}

function receive(message: Incoming): Reply {
  if (message.type === "configure") configure(message.payload);
  if (message.type === "mode") setMode(message.payload.mode, false);
  if (message.type === "agent.pointer" && isTop)
    showAgentPointer(message.payload, message.payload.pressed);
  if (message.type === "agent.note" && isTop)
    showAgentNote(message.payload.text);
  if (message.type === "agent.point")
    return {
      found: isTop && showAgentMark(message.payload.selector, message.payload.text),
    };
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
/** While something is being selected, the page must not react to the pointer. */
function keepFromPage(event: Event): void {
  event.preventDefault();
  event.stopImmediatePropagation();
}
// The click that ends a selection arrives after the selection is over; it is still ours.
let ignoreClicksUntil = 0;
window.addEventListener(
  "pointermove",
  (event) => {
    if (!event.isTrusted) return;
    cursor = { x: event.clientX, y: event.clientY };
    pointerHere = true;
    const wait = pointerInterval - (performance.now() - lastPointer);
    if (wait <= 0) reportPointer();
    else pointerTimer ??= setTimeout(reportPointer, wait);
    if (mode === "element") highlight();
    if (mode === "region" && origin) {
      const width = Math.abs(cursor.x - origin.x);
      const height = Math.abs(cursor.y - origin.y);
      box(
        {
          x: Math.min(cursor.x, origin.x),
          y: Math.min(cursor.y, origin.y),
          width,
          height,
        },
        `${Math.round(width)} × ${Math.round(height)}`,
      );
    }
    if (mode !== "none") keepFromPage(event);
  },
  true,
);
window.addEventListener(
  "pointerout",
  (event) => {
    // The pointer left this document: nothing here is under it any more.
    if (event.relatedTarget) return;
    pointerHere = false;
    if (mode === "element" && outline) outline.hidden = true;
  },
  true,
);
window.addEventListener(
  "pointerdown",
  (event) => {
    if (!event.isTrusted) return;
    // Any click of the person dismisses what an agent pointed out.
    if (marked) clearAgentMark();
    if (mode === "none") return;
    keepFromPage(event);
    if (mode === "region" && isTop) {
      origin = { x: event.clientX, y: event.clientY };
      if (overlay) overlay.dataset.dragging = "true";
    }
  },
  true,
);
window.addEventListener(
  "pointerup",
  (event) => {
    if (!event.isTrusted || mode === "none") return;
    keepFromPage(event);
    ignoreClicksUntil = performance.now() + 400;
    cursor = { x: event.clientX, y: event.clientY };
    if (mode === "element") {
      captureElement();
      return;
    }
    if (!origin) return;
    const start = topPoint(origin),
      end = topPoint(cursor);
    const rect = {
      x: Math.min(start.x, end.x),
      y: Math.min(start.y, end.y),
      width: Math.abs(end.x - start.x),
      height: Math.abs(end.y - start.y),
    };
    // A click without a drag selects nothing; the person can try again.
    if (rect.width < 4 || rect.height < 4) {
      origin = undefined;
      if (outline) outline.hidden = true;
      if (overlay) overlay.dataset.dragging = "false";
      return;
    }
    setMode("none", true);
    send({
      type: "capture.request",
      payload: { kind: "region", cursor: end, rect, dom: null },
    });
  },
  true,
);
for (const type of ["mousedown", "mouseup", "click", "auxclick", "contextmenu"])
  window.addEventListener(
    type,
    (event) => {
      if (!event.isTrusted) return;
      if (mode !== "none" || performance.now() < ignoreClicksUntil)
        keepFromPage(event);
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
    // During a selection Enter takes the highlighted element, whatever the bindings say.
    if (mode === "element" && event.code === "Enter") {
      taken.add(event.code);
      swallow(event);
      captureElement();
      return;
    }
    const binding = bindingFor(event);
    if (!binding) return;
    const action = binding.action;
    if (action === "action.cancel") {
      // Only ours while there is something to cancel; otherwise the page keeps its Escape.
      if (mode === "none" && !marked) return;
      taken.add(event.code);
      swallow(event);
      clearAgentMark();
      if (mode !== "none") setMode("none", true);
      return;
    }
    taken.add(event.code);
    swallow(event);
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
    if (action === "capture.dom" || action === "capture.region") {
      // Choose first: the same key again leaves the selection.
      const wanted: Mode = action === "capture.dom" ? "element" : "region";
      setMode(mode === wanted ? "none" : wanted, true);
      return;
    }
    if (mode !== "none") setMode("none", true);
    if (action === "capture.viewport") {
      // The whole view has nothing to choose: take it at once.
      send({
        type: "capture.request",
        payload: {
          kind: "viewport",
          cursor: topPoint(cursor),
          rect: null,
          dom: null,
        },
      });
      return;
    }
    send({ type: "action", payload: { action } });
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
