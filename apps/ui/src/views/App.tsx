import { For, Match, Show, Switch, createMemo, createSignal } from "solid-js";
import { Icon, type IconName } from "../components/basics";
import { Menu } from "../components/Menu";
import type { AppStore } from "../lib/store";
import { noticeMessage } from "../lib/text";
import { Panel } from "./Panel";
import { Sessions } from "./Sessions";
import { Settings, type Tab as SettingsTab } from "./Settings";
import { Setup } from "./Setup";
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
  const [settingsTab, setSettingsTab] = createSignal<SettingsTab | undefined>();
  const go = (next: View, tab?: SettingsTab) => {
    setSettingsTab(tab);
    setView(next);
    history.replaceState(null, "", `#${next}`);
  };

  // Until T3, a project and an agent are settled there is nothing to review or send.
  const needsSetup = createMemo(() => !store.activeProject() || !store.snapshot()?.profiles.length);
  const projects = () => store.snapshot()?.projects ?? [];

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
        <div class="rail-brand">
          <span class="mark" role="img" aria-label="Zuruf" />
          <Show when={store.activeProject()}>
            {(project) => (
              <Menu
                trigger={(toggle, open) => (
                  <button type="button" class="rail-project" aria-haspopup="menu" aria-expanded={open()} onClick={toggle}>
                    <span>{project().name}</span>
                    <Icon name="chevron" size={12} />
                  </button>
                )}
              >
                {(close) => (
                  <>
                    <For each={projects()}>
                      {(entry) => (
                        <button
                          type="button"
                          class="menu-item"
                          role="menuitemradio"
                          aria-checked={entry.id === project().id}
                          onClick={() => {
                            close();
                            if (entry.id !== project().id) void store.commands.setActive({ projectId: entry.id });
                          }}
                        >
                          <span class="menu-check">
                            <Show when={entry.id === project().id}>
                              <Icon name="check" size={12} />
                            </Show>
                          </span>
                          {entry.name}
                        </button>
                      )}
                    </For>
                    <button
                      type="button"
                      class="menu-item menu-item-quiet"
                      role="menuitem"
                      onClick={() => {
                        close();
                        go("settings", "projects");
                      }}
                    >
                      <span class="menu-check" />
                      Projekte verwalten
                    </button>
                  </>
                )}
              </Menu>
            )}
          </Show>
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
          <Match when={view() !== "settings" && needsSetup()}>
            <Setup store={store} />
          </Match>
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
            <Show when={settingsTab() ?? "shortcuts"} keyed>
              {(tab) => <Settings store={store} tab={tab} />}
            </Show>
          </Match>
        </Switch>
      </main>
      <Toasts store={store} />
    </div>
  );
}
