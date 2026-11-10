import { For, Match, Show, Switch, createMemo, createSignal } from "solid-js";
import { Icon, type IconName } from "../components/basics";
import type { AppStore } from "../lib/store";
import { noticeMessage } from "../lib/text";
import { Panel } from "./Panel";
import { Sessions } from "./Sessions";
import { Settings } from "./Settings";
import { Tasks } from "./Tasks";

type View = "review" | "tasks" | "sessions" | "settings";

const NAV: { id: View; label: string; icon: IconName }[] = [
  { id: "review", label: "Review", icon: "review" },
  { id: "tasks", label: "Aufgaben", icon: "tasks" },
  { id: "sessions", label: "Sitzungen", icon: "browser" },
  { id: "settings", label: "Einstellungen", icon: "settings" },
];

export function Toasts(props: { store: AppStore }) {
  return (
    <div class="toasts" role="status" aria-live="polite">
      <For each={props.store.state.toasts}>
        {(toast) => (
          <button
            type="button"
            class={`toast toast-${toast.notice.level}`}
            onClick={() => props.store.dismissToast(toast.id)}
          >
            {noticeMessage(toast.notice.code, toast.notice.detail)}
          </button>
        )}
      </For>
    </div>
  );
}

/** Shown instead of content while the daemon is unreachable. One line, no drama: it reconnects by itself. */
export function Offline(props: { store: AppStore }) {
  return (
    <Show when={props.store.state.ready && props.store.state.link === "offline"}>
      <div class="offline" role="status">
        Verbindung zum Dienst unterbrochen – verbinde neu …
      </div>
    </Show>
  );
}

export function App(props: { store: AppStore }) {
  const store = props.store;
  const [view, setView] = createSignal<View>((location.hash.slice(1) as View) || "tasks");
  const go = (next: View) => {
    setView(next);
    history.replaceState(null, "", `#${next}`);
  };

  // The only number in the chrome: how many tasks are waiting for the user.
  const waiting = createMemo(
    () =>
      store
        .tasks()
        .filter((task) =>
          ["needs_approval", "needs_review", "failed", "unknown_remote_state"].includes(task.status),
        ).length,
  );

  return (
    <div class="app">
      <nav class="rail" aria-label="Hauptnavigation">
        <div class="rail-brand" aria-label="Zuruf">
          <span class="mark" />
        </div>
        <For each={NAV}>
          {(entry) => (
            <button
              type="button"
              class="rail-item"
              aria-current={view() === entry.id ? "page" : undefined}
              onClick={() => go(entry.id)}
            >
              <Icon name={entry.icon} size={18} />
              <span>{entry.label}</span>
              <Show when={entry.id === "tasks" && waiting() > 0}>
                <span class="rail-count">{waiting()}</span>
              </Show>
            </button>
          )}
        </For>
      </nav>

      <main class="main">
        <Offline store={store} />
        <Switch>
          <Match when={view() === "review"}>
            <div class="review-stage">
              <Panel store={store} />
            </div>
          </Match>
          <Match when={view() === "tasks"}>
            <Tasks store={store} />
          </Match>
          <Match when={view() === "sessions"}>
            <Sessions store={store} />
          </Match>
          <Match when={view() === "settings"}>
            <Settings store={store} />
          </Match>
        </Switch>
      </main>
      <Toasts store={store} />
    </div>
  );
}
