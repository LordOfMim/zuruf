import type {
  Bootstrap,
  Envelope,
  ExtensionTab,
  Message,
  Point,
} from "./protocol";
import type { Binding } from "../../../packages/protocol/src/index";
let socket: WebSocket | undefined;
let bootstrap: Bootstrap;
let configuration: { review: boolean; bindings: Binding[] } = {
  review: false,
  bindings: [],
};
let pending: Record<string, Envelope> = {};
const pointerFrames = new Map<number, number>();
const liveFrames = new Map<number, Set<number>>();
const scriptable = new Set<number>();
const panelAttempted = new Set<number>();
const panelWindows = new Set<number>();
chrome.sidePanel.onOpened.addListener((info) => {
  panelWindows.add(info.windowId);
});
chrome.sidePanel.onClosed.addListener((info) => {
  panelWindows.delete(info.windowId);
});
let retry = 250;
function envelope(
  message: Message,
  tabId?: number,
  frameId?: number,
  documentId?: string,
): Envelope {
  return {
    ...message,
    v: 1,
    requestId: crypto.randomUUID(),
    sessionId: bootstrap.sessionId,
    timestamp: Date.now(),
    tabId,
    frameId,
    documentId,
  };
}
async function send(
  message: Message,
  tabId?: number,
  frameId?: number,
  documentId?: string,
): Promise<void> {
  const value = envelope(message, tabId, frameId, documentId);
  if (
    !(["heartbeat", "ack", "pointer", "tabs"] as string[]).includes(
      message.type,
    )
  ) {
    if (Object.keys(pending).length >= 1024) {
      if (tabId !== undefined)
        void chrome.tabs
          .sendMessage(tabId, {
            type: "capture.failed",
            payload: {
              reason: "Aufnahmepuffer voll – Verbindung zum Dienst prüfen.",
            },
          })
          .catch(() => {});
      return;
    }
    pending[value.requestId] = value;
    await chrome.storage.session.set({ pending });
  }
  if (socket?.readyState === WebSocket.OPEN) socket.send(JSON.stringify(value));
}
async function tabReport(): Promise<void> {
  const targets = await chrome.debugger.getTargets();
  const tabs = await chrome.tabs.query({});
  const result: ExtensionTab[] = [];
  for (const tab of tabs) {
    if (tab.id === undefined) continue;
    const target = targets.find((t) => t.tabId === tab.id && t.type === "page");
    if (!target) continue;
    result.push({
      tabId: tab.id,
      targetId: target.id,
      url: tab.url ?? "",
      title: tab.title ?? "",
      zoom: await chrome.tabs.getZoom(tab.id),
      active: tab.active,
      scriptable: scriptable.has(tab.id),
    });
  }
  await send({ type: "tabs", payload: { tabs: result } });
}
async function relay(value: Envelope): Promise<void> {
  if (value.type === "ack") {
    delete pending[value.payload.requestId];
    await chrome.storage.session.set({ pending });
    return;
  }
  if (value.type === "heartbeat") {
    await send({ type: "heartbeat" });
    return;
  }
  if (value.type === "configure") {
    configuration = value.payload;
    await chrome.storage.session.set({ configuration });
    for (const tab of await chrome.tabs.query({})) {
      if (tab.id !== undefined)
        void chrome.tabs.sendMessage(tab.id, value).catch(() => {});
    }
    return;
  }
  if (value.tabId === undefined) {
    for (const tab of await chrome.tabs.query({})) {
      if (tab.id !== undefined)
        void chrome.tabs.sendMessage(tab.id, value).catch(() => {});
    }
    return;
  }
  if (value.type === "overlay.hide") {
    const frames = liveFrames.get(value.tabId) ?? new Set([0]);
    // Require every live content frame to acknowledge. Never shoot on a partial hide.
    await Promise.all(
      [...frames].map((frameId) =>
        chrome.tabs.sendMessage(value.tabId!, value, { frameId }),
      ),
    );
    await send(
      { type: "overlay.hidden", payload: { requestId: value.requestId } },
      value.tabId,
    );
    return;
  }
  if (value.type === "dom.request") {
    const frameId = pointerFrames.get(value.tabId) ?? 0;
    const result: {
      dom: import("../../../packages/protocol/src/index").DomCapture | null;
      cursor: Point;
    } = await chrome.tabs.sendMessage(value.tabId, value, { frameId });
    await send(
      {
        type: "dom.result",
        payload: { requestId: value.requestId, ...result },
      },
      value.tabId,
      frameId,
    );
    return;
  }
  await chrome.tabs.sendMessage(
    value.tabId,
    value,
    value.frameId === undefined ? {} : { frameId: value.frameId },
  );
}
function connect(): void {
  socket = new WebSocket(`ws://127.0.0.1:${bootstrap.port}/ext`);
  socket.onopen = () => {
    retry = 250;
    socket?.send(JSON.stringify({ type: "auth", token: bootstrap.token }));
    void send({ type: "resume" });
    for (const message of Object.values(pending))
      socket?.send(JSON.stringify(message));
    void tabReport();
  };
  socket.onmessage = (event) => {
    void relay(JSON.parse(String(event.data)) as Envelope).catch((error) => {
      console.warn("Zuruf relay failed", String(error));
    });
  };
  socket.onclose = () => {
    setTimeout(connect, retry);
    retry = Math.min(retry * 2, 10000);
  };
  socket.onerror = () => socket?.close();
}
chrome.runtime.onMessage.addListener(
  (message: Message | { type: "ready" }, sender, reply) => {
    const tabId = sender.tab?.id;
    if (tabId === undefined || sender.id !== chrome.runtime.id) return;
    const frameId = sender.frameId ?? 0;
    if (message.type === "action") {
      // Call open directly in the user-gesture message handler, before any await.
      const windowId = sender.tab?.windowId;
      if (
        message.payload.action === "panel.toggle" &&
        windowId !== undefined &&
        panelWindows.has(windowId)
      ) {
        void chrome.sidePanel.close({ windowId }).catch(() => {});
      } else if (
        message.payload.action === "panel.toggle" ||
        (configuration.review && !panelAttempted.has(tabId))
      ) {
        panelAttempted.add(tabId);
        void chrome.sidePanel.open({ tabId }).catch(() => {});
      }
      if (message.payload.action.startsWith("capture.")) return;
    }
    if (message.type === "ready") {
      scriptable.add(tabId);
      void send({ type: "frame.ready" }, tabId, frameId, sender.documentId);
      let frames = liveFrames.get(tabId);
      if (!frames) {
        frames = new Set();
        liveFrames.set(tabId, frames);
      }
      frames.add(frameId);
      void chrome.storage.session.set({
        liveFrames: [...liveFrames].map(([tab, frames]) => [tab, [...frames]]),
        scriptable: [...scriptable],
      });
      reply({ type: "configure", payload: configuration });
      void tabReport();
      return;
    }
    if (message.type === "pointer") {
      if (pointerFrames.get(tabId) !== frameId) {
        pointerFrames.set(tabId, frameId);
        void chrome.storage.session.set({ pointerFrames: [...pointerFrames] });
      }
    }
    if (message.type === "capture.request") {
      void chrome.webNavigation.getFrame({ tabId, frameId }).then((current) => {
        if (current?.documentId !== sender.documentId) {
          void chrome.tabs
            .sendMessage(tabId, {
              type: "capture.failed",
              payload: {
                reason: "Seite wurde während der Aufnahme gewechselt.",
              },
            })
            .catch(() => {});
          return;
        }
        void send(message, tabId, frameId, sender.documentId);
      });
    } else void send(message, tabId, frameId, sender.documentId);
    return;
  },
);
chrome.tabs.onUpdated.addListener((id, change) => {
  if (change.status === "loading") {
    scriptable.delete(id);
    liveFrames.delete(id);
    pointerFrames.delete(id);
  }
  void tabReport();
});
chrome.tabs.onZoomChange.addListener(() => {
  void tabReport();
});
chrome.tabs.onActivated.addListener(() => {
  void tabReport();
});
chrome.tabs.onRemoved.addListener((id) => {
  scriptable.delete(id);
  liveFrames.delete(id);
  pointerFrames.delete(id);
  void tabReport();
});
chrome.webNavigation.onCommitted.addListener((details) => {
  if (details.frameId !== 0)
    liveFrames.get(details.tabId)?.delete(details.frameId);
});
async function start(): Promise<void> {
  bootstrap = (await (
    await fetch(chrome.runtime.getURL("bootstrap.json"))
  ).json()) as Bootstrap;
  const stored = await chrome.storage.session.get([
    "pending",
    "configuration",
    "pointerFrames",
    "liveFrames",
    "scriptable",
  ]);
  for (const [tab, frame] of (stored.pointerFrames ?? []) as [number, number][])
    pointerFrames.set(tab, frame);
  for (const [tab, frames] of (stored.liveFrames ?? []) as [number, number[]][])
    liveFrames.set(tab, new Set(frames));
  for (const tab of (stored.scriptable ?? []) as number[]) scriptable.add(tab);
  pending = (stored.pending as typeof pending | undefined) ?? {};
  configuration =
    (stored.configuration as typeof configuration | undefined) ?? configuration;
  await chrome.sidePanel.setPanelBehavior({ openPanelOnActionClick: true });
  await chrome.sidePanel.setOptions({
    enabled: true,
    path: "panel/panel.html",
  });
  connect();
}
void start();
